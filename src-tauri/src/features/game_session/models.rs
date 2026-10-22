//! Data contracts returned by the game-session Tauri commands.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{domain::Ruleset, features::local_analysis::LocalClient};

/// 会话起点/终点各取一次的用户资料与统计数据快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSnapshot {
    pub captured_at: DateTime<Utc>,
    pub username: String,
    pub pp: Option<f64>,
    pub ranked_score: Option<u64>,
    pub hit_accuracy: Option<f64>,
    pub total_hits: Option<u64>,
    pub total_score: Option<u64>,
}

/// 一次游玩会话的完整摘要，含起止两份资料快照；`running` 由命令层按进程
/// 实际状态修正，客户端退出后补写 `ended_at` 与 `end`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSessionSummary {
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub ruleset: Ruleset,
    pub client: String,
    pub executable: String,
    pub start: UserSnapshot,
    pub end: Option<UserSnapshot>,
    pub running: bool,
}

/// 批量打开曲包的结果统计与失败原因列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapOpenResult {
    pub opened: usize,
    pub failed: usize,
    pub failures: Vec<String>,
}

/// The independently monitored state of one installed osu! client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameClientStatus {
    pub client: LocalClient,
    pub running: bool,
    pub executable: Option<String>,
    pub detected_at: DateTime<Utc>,
}

/// 一轮监视扫描得到的状态快照：stable 与 lazer 各一项，顺序固定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameStatusSnapshot {
    pub clients: Vec<GameClientStatus>,
}

/// 媒体库条目：`kind` 为 `replay` 或 `screenshot`，`modified_at` 为 RFC3339。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMediaItem {
    pub client: LocalClient,
    pub path: String,
    pub kind: String,
    pub modified_at: Option<String>,
    pub size: u64,
}

/// 会话期间新检测到的回放：`renderable` 为 false 时 `reason` 说明原因
/// （如模式不受支持、谱面未索引）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewReplayItem {
    pub path: String,
    pub file_name: String,
    pub beatmap_title: Option<String>,
    pub username: Option<String>,
    pub renderable: bool,
    pub reason: Option<String>,
}

/// `new-replays-detected` 事件载荷：会话起止时间与本次新增的回放列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewReplaysDetected {
    pub client: LocalClient,
    pub started_at: DateTime<Utc>,
    pub detected_at: DateTime<Utc>,
    pub replays: Vec<NewReplayItem>,
}

/// 回放文件指纹：仅取自文件系统元数据，不读取文件内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayFingerprint {
    pub path: String,
    pub size: u64,
    pub modified_at_millis: u128,
}

/// 客户端启动时记录的回放指纹基线，退出时与之差分得出新回放。
pub struct ReplayWatchSession {
    pub started_at: DateTime<Utc>,
    pub before: HashMap<String, ReplayFingerprint>,
}

/// 回放读取结果：`bytes_base64` 为原始 `.osr` 字节；`video_ready` 恒为
/// false，视频需另行经 o!rdr 生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayPayload {
    pub path: String,
    pub file_name: String,
    pub bytes_base64: String,
    pub video_ready: bool,
    pub note: String,
}

/// 回放元数据与本地索引匹配结果；`submitted` 表示索引能给出已上架谱面的
/// `beatmap_id`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayMapInfo {
    pub ruleset: Ruleset,
    pub path: String,
    pub beatmap_hash: String,
    pub username: String,
    pub beatmap_id: Option<i32>,
    pub beatmap_resource_id: Option<String>,
    pub beatmap_title: Option<String>,
    pub submitted: bool,
}

/// 截图读取结果：`mime_type` 按扩展名推断，内容以 Base64 返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameScreenshotPayload {
    pub path: String,
    pub file_name: String,
    pub mime_type: String,
    pub bytes_base64: String,
}

/// In-memory state for the currently launched osu! process.
pub struct GameSessionRuntime {
    /// 当前活跃会话摘要（命令层写入，轮询时按进程状态修正）。
    pub active: Mutex<Option<GameSessionSummary>>,
    /// 文件选择器放行的回放白名单；允许读取媒体目录之外的已选文件。
    pub selected_replays: Mutex<HashSet<PathBuf>>,
}

/// Shared, continuously refreshed process state. It deliberately has no
/// session/account data so externally launched games are represented safely.
pub struct GameMonitorRuntime {
    /// 监视循环维护的最新状态快照（每秒刷新）。
    pub current: Mutex<GameStatusSnapshot>,
    /// 每个客户端的回放差分基线会话。
    pub replay_sessions: Mutex<HashMap<LocalClient, ReplayWatchSession>>,
}

impl Default for GameMonitorRuntime {
    fn default() -> Self {
        Self {
            current: Mutex::new(GameStatusSnapshot {
                clients: Vec::new(),
            }),
            replay_sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for GameSessionRuntime {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
            selected_replays: Mutex::new(HashSet::new()),
        }
    }
}
