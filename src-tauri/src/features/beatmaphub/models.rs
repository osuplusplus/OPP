//! BeatmapHub 与服务端交互的数据契约（请求/响应 DTO）。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 本地设备身份元数据，持久化在 `<应用数据目录>/beatmaphub/identity.json`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityMetadata {
    /// Base64URL（无填充）编码的 Ed25519 公钥，即设备指纹。
    pub public_key: String,
    /// 认领的 osu 用户 ID。
    pub user_id: String,
    /// 服务端分配的设备 ID。
    pub device_id: String,
    /// 认领的 osu 用户名。
    pub display_name: String,
    /// 上报的本机设备名称。
    pub device_name: String,
    /// 会话过期时间；登出后置 `None`，身份保留但需重新认证。
    pub expires_at: Option<DateTime<Utc>>,
}

/// 认证状态快照，供前端展示连接状态与身份信息。
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    /// 本地是否存在设备身份文件（不代表会话仍可用）。
    pub has_identity: bool,
    /// 访问令牌存在且距过期超过 10 秒。
    pub connected: bool,
    pub public_key: Option<String>,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub display_name: Option<String>,
    pub device_name: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Hub 侧用户摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubUser {
    pub id: String,
    pub display_name: String,
}

/// bootstrap 响应中内嵌的设备摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubDeviceSummary {
    pub id: String,
    pub device_name: String,
}

/// `/auth/bootstrap` 的响应：访问令牌与认领的设备、身份信息。
#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapResponse {
    pub access_token: String,
    /// 令牌有效秒数。
    pub expires_in: u64,
    pub device: HubDeviceSummary,
    pub identity: BootstrapIdentity,
}

/// bootstrap 返回的已认领 osu 身份。
#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapIdentity {
    pub claimed_osu_user_id: String,
    pub claimed_username: String,
}

/// `/auth/refresh` 的响应。
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub expires_in: u64,
}

/// `/auth/refresh/nonce` 的响应，nonce 用于刷新签名防重放。
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshNonceResponse {
    pub nonce: String,
}

/// 个人资料中的设备条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub device_name: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    /// 设备被吊销的时间，未吊销时为 `None`。
    pub revoked_at: Option<DateTime<Utc>>,
}

/// `/auth/me` 的响应：当前用户、本设备 ID 与全部已绑定设备。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub user: HubUser,
    pub current_device_id: String,
    pub devices: Vec<Device>,
}

/// 曲包所有者摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackOwner {
    pub id: String,
    pub display_name: String,
}

/// 曲包评分聚合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRating {
    /// 平均分，尚无评分时为 `None`。
    pub average: Option<f64>,
    /// 评分人数。
    pub count: u64,
}

/// 点赞/评论计数包装，配合 `#[serde(default)]` 兼容缺失字段的旧响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackCount {
    pub count: u64,
}

/// 当前查看者对曲包的个性化状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackViewer {
    /// 查看者自己的评分，未评分为 `None`。
    pub rating: Option<u8>,
    /// 是否已收藏。
    pub favorited: bool,
    /// 后加字段，旧响应缺失时按 `false` 反序列化。
    #[serde(default)]
    pub liked: bool,
    /// 是否允许当前查看者编辑该曲包。
    pub can_edit: bool,
}

/// 评论作者摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackCommentUser {
    pub id: String,
    pub display_name: String,
}

/// 曲包评论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackComment {
    pub id: String,
    pub user: PackCommentUser,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 评论列表响应包装。
#[derive(Debug, Clone, Deserialize)]
pub struct PackCommentsResponse {
    pub comments: Vec<PackComment>,
}

/// 搜索结果包装。
#[derive(Debug, Clone, Deserialize)]
pub struct PackSearchResponse {
    pub packs: Vec<Pack>,
}

/// 曲包数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub is_private: bool,
    pub owner: PackOwner,
    /// 曲包收录的谱面集 ID 列表。
    pub beatmapset_ids: Vec<i32>,
    /// 星数范围，服务端未提供时为 `None`。
    #[serde(default)]
    pub stars_min: Option<f64>,
    #[serde(default)]
    pub stars_max: Option<f64>,
    pub manifest_hash: String,
    pub rating: PackRating,
    #[serde(default)]
    pub likes: PackCount,
    #[serde(default)]
    pub comments: PackCount,
    /// 查看者视角信息，匿名访问时为 `None`。
    pub viewer: Option<PackViewer>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 曲包预览：结合本地索引区分已有与缺失的谱面集。
