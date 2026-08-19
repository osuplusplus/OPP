//! Data contracts returned by the game-session Tauri commands.

use std::{collections::HashMap, sync::Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{local_analysis::LocalClient, app::models::Ruleset};

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

/// The independently monitored state of one installed osu! client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameClientStatus {
    pub client: LocalClient,
    pub running: bool,
    pub executable: Option<String>,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameStatusSnapshot {
    pub clients: Vec<GameClientStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMediaItem {
    pub client: LocalClient,
    pub path: String,
    pub kind: String,
    pub modified_at: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewReplayItem {
    pub path: String,
    pub file_name: String,
    pub beatmap_title: Option<String>,
    pub username: Option<String>,
    pub renderable: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewReplaysDetected {
    pub client: LocalClient,
    pub started_at: DateTime<Utc>,
    pub detected_at: DateTime<Utc>,
    pub replays: Vec<NewReplayItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayFingerprint {
    pub path: String,
    pub size: u64,
    pub modified_at_millis: u128,
}

pub struct ReplayWatchSession {
    pub started_at: DateTime<Utc>,
    pub before: HashMap<String, ReplayFingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayPayload {
    pub path: String,
    pub file_name: String,
    pub bytes_base64: String,
    pub video_ready: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayMapInfo {
    pub path: String,
    pub beatmap_hash: String,
    pub username: String,
    pub beatmap_id: Option<i32>,
    pub beatmap_resource_id: Option<String>,
    pub beatmap_title: Option<String>,
    pub submitted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameScreenshotPayload {
    pub path: String,
    pub file_name: String,
    pub mime_type: String,
    pub bytes_base64: String,
}

/// In-memory state for the currently launched osu! process.
pub struct GameSessionRuntime {
    pub active: Mutex<Option<GameSessionSummary>>,
}

/// Shared, continuously refreshed process state. It deliberately has no
/// session/account data so externally launched games are represented safely.
pub struct GameMonitorRuntime {
    pub current: Mutex<GameStatusSnapshot>,
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
        }
    }
}
