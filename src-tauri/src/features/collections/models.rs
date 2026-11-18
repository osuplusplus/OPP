//! collections 的领域模型与前后端序列化契约：收藏夹、条目、候选谱面、
//! 同步/备份/分享状态、下载项与任务进度。
use serde::{Deserialize, Serialize};

use crate::features::local_analysis::LocalClient;

/// 收藏夹来源：`Opp` 为 OPP 自建，`Stable`/`Lazer` 为从对应游戏客户端读取的收藏。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CollectionSource {
    Opp,
    Stable,
    Lazer,
}

/// 收藏夹中的一个谱面条目。
///
/// 标识字段按可得性填充：线上来源提供 `beatmap_id`/`beatmapset_id`，
/// 本地来源另有 `checksum`（.osu 文件内容的 MD5）；解析成功前显示字段可能为占位文本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionEntry {
    /// OPP 内部条目标识，跨刷新保持稳定，用于笔记关联与增删操作。
    pub id: String,
    pub beatmap_id: Option<i32>,
    pub beatmapset_id: Option<i32>,
    pub checksum: Option<String>,
    pub ruleset: Option<String>,
    pub difficulty_name: String,
    pub title: String,
    pub artist: String,
    pub creator: String,
    /// 是否已在任一本地客户端中找到该谱面；缺谱补齐流程的目标即让此位为真。
    pub resolved: bool,
}

/// 一个收藏夹（文件夹）及其全部条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionFolder {
    pub id: String,
    pub name: String,
    pub creator: String,
    pub created_at: String,
    pub updated_at: String,
    pub source: CollectionSource,
    /// 只读来源：lazer 收藏夹由 client.realm 本地读取生成，无法写回游戏。
    pub read_only: bool,
    /// 存在待写回 Stable collection.db 的本地变更。
    pub pending_write: bool,
    /// 是否参与 Stable collection.db 同步；赛事图池等 OPP 专属收藏默认不参与。
    #[serde(default = "default_stable_sync")]
    pub stable_sync: bool,
    /// Pool membership and its source metadata commit in the same atomic folder shard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<super::notebook::PoolSnapshot>,
    pub entries: Vec<CollectionEntry>,
    /// 外部来源标识，当前用于按 `source_id` 关联赛事图池收藏夹。
    #[serde(default)]
    pub external_id: Option<String>,
    /// 外部数据指纹（预留字段，当前写入路径均为 `None`）。
    #[serde(default)]
    pub external_fingerprint: Option<String>,
    /// 最近一次从外部客户端读取的时间。
    #[serde(default)]
    pub last_read_at: Option<String>,
    /// 写回 Stable 前自动备份的 collection.db 路径。
    #[serde(default)]
    pub backup_path: Option<String>,
    /// 备份内容指纹（预留字段，当前写入路径均为 `None`）。
    #[serde(default)]
    pub backup_fingerprint: Option<String>,
    /// 用户确认备份无误的时间（预留字段，当前写入路径均为 `None`）。
    #[serde(default)]
    pub backup_confirmed_at: Option<String>,
}

/// 旧数据兼容：序列化内容缺失 `stable_sync` 字段时默认开启同步，维持既有行为。
fn default_stable_sync() -> bool {
    true
}

impl CollectionFolder {
    /// 是否参与 Stable collection.db 写回：排除 lazer 来源与显式关闭同步的收藏夹。
    pub(crate) fn participates_in_stable(&self) -> bool {
        self.source != CollectionSource::Lazer && self.stable_sync
    }
}

/// 待加入收藏夹的候选谱面（压缩包导入、图池同步、下载后解析等场景），
/// 由 `candidate_to_entry` 转换为正式条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionCandidate {
    pub beatmap_id: Option<i32>,
    pub beatmapset_id: Option<i32>,
    pub checksum: Option<String>,
    pub ruleset: Option<String>,
    pub difficulty_name: String,
    pub title: String,
    pub artist: String,
    pub creator: String,
    /// 命中的本地客户端；仅当同时存在 `checksum` 时候选才会被标记为已解析。
    #[serde(default)]
    pub local_client: Option<LocalClient>,
    #[serde(default)]
    pub local_resource_id: Option<String>,
}

/// 单个游戏客户端来源的可用性状态（是否可读、是否只读及提示信息）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSourceStatus {
    pub client: LocalClient,
    pub available: bool,
    pub read_only: bool,
    pub message: String,
    pub refreshed_at: Option<String>,
}

/// 全量快照：所有收藏夹与各来源状态，供前端一次性加载（大收藏夹请改用分页命令）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSnapshot {
    pub folders: Vec<CollectionFolder>,
    pub sources: Vec<CollectionSourceStatus>,
}

/// 收藏同步状态总览。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSyncStatus {
    pub available: bool,
    pub in_sync: bool,
    pub pending_changes: bool,
    /// 游戏侧 collection 数据与上次读取的指纹不一致，即外部被修改过。
    pub game_changed: bool,
    /// 缺失但持有 `beatmapset_id`、可直接在线补齐的条目数。
    pub missing_downloadable_count: usize,
    /// 既不在本地也没有可靠在线标识的条目数。
    pub missing_unresolved_count: usize,
    #[serde(default)]
    pub sources: Vec<CollectionSourceSyncStatus>,
}

