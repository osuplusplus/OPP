use std::{
    collections::BTreeMap,
    error::Error as _,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration as StdDuration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use keyring::{Entry, Error as KeyringError};
use rand_core::OsRng;
use reqwest::{
    Client, Method, Response, StatusCode,
    header::{ETAG, IF_NONE_MATCH},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;

use super::models::*;
use crate::{
    error::{CommandError, CommandResult},
    features::collections::{CollectionCandidate, CollectionSource},
    state::AppState,
};

pub const BASE_URL: &str = "https://beatmap-pack-hub.l1rics2006.workers.dev/api/v1";
const SERVICE: &str = "com.opp.desktop";
const PRIVATE_KEY_ENTRY: &str = "beatmaphub-ed25519-private-key";
const ACCESS_TOKEN_ENTRY: &str = "beatmaphub-access-token";

pub struct BeatmapHubService {
    client: Client,
    identity_path: PathBuf,
    identity: Mutex<Option<IdentityMetadata>>,
    auth_lock: AsyncMutex<()>,
    recommendations_cache_path: PathBuf,
    recommendations_cache: Mutex<Option<RecommendationsCache>>,
    pack_cache_path: PathBuf,
    pack_cache: Mutex<PackCache>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RecommendationsCache {
    updated_at: DateTime<Utc>,
    packs: Vec<Pack>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct PackCache {
    entries: BTreeMap<String, CachedPack>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CachedPack {
    etag: String,
    manifest_hash: String,
    cached_at: DateTime<Utc>,
    pack: Pack,
}

impl BeatmapHubService {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let directory = app_data_dir.join("beatmaphub");
        fs::create_dir_all(&directory)?;
        let identity_path = directory.join("identity.json");
        let recommendations_cache_path = directory.join("recommendations.json");
        let pack_cache_path = directory.join("packs.json");
        let identity = fs::read(&identity_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let mut client_builder = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            // Some Windows system proxies terminate Cloudflare HTTP/2 streams with an EOF.
            // Hub payloads are small, so HTTP/1.1 is the more compatible transport here.
            .http1_only()
            .user_agent(concat!("OPP/", env!("CARGO_PKG_VERSION")));
        if let Some(proxy_url) = discover_proxy_url() {
            let proxy = reqwest::Proxy::all(&proxy_url).map_err(|error| {
                CommandError::new(
                    "HUB_PROXY_ERROR",
                    format!("BeatmapHub 代理配置无效：{error}"),
                )
            })?;
            client_builder = client_builder.proxy(proxy);
        }
        let client = client_builder
            .build()
            .map_err(|error| CommandError::network(error.to_string()))?;
        Ok(Self {
            client,
            identity_path,
            identity: Mutex::new(identity),
            auth_lock: AsyncMutex::new(()),
            recommendations_cache: Mutex::new(read_recommendations_cache(
                &recommendations_cache_path,
            )),
            recommendations_cache_path,
            pack_cache: Mutex::new(read_pack_cache(&pack_cache_path)),
            pack_cache_path,
        })
    }

    pub fn status(&self) -> CommandResult<AuthStatus> {
        let identity = self.identity()?;
        let connected = identity
            .as_ref()
            .and_then(|value| value.expires_at)
            .is_some_and(|expiry| expiry > Utc::now())
            && read_secret(ACCESS_TOKEN_ENTRY)?.is_some();
        Ok(AuthStatus {
            has_identity: identity.is_some(),
            connected,
            public_key: identity.as_ref().map(|value| value.public_key.clone()),
            user_id: identity.as_ref().map(|value| value.user_id.clone()),
            device_id: identity.as_ref().map(|value| value.device_id.clone()),
            display_name: identity.as_ref().map(|value| value.display_name.clone()),
            device_name: identity
                .as_ref()
                .map(|value| value.device_name.clone())
                .or_else(default_device_name),
            expires_at: identity.and_then(|value| value.expires_at),
        })
    }

    pub async fn create_profile(
        &self,
        display_name: String,
        device_name: String,
    ) -> CommandResult<AuthStatus> {
        validate_name(&display_name, "显示名")?;
        validate_name(&device_name, "设备名")?;
        let signing = SigningKey::generate(&mut OsRng);
        let public_key = URL_SAFE_NO_PAD.encode(signing.verifying_key().as_bytes());
        let message = handshake_message(&public_key, display_name.trim(), device_name.trim());
        let signature = URL_SAFE_NO_PAD.encode(signing.sign(message.as_bytes()).to_bytes());
        let session: SessionResponse = self
            .request_json(
                Method::POST,
                "/auth/handshake",
                Some(json!({
                    "public_key": public_key, "display_name": display_name.trim(),
                    "device_name": device_name.trim(), "signature": signature,
                })),
                None,
            )
            .await?;
        self.commit_identity(signing, public_key, session)?;
        self.status()
    }

    pub async fn link_device(
        &self,
        link_token: String,
        device_name: String,
    ) -> CommandResult<AuthStatus> {
        validate_name(&device_name, "设备名")?;
        if !is_token(&link_token) {
            return Err(CommandError::new("INVALID_DEVICE_LINK", "链接码格式无效"));
        }
        let signing = SigningKey::generate(&mut OsRng);
        let public_key = URL_SAFE_NO_PAD.encode(signing.verifying_key().as_bytes());
        let message = device_link_message(link_token.trim(), &public_key, device_name.trim());
        let signature = URL_SAFE_NO_PAD.encode(signing.sign(message.as_bytes()).to_bytes());
        let session: SessionResponse = self
            .request_json(
                Method::POST,
                "/auth/devices/link",
                Some(json!({
                    "link_token": link_token.trim(), "public_key": public_key,
                    "device_name": device_name.trim(), "signature": signature,
                })),
                None,
            )
            .await?;
        self.commit_identity(signing, public_key, session)?;
        self.status()
    }

    pub async fn login(&self) -> CommandResult<AuthStatus> {
        let _ = self.ensure_session(true).await?;
        self.status()
    }

    pub async fn logout(&self) -> CommandResult<()> {
        if let Some(token) = read_secret(ACCESS_TOKEN_ENTRY)? {
            let result = self
                .request_empty(Method::POST, "/auth/logout", None, Some(&token))
                .await;
            if let Err(error) = &result
                && !matches!(error.code.as_str(), "INVALID_SESSION" | "AUTH_REQUIRED")
            {
                return result;
            }
        }
        delete_secret(ACCESS_TOKEN_ENTRY)?;
        self.update_identity(|identity| identity.expires_at = None)
    }

    pub async fn profile(&self) -> CommandResult<Profile> {
        self.auth_json(Method::GET, "/auth/me", None).await
    }

    pub async fn create_device_link(&self) -> CommandResult<LinkTokenResponse> {
        self.auth_json(Method::POST, "/auth/device-links", None)
            .await
    }

    pub async fn revoke_device(&self, device_id: &str) -> CommandResult<()> {
        self.auth_empty(Method::DELETE, &format!("/auth/devices/{device_id}"), None)
            .await
    }

    pub async fn get_pack(&self, share_id: &str) -> CommandResult<Pack> {
        let id = normalize_share_id(share_id)?;
        if self.identity()?.is_some() {
            self.auth_json(Method::GET, &format!("/packs/{id}"), None)
                .await
        } else {
            self.get_anonymous_pack(&id).await
        }
    }

    async fn get_anonymous_pack(&self, id: &str) -> CommandResult<Pack> {
        let cached = self.cached_pack(id)?;
        // The hash endpoint is intentionally checked first.  It avoids downloading
        // the full pack (and its viewer-independent metadata) when our local copy
        // still represents the same ordered beatmapset manifest.
        let mut hash_request = self.client.get(format!("{BASE_URL}/packs/{id}/hash"));
        if let Some(cache) = &cached {
            hash_request =
                hash_request.header(IF_NONE_MATCH, format!("\"{}\"", cache.manifest_hash));
        }
        let hash_response = hash_request.send().await.map_err(|error| {
            CommandError::network(format!(
                "无法连接 BeatmapHub：{}",
                reqwest_error_details(&error)
            ))
        });
        let hash_response = match hash_response {
            Ok(response) => response,
            Err(error) => return cached.map(|cache| cache.pack).ok_or(error),
        };
        if hash_response.status() == StatusCode::NOT_MODIFIED {
            return cached.map(|cache| cache.pack).ok_or_else(|| {
                CommandError::new("HUB_CACHE_ERROR", "服务器返回了无对应内容的缓存验证结果")
            });
        }
        if !hash_response.status().is_success() {
            return Err(response_error(hash_response).await);
        }
        #[derive(serde::Deserialize)]
        struct ManifestResponse {
            manifest_hash: String,
        }
        let remote_manifest_hash = hash_response
            .json::<ManifestResponse>()
            .await
            .map_err(|error| {
                CommandError::new(
                    "INVALID_HUB_RESPONSE",
                    format!("BeatmapHub hash 响应格式无效：{error}"),
                )
            })?
            .manifest_hash;
        if let Some(cache) = cached
            .as_ref()
            .filter(|cache| cache.manifest_hash == remote_manifest_hash)
        {
            return Ok(cache.pack.clone());
        }

        let mut request = self.client.get(format!("{BASE_URL}/packs/{id}"));
        if let Some(cache) = &cached {
            request = request.header(IF_NONE_MATCH, &cache.etag);
        }
        let response = request.send().await.map_err(|error| {
            CommandError::network(format!(
                "无法连接 BeatmapHub：{}",
                reqwest_error_details(&error)
            ))
        });
        let response = match response {
            Ok(response) => response,
            Err(error) => return cached.map(|cache| cache.pack).ok_or(error),
        };
        if response.status() == StatusCode::NOT_MODIFIED {
            return cached.map(|cache| cache.pack).ok_or_else(|| {
                CommandError::new("HUB_CACHE_ERROR", "服务器返回了无对应内容的缓存验证结果")
            });
        }
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }

        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let manifest_hash = response
            .headers()
            .get("x-beatmap-manifest-hash")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let pack: Pack = response.json().await.map_err(|error| {
            CommandError::new(
                "INVALID_HUB_RESPONSE",
                format!("BeatmapHub 响应格式无效：{error}"),
            )
        })?;
        if pack.manifest_hash != remote_manifest_hash {
            return Err(CommandError::new(
                "HUB_CACHE_ERROR",
                "BeatmapHub 返回的曲包 hash 前后不一致",
            ));
        }
        if let (Some(etag), Some(manifest_hash)) = (etag, manifest_hash)
            && manifest_hash == pack.manifest_hash
        {
            self.store_cached_pack(
                id,
                CachedPack {
                    etag,
                    manifest_hash,
                    cached_at: Utc::now(),
                    pack: pack.clone(),
                },
            );
        }
        Ok(pack)
    }

    pub async fn recommendations(
        &self,
        limit: u8,
        force_refresh: bool,
    ) -> CommandResult<Vec<Pack>> {
        let limit = limit.clamp(1, 50);
        if !force_refresh {
            let cached = self
                .recommendations_cache
                .lock()
                .map_err(|_| CommandError::new("HUB_CACHE_ERROR", "BeatmapHub 缓存不可用"))?;
            if let Some(cache) = cached.as_ref() {
                return Ok(cache.packs.iter().take(limit as usize).cloned().collect());
            }
        }
        #[derive(serde::Deserialize)]
        struct RecommendationResponse {
            packs: Vec<Pack>,
        }
        let result: RecommendationResponse = match self
            .request_json(
                Method::GET,
                &format!("/packs/recommendations?limit={limit}"),
                None,
                None,
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                let cached = self
                    .recommendations_cache
                    .lock()
                    .map_err(|_| CommandError::new("HUB_CACHE_ERROR", "BeatmapHub 缓存不可用"))?;
                if let Some(cache) = cached.as_ref() {
                    return Ok(cache.packs.iter().take(limit as usize).cloned().collect());
                }
                return Err(error);
            }
        };
        let cache = RecommendationsCache {
            updated_at: Utc::now(),
            packs: result.packs,
        };
        fs::write(
            &self.recommendations_cache_path,
            serde_json::to_vec(&cache)?,
        )?;
        let packs = cache.packs.iter().take(limit as usize).cloned().collect();
        *self
            .recommendations_cache
            .lock()
            .map_err(|_| CommandError::new("HUB_CACHE_ERROR", "BeatmapHub 缓存不可用"))? =
            Some(cache);
        Ok(packs)
    }

    pub async fn search(&self, query: &str, limit: u8) -> CommandResult<Vec<Pack>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let path = format!("/packs/search?q={encoded}&limit={}", limit.clamp(1, 50));
        let result: PackSearchResponse = if self.identity()?.is_some() {
            self.auth_json(Method::GET, &path, None).await?
        } else {
            self.request_json(Method::GET, &path, None, None).await?
        };
        Ok(result.packs)
    }

    pub async fn preview_pack(
        &self,
        state: &AppState,
        share_id: &str,
    ) -> CommandResult<PackPreview> {
        let pack = self.get_pack(share_id).await?;
        let (locally_available_ids, missing_ids) = pack
            .beatmapset_ids
            .iter()
            .copied()
            .partition(|id| state.local_analysis.contains_beatmapset_id(*id));
        Ok(PackPreview {
            pack,
            locally_available_ids,
            missing_ids,
        })
    }

    pub async fn publish(
        &self,
        state: &AppState,
        folder_id: &str,
        title: String,
        description: String,
        is_private: bool,
    ) -> CommandResult<PublishResult> {
        validate_title_description(&title, &description)?;
        let folder = state.collections.folder(folder_id)?;
        if folder.read_only || folder.source == CollectionSource::Lazer {
            return Err(CommandError::new(
                "COLLECTION_READ_ONLY",
                "只读收藏夹不能发布",
            ));
        }
        let mut ids = Vec::new();
        let mut skipped = 0;
        for entry in &folder.entries {
            match entry.beatmapset_id {
                Some(id) if id > 0 && !ids.contains(&id) => ids.push(id),
                Some(_) => {}
                None => skipped += 1,
            }
        }
        if ids.is_empty() || ids.len() > 500 {
            return Err(CommandError::new(
                "INVALID_PACK_ITEMS",
                "曲包必须包含 1 到 500 个可识别谱面集",
            ));
        }
        let result: CreatePackResponse = self
            .auth_json(
                Method::POST,
                "/packs",
                Some(json!({
                    "title": title.trim(), "description": description, "beatmapset_ids": ids,
                    "is_private": is_private,
                })),
            )
            .await?;
        Ok(PublishResult {
            id: result.id,
            included: ids.len(),
            skipped,
        })
    }

    pub async fn update_pack(
        &self,
        state: &AppState,
        share_id: &str,
        folder_id: &str,
        title: String,
        description: String,
        is_private: bool,
    ) -> CommandResult<()> {
        validate_title_description(&title, &description)?;
        let folder = state.collections.folder(folder_id)?;
        let mut ids = Vec::new();
        for id in folder
            .entries
            .iter()
            .filter_map(|entry| entry.beatmapset_id)
        {
            if id > 0 && !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.is_empty() || ids.len() > 500 {
            return Err(CommandError::new(
                "INVALID_PACK_ITEMS",
                "曲包必须包含 1 到 500 个可识别谱面集",
            ));
        }
        self.auth_empty(
            Method::PATCH,
            &format!("/packs/{}", normalize_share_id(share_id)?),
            Some(json!({
                "title": title.trim(), "description": description, "beatmapset_ids": ids,
                "is_private": is_private,
            })),
        )
        .await
    }

    pub async fn delete_pack(&self, share_id: &str) -> CommandResult<()> {
        self.auth_empty(
            Method::DELETE,
            &format!("/packs/{}", normalize_share_id(share_id)?),
            None,
        )
        .await
    }

    pub async fn rate(&self, share_id: &str, score: u8) -> CommandResult<()> {
        if !(1..=5).contains(&score) {
            return Err(CommandError::new("INVALID_RATING", "评分必须为 1 到 5"));
        }
        self.auth_empty(
            Method::PUT,
            &format!("/packs/{}/rating", normalize_share_id(share_id)?),
            Some(json!({ "score": score })),
        )
        .await
    }

    pub async fn favorite(&self, share_id: &str, enabled: bool) -> CommandResult<()> {
        self.auth_empty(
            if enabled { Method::PUT } else { Method::DELETE },
            &format!("/packs/{}/favorite", normalize_share_id(share_id)?),
            None,
        )
        .await
    }

    pub async fn like(&self, share_id: &str, enabled: bool) -> CommandResult<()> {
        self.auth_empty(
            if enabled { Method::PUT } else { Method::DELETE },
            &format!("/packs/{}/like", normalize_share_id(share_id)?),
            None,
        )
        .await
    }

    pub async fn comments(&self, share_id: &str, limit: u8) -> CommandResult<Vec<PackComment>> {
        let id = normalize_share_id(share_id)?;
        let response: PackCommentsResponse = if self.identity()?.is_some() {
            self.auth_json(
                Method::GET,
                &format!("/packs/{id}/comments?limit={}", limit.clamp(1, 100)),
                None,
            )
            .await?
        } else {
            self.request_json(
                Method::GET,
                &format!("/packs/{id}/comments?limit={}", limit.clamp(1, 100)),
                None,
                None,
            )
            .await?
        };
        Ok(response.comments)
    }

    pub async fn create_comment(
        &self,
        share_id: &str,
        content: String,
    ) -> CommandResult<PackComment> {
        if content.trim().is_empty() || content.chars().count() > 2_000 {
            return Err(CommandError::new(
                "INVALID_COMMENT",
                "评论需为 1 到 2000 个字符",
            ));
        }
        self.auth_json(
            Method::POST,
            &format!("/packs/{}/comments", normalize_share_id(share_id)?),
            Some(json!({"content": content.trim()})),
        )
        .await
    }

    pub async fn update_comment(
        &self,
        comment_id: &str,
        content: String,
    ) -> CommandResult<PackComment> {
        if content.trim().is_empty() || content.chars().count() > 2_000 {
            return Err(CommandError::new(
                "INVALID_COMMENT",
                "评论需为 1 到 2000 个字符",
            ));
        }
        self.auth_json(
            Method::PATCH,
            &format!("/comments/{comment_id}"),
            Some(json!({"content": content.trim()})),
        )
        .await
    }

    pub async fn delete_comment(&self, comment_id: &str) -> CommandResult<()> {
        self.auth_empty(Method::DELETE, &format!("/comments/{comment_id}"), None)
            .await
    }

    pub async fn import_pack(
        &self,
        state: &AppState,
        share_id: &str,
        resolved: Vec<ResolvedBeatmapset>,
    ) -> CommandResult<ImportResult> {
        let pack = self.get_pack(share_id).await?;
        let folder = state
            .collections
            .create(&pack.title, &pack.owner.display_name)?;
        let mut candidates = Vec::new();
        let mut unresolved = 0;
        for set_id in &pack.beatmapset_ids {
            if let Some(set) = resolved.iter().find(|set| set.id == *set_id) {
                if set.beatmaps.is_empty() {
                    unresolved += 1;
                    candidates.push(placeholder(*set_id, &set.title, &set.artist, &set.creator));
                } else {
                    candidates.extend(set.beatmaps.iter().map(|beatmap| CollectionCandidate {
                        beatmap_id: Some(beatmap.id),
                        beatmapset_id: Some(*set_id),
                        checksum: beatmap.checksum.clone(),
                        ruleset: beatmap.mode.clone(),
                        difficulty_name:
                            beatmap.version.clone().unwrap_or_else(|| "未知难度".into()),
                        title: set.title.clone(),
                        artist: set.artist.clone(),
                        creator: set.creator.clone(),
                        local_client: None,
                        local_resource_id: None,
                    }));
                }
            } else {
                unresolved += 1;
                candidates.push(placeholder(
                    *set_id,
                    &format!("Beatmapset #{set_id}"),
                    "",
                    "",
                ));
            }
        }
        let entry_count = candidates.len();
        if let Err(error) = state.collections.add_entries(&folder.id, candidates) {
            let _ = state.collections.delete(&folder.id);
            return Err(error);
        }
        Ok(ImportResult {
            folder_id: folder.id,
            imported_sets: pack.beatmapset_ids.len(),
            imported_entries: entry_count,
            unresolved_sets: unresolved,
        })
    }

    async fn ensure_session(&self, force: bool) -> CommandResult<String> {
        let _guard = self.auth_lock.lock().await;
        if !force {
            let current = self.identity()?.ok_or_else(|| {
                CommandError::new("HUB_IDENTITY_REQUIRED", "请先创建或链接 BeatmapHub 档案")
            })?;
            if current
                .expires_at
                .is_some_and(|expiry| expiry > Utc::now() + Duration::seconds(60))
                && let Some(token) = read_secret(ACCESS_TOKEN_ENTRY)?
            {
                return Ok(token);
            }
        }
        self.challenge_login(true).await
    }

    async fn challenge_login(&self, retry_invalid: bool) -> CommandResult<String> {
        let identity = self.identity()?.ok_or_else(|| {
            CommandError::new("HUB_IDENTITY_REQUIRED", "请先创建或链接 BeatmapHub 档案")
        })?;
        let challenge: ChallengeResponse = self
            .request_json(
                Method::POST,
                "/auth/challenge",
                Some(json!({ "public_key": identity.public_key })),
                None,
            )
            .await?;
        let message = URL_SAFE_NO_PAD
            .decode(&challenge.message)
            .map_err(|_| CommandError::new("INVALID_CHALLENGE", "服务端 Challenge 编码无效"))?;
        let signing = load_signing_key()?;
        let signature = URL_SAFE_NO_PAD.encode(signing.sign(&message).to_bytes());
        let verified = self
            .request_json::<SessionResponse>(
                Method::POST,
                "/auth/verify",
                Some(json!({
                    "challenge_id": challenge.challenge_id, "signature": signature,
                })),
                None,
            )
            .await;
        match verified {
            Ok(session) => {
                write_secret(ACCESS_TOKEN_ENTRY, &session.access_token)?;
                self.update_identity(|value| {
                    value.user_id = session.user.id;
                    value.device_id = session.device.id;
                    value.display_name = session.user.display_name;
                    value.device_name = session.device.device_name;
                    value.expires_at = Some(session.expires_at);
                })?;
                Ok(session.access_token)
            }
            Err(error) if error.code == "INVALID_CHALLENGE" && retry_invalid => {
                Box::pin(self.challenge_login(false)).await
            }
            Err(error) => Err(error),
        }
    }

    fn commit_identity(
        &self,
        signing: SigningKey,
        public_key: String,
        session: SessionResponse,
    ) -> CommandResult<()> {
        write_secret(
            PRIVATE_KEY_ENTRY,
            &URL_SAFE_NO_PAD.encode(signing.to_bytes()),
        )?;
        write_secret(ACCESS_TOKEN_ENTRY, &session.access_token)?;
        self.replace_identity(Some(IdentityMetadata {
            public_key,
            user_id: session.user.id,
            device_id: session.device.id,
            display_name: session.user.display_name,
            device_name: session.device.device_name,
            expires_at: Some(session.expires_at),
        }))
    }

    fn identity(&self) -> CommandResult<Option<IdentityMetadata>> {
        self.identity
            .lock()
            .map(|value| value.clone())
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 身份状态不可用"))
    }

    fn update_identity(&self, operation: impl FnOnce(&mut IdentityMetadata)) -> CommandResult<()> {
        let mut identity = self
            .identity()?
            .ok_or_else(|| CommandError::new("HUB_IDENTITY_REQUIRED", "BeatmapHub 身份不存在"))?;
        operation(&mut identity);
        self.replace_identity(Some(identity))
    }

    fn replace_identity(&self, value: Option<IdentityMetadata>) -> CommandResult<()> {
        let bytes = serde_json::to_vec_pretty(&value)?;
        atomic_write(&self.identity_path, &bytes)?;
        *self
            .identity
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 身份状态不可用"))? =
            value;
        Ok(())
    }

    fn cached_pack(&self, id: &str) -> CommandResult<Option<CachedPack>> {
        self.pack_cache
            .lock()
            .map(|cache| {
                cache
                    .entries
                    .get(id)
                    .filter(|entry| valid_cached_pack(entry))
                    .cloned()
            })
            .map_err(|_| CommandError::new("HUB_CACHE_ERROR", "BeatmapHub 缓存不可用"))
    }

    // Cache persistence is best-effort: a read-only cache directory must not prevent opening a pack.
    fn store_cached_pack(&self, id: &str, entry: CachedPack) {
        let snapshot = match self.pack_cache.lock() {
            Ok(mut cache) => {
                cache.entries.insert(id.to_string(), entry);
                cache.clone()
            }
            Err(_) => return,
        };
        if let Ok(bytes) = serde_json::to_vec(&snapshot) {
            let _ = atomic_write(&self.pack_cache_path, &bytes);
        }
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> CommandResult<T> {
        let response = self.send(method, path, body, token).await?;
        response.json().await.map_err(|error| {
            CommandError::new(
                "INVALID_HUB_RESPONSE",
                format!("BeatmapHub 响应格式无效：{error}"),
            )
        })
    }

    async fn request_empty(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> CommandResult<()> {
        self.send(method, path, body, token).await.map(|_| ())
    }

    async fn auth_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> CommandResult<T> {
        let token = self.ensure_session(false).await?;
        match self
            .request_json(method.clone(), path, body.clone(), Some(&token))
            .await
        {
            Err(error) if error.code == "INVALID_SESSION" => {
                delete_secret(ACCESS_TOKEN_ENTRY)?;
                let token = self.ensure_session(true).await?;
                self.request_json(method, path, body, Some(&token)).await
            }
            result => result,
        }
    }

    async fn auth_empty(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> CommandResult<()> {
        let token = self.ensure_session(false).await?;
        match self
            .request_empty(method.clone(), path, body.clone(), Some(&token))
            .await
        {
            Err(error) if error.code == "INVALID_SESSION" => {
                delete_secret(ACCESS_TOKEN_ENTRY)?;
                let token = self.ensure_session(true).await?;
                self.request_empty(method, path, body, Some(&token)).await
            }
            result => result,
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> CommandResult<Response> {
        let mut request = self.client.request(method, format!("{BASE_URL}{path}"));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|error| {
            CommandError::network(format!(
                "无法连接 BeatmapHub：{}",
                reqwest_error_details(&error)
            ))
        })?;
        if response.status().is_success() {
            return Ok(response);
        }
        Err(response_error(response).await)
    }
}

fn read_recommendations_cache(path: &Path) -> Option<RecommendationsCache> {
    let cache = serde_json::from_slice::<RecommendationsCache>(&fs::read(path).ok()?).ok()?;
    let valid = cache.packs.iter().all(|pack| {
        !pack.id.trim().is_empty()
            && !pack.title.trim().is_empty()
            && !pack.beatmapset_ids.is_empty()
            && cache.updated_at <= Utc::now() + Duration::minutes(5)
            && cache.updated_at >= Utc::now() - Duration::days(30)
    });
    valid.then_some(cache)
}

fn read_pack_cache(path: &Path) -> PackCache {
    let mut cache = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PackCache>(&bytes).ok())
        .unwrap_or_default();
    cache.entries.retain(|_, entry| valid_cached_pack(entry));
    cache
}

fn valid_cached_pack(entry: &CachedPack) -> bool {
    !entry.etag.trim().is_empty()
        && !entry.manifest_hash.trim().is_empty()
        && entry.manifest_hash == entry.pack.manifest_hash
        && !entry.pack.id.trim().is_empty()
        && !entry.pack.beatmapset_ids.is_empty()
        && entry.cached_at <= Utc::now() + Duration::minutes(5)
        && entry.cached_at >= Utc::now() - Duration::days(30)
}

async fn response_error(response: Response) -> CommandError {
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = response.status();
    let envelope = response.json::<ErrorEnvelope>().await.ok();
    let error = envelope
        .map(|value| CommandError::new(value.error.code, value.error.message))
        .unwrap_or_else(|| {
            CommandError::new(
                "HUB_HTTP_ERROR",
                format!("BeatmapHub 请求失败（HTTP {status}）"),
            )
        });
    error.request_id(request_id)
}

fn placeholder(set_id: i32, title: &str, artist: &str, creator: &str) -> CollectionCandidate {
    CollectionCandidate {
        beatmap_id: None,
        beatmapset_id: Some(set_id),
        checksum: None,
        ruleset: None,
        difficulty_name: "全部难度".into(),
        title: title.into(),
        artist: artist.into(),
        creator: creator.into(),
        local_client: None,
        local_resource_id: None,
    }
}

fn default_device_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn reqwest_error_details(error: &reqwest::Error) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        if !messages.contains(&message) {
            messages.push(message);
        }
        source = cause.source();
    }
    messages.join("：")
}

fn discover_proxy_url() -> Option<String> {
    ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(windows_internet_proxy)
        .or_else(loopback_proxy)
}

#[cfg(windows)]
fn windows_internet_proxy() -> Option<String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled = settings.get_value::<u32, _>("ProxyEnable").ok()? != 0;
    if !enabled {
        return None;
    }
    let value = settings.get_value::<String, _>("ProxyServer").ok()?;
    let candidate = value
        .split(';')
        .find_map(|entry| {
            entry
                .strip_prefix("https=")
                .or_else(|| entry.strip_prefix("http="))
        })
        .unwrap_or(value.as_str())
        .trim();
    (!candidate.is_empty()).then(|| normalize_http_proxy(candidate))
}

#[cfg(not(windows))]
fn windows_internet_proxy() -> Option<String> {
    None
}

fn loopback_proxy() -> Option<String> {
    // 7890 is the standard local mixed-proxy port used by the current desktop setup.
    // Only select it when a listener is actually present, so direct-network users are unaffected.
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7890);
    TcpStream::connect_timeout(&address, StdDuration::from_millis(100))
        .ok()
        .map(|_| "http://127.0.0.1:7890".to_string())
}

fn normalize_http_proxy(value: &str) -> String {
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

fn handshake_message(public_key: &str, display_name: &str, device_name: &str) -> String {
    format!("OPP_BPH_HANDSHAKE_V1\n{public_key}\n{display_name}\n{device_name}")
}

fn device_link_message(link_token: &str, public_key: &str, device_name: &str) -> String {
    format!("OPP_BPH_LINK_DEVICE_V1\n{link_token}\n{public_key}\n{device_name}")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> CommandResult<()> {
    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    fs::write(&temporary, bytes)?;
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(path, &backup)?;
    }
    match fs::rename(&temporary, path) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            if backup.exists() {
                let _ = fs::rename(backup, path);
            }
            Err(error.into())
        }
    }
}

pub fn normalize_share_id(raw: &str) -> CommandResult<String> {
    let normalized = raw
        .trim()
        .to_ascii_uppercase()
        .strip_prefix("BPH-")
        .unwrap_or(raw.trim())
        .to_ascii_uppercase();
    let valid = normalized.len() == 6
        && normalized
            .bytes()
            .all(|value| b"23456789ABCDEFGHJKMNPQRSTUVWXYZ".contains(&value));
    if valid {
        Ok(normalized)
    } else {
        Err(CommandError::new(
            "INVALID_SHARE_ID",
            "请输入有效的 6 位 BeatmapHub 分享码",
        ))
    }
}

fn validate_name(value: &str, label: &str) -> CommandResult<()> {
    let length = value.trim().chars().count();
    if (1..=64).contains(&length) {
        Ok(())
    } else {
        Err(CommandError::new(
            "INVALID_IDENTITY",
            format!("{label}需为 1 到 64 个字符"),
        ))
    }
}

fn validate_title_description(title: &str, description: &str) -> CommandResult<()> {
    let title_len = title.trim().chars().count();
    if !(1..=120).contains(&title_len) {
        return Err(CommandError::new(
            "INVALID_PACK_TITLE",
            "标题需为 1 到 120 个字符",
        ));
    }
    if description.chars().count() > 2_000 {
        return Err(CommandError::new(
            "INVALID_PACK_DESCRIPTION",
            "描述不能超过 2000 个字符",
        ));
    }
    Ok(())
}

fn is_token(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn entry(name: &str) -> CommandResult<Entry> {
    Entry::new(SERVICE, name).map_err(keyring_error)
}
fn write_secret(name: &str, value: &str) -> CommandResult<()> {
    entry(name)?
        .set_secret(value.as_bytes())
        .map_err(keyring_error)
}
fn read_secret(name: &str) -> CommandResult<Option<String>> {
    match entry(name)?.get_secret() {
        Ok(value) => String::from_utf8(value)
            .map(Some)
            .map_err(|_| CommandError::new("HUB_CREDENTIAL_ERROR", "BeatmapHub 安全凭据编码无效")),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(keyring_error(error)),
    }
}
fn delete_secret(name: &str) -> CommandResult<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(keyring_error(error)),
    }
}
fn keyring_error(error: KeyringError) -> CommandError {
    CommandError::new(
        "HUB_CREDENTIAL_ERROR",
        format!("系统安全存储不可用：{error}"),
    )
}
fn load_signing_key() -> CommandResult<SigningKey> {
    let encoded = read_secret(PRIVATE_KEY_ENTRY)?.ok_or_else(|| {
        CommandError::new("HUB_KEY_MISSING", "BeatmapHub 私钥不存在，请重新链接设备")
    })?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| CommandError::new("HUB_KEY_INVALID", "BeatmapHub 私钥编码无效"))?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| CommandError::new("HUB_KEY_INVALID", "BeatmapHub 私钥长度无效"))?;
    Ok(SigningKey::from_bytes(&key))
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use ed25519_dalek::{Signer, SigningKey};

    use super::{
        CachedPack, Pack, device_link_message, handshake_message, normalize_share_id,
        valid_cached_pack,
    };

    #[test]
    fn normalizes_share_ids() {
        assert_eq!(normalize_share_id("bph-7k3n9a").unwrap(), "7K3N9A");
        assert!(normalize_share_id("O0I1LL").is_err());
    }

    #[test]
    fn protocol_messages_use_lf_without_trailing_newline() {
        assert_eq!(
            handshake_message("pub", "Player", "PC"),
            "OPP_BPH_HANDSHAKE_V1\npub\nPlayer\nPC"
        );
        assert_eq!(
            device_link_message("token", "pub", "Laptop"),
            "OPP_BPH_LINK_DEVICE_V1\ntoken\npub\nLaptop"
        );
    }

    #[test]
    fn signatures_have_protocol_base64url_length() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let encoded = URL_SAFE_NO_PAD.encode(signing.sign(b"challenge").to_bytes());
        assert_eq!(encoded.len(), 86);
        assert!(!encoded.contains('='));
    }

    #[test]
    fn accepts_only_cache_entries_bound_to_the_pack_manifest_hash() {
        let pack: Pack = serde_json::from_value(serde_json::json!({
            "id": "7K3N9A",
            "title": "Cache test",
            "description": "",
            "owner": { "id": "owner", "display_name": "Owner" },
            "beatmapset_ids": [123],
            "manifest_hash": "manifest-a",
            "rating": { "average": null, "count": 0 },
            "viewer": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        let cached = CachedPack {
            etag: "\"2026-01-01T00:00:00Z:manifest-a\"".into(),
            manifest_hash: "manifest-a".into(),
            cached_at: chrono::Utc::now(),
            pack,
        };
        assert!(valid_cached_pack(&cached));

        let mismatched = CachedPack {
            manifest_hash: "manifest-b".into(),
            ..cached
        };
        assert!(!valid_cached_pack(&mismatched));
    }

    #[tokio::test]
    #[ignore = "live production transport check"]
    async fn production_worker_is_reachable_with_hub_transport() {
        let proxy_url = super::loopback_proxy().expect("local proxy on port 7890 is required");
        let client = reqwest::Client::builder()
            .no_proxy()
            .proxy(reqwest::Proxy::all(proxy_url).unwrap())
            .http1_only()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap();
        let response = client.get(super::BASE_URL).send().await.unwrap();
        assert!(response.status().is_success());

        let handshake = client
            .post(format!("{}/auth/handshake", super::BASE_URL))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            handshake.status(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
