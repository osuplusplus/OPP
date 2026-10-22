//! BeatmapHub 客户端：Ed25519 设备身份与签名认证、会话自动恢复、
//! 曲包的浏览/发布/互动/导入，以及系统代理探测。
use std::{
    collections::BTreeSet,
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

/// 服务端 API 根地址。
pub const BASE_URL: &str = "http://8.137.98.96/api/v2";
/// 系统凭据库（keyring）中的服务名。
const SERVICE: &str = "com.opp.desktop";
/// keyring 中 Ed25519 设备私钥的条目名。
const PRIVATE_KEY_ENTRY: &str = "beatmaphub-ed25519-private-key";
/// 旧版本遗留的令牌条目名，启动时清理。
const LEGACY_ACCESS_TOKEN_ENTRY: &str = "beatmaphub-access-token";

/// BeatmapHub 服务入口，持有 HTTP 客户端与全部会话状态，随 `AppState` 共享。
pub struct BeatmapHubService {
    /// 共享 HTTP 客户端（HTTP/1.1、20 秒超时、可选代理）。
    client: Client,
    /// 身份文件路径（`beatmaphub/identity.json`）。
    identity_path: PathBuf,
    /// 内存身份缓存，写穿到磁盘文件。
    identity: Mutex<Option<IdentityMetadata>>,
    /// 内存访问令牌缓存，不落盘。
    access_token: Mutex<Option<AccessToken>>,
    /// 最近一次 bootstrap 使用的 osu 身份，用于会话失效后静默重连。
    bootstrap_claims: Mutex<Option<BootstrapClaims>>,
    /// 串行化认证流程的异步锁，避免并发触发重复 bootstrap 或刷新。
    auth_lock: AsyncMutex<()>,
}

/// 带过期时间的访问令牌。
#[derive(Debug, Clone)]
struct AccessToken {
    value: String,
    expires_at: DateTime<Utc>,
}

/// bootstrap 所用的 osu 身份声明。
#[derive(Debug, Clone)]
struct BootstrapClaims {
    user_id: String,
    username: String,
}

impl BeatmapHubService {
    /// 初始化服务：清理旧版令牌、加载身份文件并构建 HTTP 客户端。
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        // 尽力清理历史版本遗留在系统凭据库中的明文令牌。
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

    /// 返回认证状态快照，不触发网络请求。
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

    /// 挑战-签名式登录：以当前 osu 身份注册新设备并换取访问令牌。
    ///
    /// 流程：申请 challenge → 用 Ed25519 私钥签署规范消息 → 提交
    /// bootstrap，最后把令牌与设备身份落盘。
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

    /// 吊销本地密钥与身份后重新走 bootstrap，用于身份异常时的强制重置。
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

    /// 注销会话；服务端已判定会话失效时仍视为登出成功。
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

    /// 获取当前用户资料与已绑定设备列表。
    pub async fn profile(&self) -> CommandResult<Profile> {
        self.auth_json(Method::GET, "/auth/me", None).await
    }

    /// 吊销指定设备在服务端的访问权。
    pub async fn revoke_device(&self, device_id: &str) -> CommandResult<()> {
        self.auth_empty(Method::DELETE, &format!("/auth/devices/{device_id}"), None)
            .await
    }

    /// 获取曲包；已登录走认证接口以可见私有包，匿名走公开接口。
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

    /// 获取推荐曲包，`limit` 会被夹取到 1 到 50。
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

    /// 按关键词搜索曲包；空白关键词直接返回空列表。
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

    /// 预览曲包，并按本地索引把谱面集拆分为已有/缺失两组。
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

    /// 把本地收藏夹发布为曲包。
    ///
    /// 只读或 lazer 来源的收藏夹不允许发布；条目按谱面集去重，
    /// 缺少谱面集 ID 的条目计入 `skipped`，总数必须为 1 到 500。
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

    /// 用收藏夹的最新内容更新已有曲包，条目规则与发布一致。
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

    /// 删除曲包。
    pub async fn delete_pack(&self, share_id: &str) -> CommandResult<()> {
        self.auth_empty(
            Method::DELETE,
            &format!("/packs/{}", normalize_share_id(share_id)?),
            None,
        )
        .await
    }

    /// 为曲包评分，分数必须是 1 到 5。
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

    /// 收藏或取消收藏曲包。
    pub async fn favorite(&self, share_id: &str, enabled: bool) -> CommandResult<()> {
        self.auth_empty(
            if enabled { Method::PUT } else { Method::DELETE },
            &format!("/packs/{}/favorite", normalize_share_id(share_id)?),
            None,
        )
        .await
    }

    /// 点赞或取消点赞曲包。
    pub async fn like(&self, share_id: &str, enabled: bool) -> CommandResult<()> {
        self.auth_empty(
            if enabled { Method::PUT } else { Method::DELETE },
            &format!("/packs/{}/like", normalize_share_id(share_id)?),
            None,
        )
        .await
    }

    /// 拉取曲包评论，`limit` 夹取到 1 到 100，匿名也可读取。
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

    /// 发表评论，正文为 1 到 2000 个字符。
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

    /// 编辑评论，长度规则与发表相同。
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

    /// 删除评论。
    pub async fn delete_comment(&self, comment_id: &str) -> CommandResult<()> {
        self.auth_empty(
            Method::DELETE,
            &format!("/pack-comments/{comment_id}"),
            None,
        )
        .await
    }

    /// 把曲包导入为新的本地收藏夹。
    ///
    /// `resolved` 为前端解析的谱面集元数据；解析不到难度列表的集合
    /// 生成“全部难度”占位条目，写入失败时回滚删除新建的收藏夹。
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
        // 写入失败时回滚：删除刚创建的收藏夹再上抛错误。
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

    /// 把本地谱面集 ID 列表整体上传为云端备份（覆盖式替换）。
    ///
    /// ID 取自 Stable 与 Lazer 两个本地索引的并集（正值、去重、升序）；
    /// 超出服务端单份备份上限时直接拒绝，不做截断。
    pub async fn upload_backup(&self, state: &AppState) -> CommandResult<BackupUploadResult> {
        const MAX_ITEMS: usize = 20_000;
        let ids = state.local_analysis.collect_beatmapset_ids();
        if ids.is_empty() {
            return Err(CommandError::new(
                "BACKUP_EMPTY_LIBRARY",
                "本地还没有可备份的谱面，请先扫描本地资源",
            ));
        }
        if ids.len() > MAX_ITEMS {
            return Err(CommandError::new(
                "BACKUP_TOO_LARGE",
                format!(
                    "本地谱面集共 {} 个，超过服务端单份备份上限 {} 个",
                    ids.len(),
                    MAX_ITEMS
                ),
            ));
        }
        let mut result: BackupUploadResult = self
            .auth_json(
                Method::PUT,
                "/beatmap-backup",
                Some(json!({ "beatmapset_ids": ids })),
            )
            .await?;
        // 本地已按集合去重，skipped 以服务端返回为准做兜底对齐。
        result.skipped = ids.len().saturating_sub(result.count);
        Ok(result)
    }

    /// 读取云端备份，并按本地索引计算已有 / 缺失统计。
    pub async fn get_backup(&self, state: &AppState) -> CommandResult<BeatmapBackup> {
        let mut backup: BeatmapBackup =
            self.auth_json(Method::GET, "/beatmap-backup", None).await?;
        let local: BTreeSet<i32> = state
            .local_analysis
            .collect_beatmapset_ids()
            .into_iter()
            .collect();
        let available = backup
            .beatmapset_ids
            .iter()
            .filter(|id| local.contains(*id))
            .count();
        backup.locally_available_count = available;
        backup.missing_count = backup.beatmapset_ids.len().saturating_sub(available);
        Ok(backup)
    }

    /// 删除云端谱面备份；备份不存在时上抛服务端 `BACKUP_NOT_FOUND`。
    pub async fn delete_backup(&self) -> CommandResult<()> {
        self.auth_empty(Method::DELETE, "/beatmap-backup", None)
            .await
    }

    /// 读取云端备份并恢复为新的本地收藏夹。
    ///
    /// 本地已有的谱面集按难度展开为完整条目（带 MD5，标记已解析）；
    /// 缺失的谱面集写入占位条目，之后在收藏页按谱面集批量补齐下载。
    /// 写入失败时回滚删除新建的收藏夹。
    pub async fn restore_backup(&self, state: &AppState) -> CommandResult<BackupRestoreResult> {
        let backup: BeatmapBackup = self.auth_json(Method::GET, "/beatmap-backup", None).await?;
        let ids: BTreeSet<i32> = backup.beatmapset_ids.iter().copied().collect();
        let snapshots = state.local_analysis.local_sets_by_id(&ids)?;
        let creator = self
            .identity()?
            .map(|identity| identity.display_name)
            .unwrap_or_else(|| "OPP 备份".into());
        let mut candidates = Vec::new();
        let mut local_sets = 0usize;
        for id in &ids {
            if let Some(snapshot) = snapshots.get(id) {
                local_sets += 1;
                for difficulty in &snapshot.difficulties {
                    candidates.push(CollectionCandidate {
                        beatmap_id: difficulty.beatmap_id,
                        beatmapset_id: Some(snapshot.beatmapset_id),
                        checksum: difficulty.checksum.clone(),
                        ruleset: difficulty.ruleset.clone(),
                        difficulty_name: difficulty.difficulty_name.clone(),
                        title: snapshot.title.clone(),
                        artist: snapshot.artist.clone(),
                        creator: snapshot.creator.clone(),
                        local_client: Some(snapshot.client),
                        local_resource_id: None,
                    });
                }
            } else {
                candidates.push(placeholder(*id, &format!("Beatmapset #{id}"), "", ""));
            }
        }
        let folder_name = format!("谱面备份恢复 {}", Utc::now().format("%Y-%m-%d %H:%M"));
        let folder = state.collections.create(&folder_name, &creator)?;
        let added = match state.collections.add_entries_bulk(&folder.id, candidates) {
            Ok(added) => added,
            Err(error) => {
                let _ = state.collections.delete(&folder.id);
                return Err(error);
            }
        };
        Ok(BackupRestoreResult {
            folder_id: folder.id,
            folder_name,
            total_sets: ids.len(),
            local_sets,
            missing_sets: ids.len() - local_sets,
            entries: added,
        })
    }

    /// 取出仍有效的令牌；距过期不足 10 秒即视为不可用。
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

    /// 缓存访问令牌并按 `expires_in` 推算过期时间。
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

    /// 确保返回可用令牌：优先复用未过期令牌，其次尝试签名刷新，
    /// 最后用缓存的 osu 身份重新 bootstrap。
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
        // bootstrap 内部会再次获取 auth_lock，必须先释放避免自锁。
        drop(_guard);
        let user_id = claims.user_id.parse::<u64>().map_err(|_| {
            CommandError::new("HUB_AUTH_REQUIRED", "PackHub 用户身份无效，请重新连接")
        })?;
        self.bootstrap(user_id, claims.username).await?;
        self.current_token()?
            .ok_or_else(|| CommandError::new("HUB_AUTH_REQUIRED", "BeatmapHub 会话不可用"))
    }

    /// 令牌缓存是否非空；由于 `current_token` 已过滤有效期，
    /// 此处非空即代表存在已过期、可尝试刷新的令牌。
    fn has_expired_token(&self) -> CommandResult<bool> {
        Ok(self
            .access_token
            .lock()
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 会话状态不可用"))?
            .as_ref()
            .is_some())
    }

    /// 用设备私钥走 nonce 签名流程刷新令牌，无需重新走 osu 登录。
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

    /// 读取内存中的身份元数据副本。
    fn identity(&self) -> CommandResult<Option<IdentityMetadata>> {
        self.identity
            .lock()
            .map(|value| value.clone())
            .map_err(|_| CommandError::new("HUB_STATE_ERROR", "BeatmapHub 身份状态不可用"))
    }

    /// 就地修改当前身份并落盘。
    fn update_identity(&self, operation: impl FnOnce(&mut IdentityMetadata)) -> CommandResult<()> {
        let mut identity = self
            .identity()?
            .ok_or_else(|| CommandError::new("HUB_IDENTITY_REQUIRED", "BeatmapHub 身份不存在"))?;
        operation(&mut identity);
        self.replace_identity(Some(identity))
    }

    /// 原子写入身份文件并同步内存缓存。
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

    /// 发送请求并把响应体反序列化为 `T`。
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

    /// 发送请求并忽略响应体。
    async fn request_empty(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> CommandResult<()> {
        self.send(method, path, body, token).await.map(|_| ())
    }

    /// 需要认证的 JSON 请求：会话失效时自动重建并重试一次；
    /// 设备被吊销则只清空本地令牌并原样上抛。
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

    /// 需要认证的空响应请求，失效重试策略与 `auth_json` 相同。
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

    /// 底层请求：拼接 URL、附加令牌与 JSON 体，并接入日志 span。
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> CommandResult<Response> {
        let span = crate::infrastructure::logging::global().map(|logger| {
            logger.operation(
                "community_vps",
                format!("{} {}", method, path.split('?').next().unwrap_or(path)),
            )
        });
        let result = async {
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
        .await;
        crate::infrastructure::logging::finish_span(span, result)
    }
}

/// 把非 2xx 响应转换为 `CommandError`，尽量保留服务端错误码与请求 ID。
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

// 复用同一套 HTTP 与认证栈，向 community 模块提供后端访问能力。
impl crate::infrastructure::community_client::CommunityClient for BeatmapHubService {
    async fn public_get<T: DeserializeOwned>(&self, path: &str) -> CommandResult<T> {
        self.request_json(Method::GET, path, None, None).await
    }

    async fn authenticated_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> CommandResult<T> {
        self.auth_json(method, path, body).await
    }

    async fn authenticated_delete(&self, path: &str) -> CommandResult<()> {
        self.auth_empty(Method::DELETE, path, None).await
    }
}

/// 为未解析出难度列表的谱面集生成“全部难度”占位条目。
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

/// 依次尝试 `COMPUTERNAME` 与 `HOSTNAME` 环境变量，取本机设备名。
fn default_device_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// 展开整条错误因果链并拼接为一段文本，避免丢失底层原因。
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

/// 代理发现顺序：环境变量 → Windows 系统代理设置 → 本机 7890 端口探测。
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

/// 读取 Windows 注册表中的系统代理，优先 `https=`/`http=` 前缀条目。
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

/// 为缺少协议前缀的代理地址补上 `http://`。
fn normalize_http_proxy(value: &str) -> String {
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

/// 构造与服务端约定的规范签名消息；字段顺序与换行不可改动。
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

/// 临时文件加备份回滚的原子写入，避免中断损坏身份文件。
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

/// 校验并归一化曲包分享码：转大写、可带 `BPH-` 前缀、共 6 位，
/// 且字符集排除 0/1/I/L/O 等易混淆字符。
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

/// 校验身份名称长度为 1 到 64 个字符。
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

/// 校验曲包标题（1 到 120 字符）与描述（至多 2000 字符）。
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

// 以下为系统凭据库（keyring）的薄封装，统一错误码并区分“条目不存在”。
fn entry(name: &str) -> CommandResult<Entry> {
    Entry::new(SERVICE, name).map_err(keyring_error)
}
fn write_secret(name: &str, value: &str) -> CommandResult<()> {
    entry(name)?
        .set_secret(value.as_bytes())
        .map_err(keyring_error)
}
/// 读取凭据；条目不存在返回 `None`，内容非 UTF-8 视为损坏。
fn read_secret(name: &str) -> CommandResult<Option<String>> {
    match entry(name)?.get_secret() {
        Ok(value) => String::from_utf8(value)
            .map(Some)
            .map_err(|_| CommandError::new("HUB_CREDENTIAL_ERROR", "BeatmapHub 安全凭据编码无效")),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(keyring_error(error)),
    }
}
/// 删除凭据；条目本就不存在也视为成功。
fn delete_secret(name: &str) -> CommandResult<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(keyring_error(error)),
    }
}
/// 统一把 keyring 错误转换为凭据不可用错误。
fn keyring_error(error: KeyringError) -> CommandError {
    CommandError::new(
        "HUB_CREDENTIAL_ERROR",
        format!("系统安全存储不可用：{error}"),
    )
}
/// 读取或首次生成设备 Ed25519 私钥，Base64URL 编码存放于系统凭据库。
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

    /// 分享码归一化：前缀与大小写宽松，易混淆字符集必须拒绝。
    #[test]
    fn normalizes_share_ids() {
        assert_eq!(normalize_share_id("bph-7k3n9a").unwrap(), "7K3N9A");
        assert!(normalize_share_id("O0I1LL").is_err());
    }

    /// 签名消息的字段与顺序必须与服务端约定一致。
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

    /// Ed25519 签名按 Base64URL 无填充编码固定为 86 字符。
    #[test]
    fn signatures_have_protocol_base64url_length() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let encoded = URL_SAFE_NO_PAD.encode(signing.sign(b"challenge").to_bytes());
        assert_eq!(encoded.len(), 86);
        assert!(!encoded.contains('='));
    }

    /// 依赖本地代理的真实连通性检查，平时默认跳过。
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
