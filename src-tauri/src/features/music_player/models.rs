use crate::{
    domain::Ruleset,
    features::local_analysis::{BeatmapQuery, LocalClient, MusicAsset},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    Sequential,
    #[default]
    RepeatAll,
    RepeatOne,
    Shuffle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub clients: Vec<LocalClient>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MusicLocation {
    pub client: LocalClient,
    pub ruleset: Ruleset,
    pub set_key: String,
    pub resource_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Track {
    pub info: TrackInfo,
    pub assets: Vec<MusicAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub version: u64,
    pub queue_version: u64,
    pub current: Option<TrackInfo>,
    pub resource_id: Option<String>,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub mode: PlayMode,
    pub total: usize,
    pub notice: Option<String>,
    pub mini: bool,
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

#[derive(Debug, Default, Deserialize)]
pub struct QueueRequest {
    pub client: Option<LocalClient>,
    pub query: Option<BeatmapQuery>,
    pub collection_id: Option<String>,
    pub resource_id: Option<String>,
    #[serde(default)]
    pub append: bool,
    #[serde(default)]
    pub preview: bool,
}

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

#[derive(Debug, Serialize)]
pub struct QueuePage {
    pub items: Vec<TrackInfo>,
    pub total: usize,
    pub version: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowSession {
    pub route: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub maximized: bool,
    pub mini_x: Option<i32>,
    pub mini_y: Option<i32>,
    pub pinned: Option<bool>,
}
