//! 音乐播放的前后端数据契约：播放模式、曲目信息、播放器状态快照、
//! 队列请求与控制指令，以及窗口会话的持久化结构。
use crate::{
    domain::Ruleset,
    features::local_analysis::{BeatmapQuery, LocalClient, MusicAsset},
};
use serde::{Deserialize, Serialize};

/// 播放模式，决定曲目结束或队尾之后的导航行为；序列化为 `snake_case`。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    // 顺序播放：播到队尾即停止。
    Sequential,
    // 列表循环：队尾之后回绕到队首（默认）。
    #[default]
    RepeatAll,
    // 单曲循环：自然播完时重播当前曲。
    RepeatOne,
    // 随机：洗牌袋保证每轮内不重复。
    Shuffle,
}

/// 队列中的一首曲目，对应本地合并后的一个谱面集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    // 本地拥有该曲目音频的客户端，用于展示来源并作为备用播放源。
    pub clients: Vec<LocalClient>,
}

/// 当前播放资源在本地谱库中的定位信息，供前端跳转到对应谱面。
#[derive(Debug, Clone, Serialize)]
pub struct MusicLocation {
    pub client: LocalClient,
    pub ruleset: Ruleset,
    pub set_key: String,
    pub resource_id: String,
}

/// 内部的队列条目：曲目信息加上按优先级排序的各客户端音频资源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Track {
    pub info: TrackInfo,
    pub assets: Vec<MusicAsset>,
}

/// 发布给前端的播放器状态快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    /// 状态版本号，每次发布时自增，用于识别事件的先后顺序。
    pub version: u64,
    /// 队列内容版本号，队列增删改时自增，供分页结果核对队列是否已变化。
    pub queue_version: u64,
    pub current: Option<TrackInfo>,
    /// 当前实际播放的谱面资源 ID，来自命中的那个音频资源。
    pub resource_id: Option<String>,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub mode: PlayMode,
    pub total: usize,
    /// 非致命错误的提示文案，成功开播或清空队列时重置。
    pub notice: Option<String>,
    /// 是否处于迷你播放器模式；该标志由窗口协调器拥有并写回。
    pub mini: bool,
    /// 正在运行的后台任务数；迷你模式下归零才允许回收主窗口与索引。
    pub background_tasks: usize,
}
impl Default for PlayerState {
    fn default() -> Self {
        Self {
            version: 0,
            queue_version: 0,
            current: None,
            resource_id: None,
            playing: false,
            position: 0.0,
            duration: 0.0,
            volume: 0.65,
            mode: PlayMode::default(),
            total: 0,
            notice: None,
            mini: false,
            background_tasks: 0,
        }
    }
}

/// 队列构建请求：按客户端、查询、收藏夹或指定资源过滤候选音频。
#[derive(Debug, Default, Deserialize)]
pub struct QueueRequest {
    /// 仅保留该客户端的候选音频。
    pub client: Option<LocalClient>,
    /// 传给本地谱库扫描的过滤查询。
    pub query: Option<BeatmapQuery>,
    /// 收藏夹 ID，仅保留能被其中条目匹配到的曲目。
    pub collection_id: Option<String>,
    /// 指定的难度资源 ID，命中者作为首选播放源。
    pub resource_id: Option<String>,
    /// 追加模式：不清空现有队列，按曲目 ID 原位合并。
    #[serde(default)]
    pub append: bool,
    /// 从音频内嵌的预览点开始播放，而非从头播放。
    #[serde(default)]
    pub preview: bool,
}

/// 播放控制指令：前端按钮与系统媒体键事件统一收敛为该枚举，
/// 以 `{"action": "next"}` 形式反序列化。
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Control {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Seek { seconds: f64 },
    Volume { value: f32 },
    Mode { mode: PlayMode },
    Select { id: String },
    Remove { id: String },
    Clear,
}

/// 队列分页结果；`version` 为查询时刻的 `queue_version`，供前端校验缓存。
#[derive(Debug, Serialize)]
pub struct QueuePage {
    pub items: Vec<TrackInfo>,
    pub total: usize,
    pub version: u64,
}

/// 持久化到 `window.json` 的窗口会话：主窗口几何与路由、迷你播放器位置与置顶状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowSession {
    /// 主窗口当前前端路由，主窗口被回收后重建时恢复到原页面。
    pub route: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub maximized: bool,
    // 迷你播放器上次出现的位置。
    pub mini_x: Option<i32>,
    pub mini_y: Option<i32>,
    /// 迷你播放器是否置顶；`None` 时按置顶处理。
    pub pinned: Option<bool>,
}
