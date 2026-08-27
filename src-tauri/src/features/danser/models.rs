use std::{
    collections::{HashSet, VecDeque},
    sync::{Mutex, atomic::AtomicBool},
};

use serde::{Deserialize, Serialize};

use crate::{domain::DanserRenderPreferences, features::local_analysis::LocalClient};

#[derive(Debug, Clone, Serialize)]
pub struct DanserStatus {
    pub available: bool,
    pub executable_path: Option<String>,
    pub ffmpeg_available: bool,
    pub profiles: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DanserRenderJob {
    pub id: String,
    pub replay_path: String,
    pub status: String,
    pub progress: u8,
    pub description: String,
    pub output_path: Option<String>,
    pub queue_position: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DanserRenderProgress {
    pub id: String,
    pub replay_path: String,
    pub status: String,
    pub progress: u8,
    pub description: String,
    pub output_path: Option<String>,
    pub queue_position: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DanserEnqueueRequest {
    pub client: LocalClient,
    pub replay_paths: Vec<String>,
    pub preferences: DanserRenderPreferences,
}

#[derive(Clone)]
pub(super) struct DanserTask {
    pub id: String,
    pub replay_path: String,
    pub preferences: DanserRenderPreferences,
}

#[derive(Default)]
pub struct DanserRuntime {
    pub(super) queue: Mutex<VecDeque<DanserTask>>,
    pub(super) jobs: Mutex<Vec<DanserRenderJob>>,
    pub(super) cancelled: Mutex<HashSet<String>>,
    pub(super) worker_running: AtomicBool,
}