#[derive(Debug, Clone, Serialize)]
pub struct PackPreview {
    pub pack: Pack,
    /// 本地已拥有的谱面集 ID。
    pub locally_available_ids: Vec<i32>,
    /// 本地缺失的谱面集 ID。
    pub missing_ids: Vec<i32>,
}

/// 发布结果。
#[derive(Debug, Clone, Serialize)]
pub struct PublishResult {
    /// 新建曲包的 ID。
    pub id: String,
    /// 实际计入曲包的谱面集数量。
    pub included: usize,
    /// 因缺少谱面集 ID 被跳过的收藏夹条目数。
    pub skipped: usize,
}

/// 前端解析后回传的单个难度信息，用于导入。
#[derive(Debug, Clone, Deserialize)]
pub struct ResolvedBeatmap {
    pub id: i32,
    pub checksum: Option<String>,
    pub mode: Option<String>,
    pub version: Option<String>,
}

/// 前端解析后回传的谱面集元数据，用于导入。
#[derive(Debug, Clone, Deserialize)]
pub struct ResolvedBeatmapset {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub creator: String,
    #[serde(default)]
    pub beatmaps: Vec<ResolvedBeatmap>,
}

/// 曲包导入结果。
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    /// 新建的本地收藏夹 ID。
    pub folder_id: String,
    /// 曲包包含的谱面集总数。
    pub imported_sets: usize,
    /// 写入收藏夹的条目数（每难度一条，未解析集合一条占位）。
    pub imported_entries: usize,
    /// 未能解析出难度列表的谱面集数量。
    pub unresolved_sets: usize,
}

/// `/auth/challenge` 的响应；下划线字段仅为载荷兼容，不参与签名。
#[derive(Debug, Deserialize)]
pub struct ChallengeResponse {
    pub challenge_id: String,
    #[serde(rename = "message")]
    pub _message: String,
    #[serde(default)]
    #[serde(rename = "expires_at")]
    pub _expires_at: Option<DateTime<Utc>>,
}

/// 创建曲包的响应，仅返回新曲包 ID。
#[derive(Debug, Deserialize)]
pub struct CreatePackResponse {
    pub id: String,
}

/// 上传谱面备份的响应，携带服务端去重后的统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupUploadResult {
    /// 去重后实际保存的谱面集数量。
    pub count: usize,
    /// 被去重丢弃的谱面集数量。
    pub skipped: usize,
    /// 去重后 ID 列表 JSON 序列化的 sha256。
    pub manifest_hash: String,
    /// 首次备份时间（RFC 3339）。
    pub created_at: String,
    /// 最近一次上传时间（RFC 3339）。
    pub updated_at: String,
}

/// 云端谱面备份内容，附带与本地库的差异统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapBackup {
    /// 备份归属的 PackHub 用户 ID。
    pub owner_user_id: String,
    /// 去重升序的谱面集 ID 列表。
    pub beatmapset_ids: Vec<i32>,
    /// 备份内谱面集总数。
    pub count: usize,
    /// 去重后 ID 列表 JSON 序列化的 sha256。
    pub manifest_hash: String,
    /// 首次备份时间（RFC 3339）。
    pub created_at: String,
    /// 最近一次上传时间（RFC 3339）。
    pub updated_at: String,
    /// 备份中本地已拥有的谱面集数量；读取时由本地计算，服务端不返回。
    #[serde(default)]
    pub locally_available_count: usize,
    /// 备份中本地缺失的谱面集数量；读取时由本地计算，服务端不返回。
    #[serde(default)]
    pub missing_count: usize,
}

/// 把云端备份恢复为本地收藏夹的结果。
#[derive(Debug, Clone, Serialize)]
pub struct BackupRestoreResult {
    /// 新建收藏夹的 ID。
    pub folder_id: String,
    /// 新建收藏夹的名称。
    pub folder_name: String,
    /// 备份内谱面集总数。
    pub total_sets: usize,
    /// 本地已有、按难度展开为完整条目的谱面集数。
    pub local_sets: usize,
    /// 本地缺失、以占位条目导入（可批量补齐下载）的谱面集数。
    pub missing_sets: usize,
    /// 实际写入的收藏条目数。
    pub entries: usize,
}

/// 服务端错误响应的外层包装。
#[derive(Debug, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ApiError,
}

/// 错误正文：机器可读的 `code` 与人类可读的 `message`。
#[derive(Debug, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
