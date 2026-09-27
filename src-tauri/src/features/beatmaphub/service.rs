use std::{
    error::Error as _,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration as StdDuration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use ed25519_dalek::{Signer, SigningKey};
use keyring::{Entry, Error as KeyringError};
use rand_core::OsRng;
use reqwest::{Client, Method, Response};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;

use super::models::*;
use crate::{
    error::{CommandError, CommandResult},
    features::collections::{CollectionCandidate, CollectionSource},
    state::AppState,
};

pub const BASE_URL: &str = "http://8.137.98.96/api/v2";
const SERVICE: &str = "com.opp.desktop";
const PRIVATE_KEY_ENTRY: &str = "beatmaphub-ed25519-private-key";
const LEGACY_ACCESS_TOKEN_ENTRY: &str = "beatmaphub-access-token";

pub struct BeatmapHubService {
    client: Client,
    identity_path: PathBuf,
    identity: Mutex<Option<IdentityMetadata>>,
    access_token: Mutex<Option<AccessToken>>,
    bootstrap_claims: Mutex<Option<BootstrapClaims>>,
    auth_lock: AsyncMutex<()>,
}

#[derive(Debug, Clone)]
struct AccessToken {
    value: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct BootstrapClaims {
    user_id: String,
    username: String,
}

impl BeatmapHubService {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let _ = delete_secret(LEGACY_ACCESS_TOKEN_ENTRY);
        let directory = app_data_dir.join("beatmaphub");
        fs::create_dir_all(&directory)?;
        let identity_path = directory.join("identity.json");
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
            access_token: Mutex::new(None),
            bootstrap_claims: Mutex::new(None),
            auth_lock: AsyncMutex::new(()),
        })
    }

    pub fn status(&self) -> CommandResult<AuthStatus> {
        let identity = self.identity()?;
        let connected = self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))?
            .as_ref()
            .is_some_and(|token| token.expires_at > Utc::now() + Duration::seconds(10));
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

    pub async fn bootstrap(
        &self,
        claimed_user_id: u64,
        claimed_username: String,
    ) -> CommandResult<AuthStatus> {
        let _guard = self.auth_lock.lock().await;
        let username = claimed_username.trim().to_string();
        validate_name(&username, "osu 用户名")?;
        let signing = load_or_generate_signing_key()?;
        let public_key = URL_SAFE_NO_PAD.encode(signing.verifying_key().as_bytes());
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let challenge: ChallengeResponse = self
            .request_json(
                Method::POST,
                "/auth/challenge",
                Some(json!({ "public_key": public_key })),
                None,
            )
            .await?;
        let message = canonical_auth_message(
            &challenge.challenge_id,
            &public_key,
            claimed_user_id,
            &username,
            &timestamp,
        );
        let signature = URL_SAFE_NO_PAD.encode(signing.sign(message.as_bytes()).to_bytes());
        let response: BootstrapResponse = self
            .request_json(Method::POST, "/auth/bootstrap", Some(json!({
                "challenge_id": challenge.challenge_id,
                "public_key": public_key,
                "signature": signature,
                "claimed_osu_user_id": claimed_user_id.to_string(),
                "claimed_username": username,
                "opp_version": env!("CARGO_PKG_VERSION"),
                "device_metadata": { "device_name": default_device_name().unwrap_or_else(|| "OPP Desktop".into()), "platform": std::env::consts::OS },
                "timestamp": timestamp,
            })), None)
            .await?;
        self.store_token(response.access_token, response.expires_in)?;
        self.replace_identity(Some(IdentityMetadata {
            public_key,
            user_id: response.identity.claimed_osu_user_id,
            device_id: response.device.id,
            display_name: response.identity.claimed_username,
            device_name: default_device_name().unwrap_or_else(|| "OPP Desktop".into()),
            expires_at: Some(Utc::now() + Duration::seconds(response.expires_in as i64)),
        }))?;
        *self
            .bootstrap_claims
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 状态不可用"))? =
            Some(BootstrapClaims {
                user_id: claimed_user_id.to_string(),
                username,
            });
        self.status()
    }

    pub async fn reconnect(
        &self,
        claimed_user_id: u64,
        claimed_username: String,
    ) -> CommandResult<AuthStatus> {
        let _ = delete_secret(PRIVATE_KEY_ENTRY);
        self.replace_identity(None)?;
        *self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))? = None;
        *self
            .bootstrap_claims
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 状态不可用"))? = None;
        self.bootstrap(claimed_user_id, claimed_username).await
    }

    pub async fn logout(&self) -> CommandResult<()> {
        if let Some(token) = self.current_token()? {
            let result = self
                .request_empty(Method::POST, "/auth/logout", None, Some(&token))
                .await;
            if let Err(error) = &result
                && !matches!(error.code.as_str(), "INVALID_SESSION" | "AUTH_REQUIRED")
            {
                return result;
            }
        }
        *self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))? = None;
        self.update_identity(|identity| identity.expires_at = None)
    }

    pub async fn profile(&self) -> CommandResult<Profile> {
        self.auth_json(Method::GET, "/auth/me", None).await
    }

    pub async fn revoke_device(&self, device_id: &str) -> CommandResult<()> {
        self.auth_empty(Method::DELETE, &format!("/auth/devices/{device_id}"), None)
            .await
    }

    pub async fn get_pack(&self, share_id: &str) -> CommandResult<Pack> {
        let id = normalize_share_id(share_id)?;
        if self.current_token()?.is_some() {
            self.auth_json(Method::GET, &format!("/packs/{id}"), None)
                .await
        } else {
            self.request_json(Method::GET, &format!("/packs/{id}"), None, None)
                .await
        }
    }

    pub async fn recommendations(&self, limit: u8) -> CommandResult<Vec<Pack>> {
        #[derive(serde::Deserialize)]
        struct RecommendationResponse {
            packs: Vec<Pack>,
        }
        let limit = limit.clamp(1, 50);
        let result: RecommendationResponse = self
            .request_json(
                Method::GET,
                &format!("/packs/recommendations?limit={limit}"),
                None,
                None,
            )
            .await?;
        Ok(result.packs)
    }

    pub async fn search(&self, query: &str, limit: u8) -> CommandResult<Vec<Pack>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let path = format!("/packs/search?q={encoded}&limit={}", limit.clamp(1, 50));
        let result: PackSearchResponse = if self.current_token()?.is_some() {
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
        let response: PackCommentsResponse = if self.current_token()?.is_some() {
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
            &format!("/pack-comments/{comment_id}"),
            Some(json!({"content": content.trim()})),
        )
        .await
    }

    pub async fn delete_comment(&self, comment_id: &str) -> CommandResult<()> {
        self.auth_empty(
            Method::DELETE,
            &format!("/pack-comments/{comment_id}"),
            None,
        )
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

    fn current_token(&self) -> CommandResult<Option<String>> {
        let token = self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))?
            .as_ref()
            .filter(|token| token.expires_at > Utc::now() + Duration::seconds(10))
            .map(|token| token.value.clone());
        Ok(token)
    }

    fn store_token(&self, value: String, expires_in: u64) -> CommandResult<()> {
        *self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))? =
            Some(AccessToken {
                value,
                expires_at: Utc::now() + Duration::seconds(expires_in as i64),
            });
        Ok(())
    }

    async fn ensure_session(&self, force: bool) -> CommandResult<String> {
        let _guard = self.auth_lock.lock().await;
        if !force && let Some(token) = self.current_token()? {
            return Ok(token);
        }
        if !force
            && self.has_expired_token()?
            && self.identity()?.is_some()
            && let Ok(token) = self.refresh_session().await
        {
            return Ok(token);
        }
        let claims = self
            .bootstrap_claims
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 状态不可用"))?
            .clone()
            .ok_or_else(|| CommandError::new("HUB_AUTH_REQUIRED", "请先完成 osu! 登录"))?;
        drop(_guard);
        let user_id = claims.user_id.parse::<u64>().map_err(|_| {
            CommandError::new("HUB_AUTH_REQUIRED", "PackHub 用户身份无效，请重新连接")
        })?;
        self.bootstrap(user_id, claims.username).await?;
        self.current_token()?
            .ok_or_else(|| CommandError::new("HUB_AUTH_REQUIRED", "BeatmapHub 会话不可用"))
    }

    fn has_expired_token(&self) -> CommandResult<bool> {
        Ok(self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))?
            .as_ref()
            .is_some())
    }

    async fn refresh_session(&self) -> CommandResult<String> {
        let identity = self
            .identity()?
            .ok_or_else(|| CommandError::new("HUB_AUTH_REQUIRED", "BeatmapHub 身份不存在"))?;
        let signing = load_or_generate_signing_key()?;
        let nonce: RefreshNonceResponse = self
            .request_json(
                Method::POST,
                "/auth/refresh/nonce",
                Some(json!({ "public_key": identity.public_key })),
                None,
            )
            .await?;
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let message = format!(
            "packhub-refresh-v2\n{}\n{}\n{}",
            nonce.nonce, identity.public_key, timestamp
        );
        let signature = URL_SAFE_NO_PAD.encode(signing.sign(message.as_bytes()).to_bytes());
        let response: RefreshResponse = self.request_json(Method::POST, "/auth/refresh", Some(json!({ "public_key": identity.public_key, "nonce": nonce.nonce, "signature": signature, "timestamp": timestamp })), None).await?;
        self.store_token(response.access_token.clone(), response.expires_in)?;
        Ok(response.access_token)
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
            Err(error) if matches!(error.code.as_str(), "INVALID_SESSION" | "INVALID_TOKEN") => {
                *self.access_token.lock().map_err(|_| {
                    CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用")
                })? = None;
                let token = self.ensure_session(true).await?;
                self.request_json(method, path, body, Some(&token)).await
            }
            Err(error) if error.code == "DEVICE_REVOKED" => {
                *self.access_token.lock().map_err(|_| {
                    CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用")
                })? = None;
                Err(error)
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
            Err(error) if matches!(error.code.as_str(), "INVALID_SESSION" | "INVALID_TOKEN") => {
                *self.access_token.lock().map_err(|_| {
                    CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用")
                })? = None;
                let token = self.ensure_session(true).await?;
                self.request_empty(method, path, body, Some(&token)).await
            }
            Err(error) if error.code == "DEVICE_REVOKED" => {
                *self.access_token.lock().map_err(|_| {
                    CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用")
                })? = None;
                Err(error)
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

fn canonical_auth_message(
    challenge_id: &str,
    public_key: &str,
    user_id: u64,
    username: &str,
    timestamp: &str,
) -> String {
    format!(
        "packhub-auth-v2\n{challenge_id}\n{user_id}\n{username}\n{}\n{timestamp}\n{public_key}",
        env!("CARGO_PKG_VERSION")
    )
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
fn load_or_generate_signing_key() -> CommandResult<SigningKey> {
    let Some(encoded) = read_secret(PRIVATE_KEY_ENTRY)? else {
        let signing = SigningKey::generate(&mut OsRng);
        write_secret(
            PRIVATE_KEY_ENTRY,
            &URL_SAFE_NO_PAD.encode(signing.to_bytes()),
        )?;
        return Ok(signing);
    };
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

    use super::{canonical_auth_message, normalize_share_id};

    #[test]
    fn normalizes_share_ids() {
        assert_eq!(normalize_share_id("bph-7k3n9a").unwrap(), "7K3N9A");
        assert!(normalize_share_id("O0I1LL").is_err());
    }

    #[test]
    fn bootstrap_message_uses_documented_canonical_fields() {
        assert_eq!(
            canonical_auth_message(
                "challenge",
                "pub",
                123,
                "Player",
                "2026-09-26T10:00:00.000Z"
            ),
            format!(
                "packhub-auth-v2\nchallenge\n123\nPlayer\n{}\n2026-09-26T10:00:00.000Z\npub",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn signatures_have_protocol_base64url_length() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let encoded = URL_SAFE_NO_PAD.encode(signing.sign(b"challenge").to_bytes());
        assert_eq!(encoded.len(), 86);
        assert!(!encoded.contains('='));
    }

    #[tokio::test]
    #[ignore = "live production transport check"]
    async fn staging_packhub_is_reachable_with_hub_transport() {
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

        let challenge = client
            .post(format!("{}/auth/challenge", super::BASE_URL))
            .json(
                &serde_json::json!({ "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" }),
            )
            .send()
            .await
            .unwrap();
        assert!(challenge.status().is_success() || challenge.status().is_client_error());
    }
}