/// 单个客户端来源的同步与备份状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSourceSyncStatus {
    pub client: LocalClient,
    pub available: bool,
    /// 外部（游戏）数据较上次读取发生变化。
    pub external_changed: bool,
    pub pending_write: bool,
    pub backup_available: bool,
    pub backup_confirmed: bool,
    pub backup_count: usize,
    pub latest_backup: Option<String>,
}
impl Default for CollectionSourceSyncStatus {
    // 手写默认值：全新环境按 Stable 处理，且视为不可用、无备份。
    fn default() -> Self {
        Self {
            client: LocalClient::Stable,
            available: false,
            external_changed: false,
            pending_write: false,
            backup_available: false,
            backup_confirmed: false,
            backup_count: 0,
            latest_backup: None,
        }
    }
}

/// 读取游戏收藏夹前的一次差异项：新增/更新/删除/保留各列表的一个条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRefreshItem {
    /// 游戏侧（新增）或 OPP 侧（更新/删除/保留）的收藏夹名称。
    pub name: String,
    /// OPP 侧文件夹来源；新增项游戏侧尚无对应文件夹，用来源区分展示。
    pub source: CollectionSource,
    /// 新增/删除项在对应一侧的条目数。
    #[serde(default)]
    pub entry_count: usize,
    /// 更新项：游戏侧相对 OPP 新增的条目数。
    #[serde(default)]
    pub entries_added: usize,
    /// 更新项：游戏侧相对 OPP 移除的条目数。
    #[serde(default)]
    pub entries_removed: usize,
    /// 更新项：OPP 存在未写回修改，确认后将被游戏侧覆盖。
    #[serde(default)]
    pub pending_overwrite: bool,
}

/// 「读取 Stable / lazer」的差异预览：只读计算，不改动任何状态或游戏文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRefreshPreview {
    pub client: LocalClient,
    /// 任一类别非空即视为有差异。
    pub has_changes: bool,
    /// 游戏侧有、OPP 侧没有的收藏夹。
    pub additions: Vec<CollectionRefreshItem>,
    /// 同名/同 ID 但条目集合不同的收藏夹。
    pub updates: Vec<CollectionRefreshItem>,
    /// 游戏侧已删除、确认后将一并从 OPP 移除的收藏夹。
    pub removals: Vec<CollectionRefreshItem>,
    /// 游戏侧已删除但 OPP 有未写回修改，应用时保留不动的收藏夹（仅 stable）。
    pub preserved: Vec<CollectionRefreshItem>,
}

/// 导入分享码前的预览信息，按本地拥有情况与可下载性给出统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSharePreview {
    pub name: String,
    pub creator: String,
    pub created_at: String,
    pub exported_at: String,
    pub entries: Vec<CollectionEntry>,
    /// 本地任一客户端已拥有的条目数。
    pub available_count: usize,
    /// 未拥有但可凭 `beatmapset_id` 在线补齐的条目数。
    pub downloadable_count: usize,
    /// 两者皆非的条目数。
    pub unresolved_count: usize,
}

/// 缺谱补齐下载队列中的一项，以谱面集（beatmapset）为单位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionDownloadItem {
    pub beatmapset_id: i32,
    pub artist: String,
    pub title: String,
}

/// 写回客户端收藏文件的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionWriteResult {
    pub written_folders: usize,
    /// 因缺少 `checksum` 无法写入 collection.db 的条目数（该文件只存 MD5 列表）。
    pub skipped_entries: usize,
    pub backup_path: Option<String>,
}

/// 删除收藏夹的结果：本地删除总是成功，区别在游戏侧 collection.db 是否同步。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionDeleteResult {
    pub name: String,
    /// 本次删除已同步写入 collection.db（游戏未运行且无指纹冲突）。
    pub stable_removed: bool,
    /// osu! 运行中：删除已记录，将在游戏退出后由监视器自动写回移除。
    pub deferred: bool,
    /// 随本次删除一并写回的收藏夹总数（未写回时为 0）。
    pub written_folders: usize,
}

/// 客户端收藏文件（如 collection.db）的备份状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionBackupStatus {
    pub client: LocalClient,
    pub target: Option<String>,
    pub fingerprint: Option<String>,
    pub backups: Vec<String>,
    pub latest: Option<String>,
}

/// 补齐下载安装到本地后的统计；`extracted_sets` 为本次解压进 Songs 的曲包数
/// （已存在同前缀目录而跳过的不计入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInstallResult {
    pub installed_sets: usize,
    pub extracted_sets: usize,
    pub resolved_entries: usize,
    pub unresolved_entries: usize,
}

/// 后台任务进度事件的负载，经 `collection-task-progress` 事件广播给前端。
#[derive(Debug, Clone, Serialize)]
pub struct CollectionTaskProgress {
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub message: String,
}
