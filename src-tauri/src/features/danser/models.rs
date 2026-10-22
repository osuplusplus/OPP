//! danser 队列的数据模型：对外状态展示、任务定义与共享运行时结构。
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{Mutex, atomic::AtomicBool},
};

use serde::{Deserialize, Serialize};

use crate::{domain::DanserRenderPreferences, features::local_analysis::LocalClient};

/// Danser 环境就绪状态，供设置页展示探测结果与提示文案。
#[derive(Debug, Clone, Serialize)]
pub struct DanserStatus {
    pub available: bool,
    pub executable_path: Option<String>,
    pub ffmpeg_available: bool,
    pub profiles: Vec<String>,
    pub message: String,
}

/// 队列中一个渲染任务的对外状态。
#[derive(Debug, Clone, Serialize)]
pub struct DanserRenderJob {
    pub id: String,
    pub replay_path: String,
    /// 任务状态：`queued` / `running` / `completed` / `failed` / `cancelled`。
    pub status: String,
    pub progress: u8,
    pub description: String,
    pub output_path: Option<String>,
    /// 排队位次（从 1 开始）；渲染中或已终态时为 `None`。
    pub queue_position: Option<usize>,
}

/// `danser-render-progress` 事件负载，字段与 `DanserRenderJob` 保持一致。
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

/// 批量入队请求：一次提交多个回放与共享的渲染偏好。
#[derive(Debug, Clone, Deserialize)]
pub struct DanserEnqueueRequest {
    /// 回放所属客户端（Stable / Lazer），决定资源目录的解析与暂存方式。
    pub client: LocalClient,
    pub replay_paths: Vec<String>,
    pub preferences: DanserRenderPreferences,
}

#[derive(Clone)]
/// Lazer：入队时为 Danser 物化的 Stable 布局目录，settings 的
/// OsuSongsDir / OsuSkinsDir 指向这里。
pub(super) struct DanserLazerStage {
    pub songs_dir: PathBuf,
    pub skins_root: PathBuf,
}

/// 队列内部任务：入队时的偏好快照，加上执行时才补充的目录信息。
#[derive(Clone)]
pub(super) struct DanserTask {
    pub id: String,
    pub client: LocalClient,
    pub replay_path: String,
    pub preferences: DanserRenderPreferences,
    pub lazer_stage: Option<DanserLazerStage>,
    /// `(谱面目录, 皮肤目录)`：Stable 任务入队时解析；Lazer 任务执行时物化到
    /// `lazer_stage`，此处保持 `None`。
    pub stable_directories: Option<(PathBuf, PathBuf)>,
}

/// 渲染队列的共享运行时状态，挂在 `AppState` 上跨命令访问。
#[derive(Default)]
pub struct DanserRuntime {
    pub(super) queue: Mutex<VecDeque<DanserTask>>,
    pub(super) jobs: Mutex<Vec<DanserRenderJob>>,
    /// 已请求取消的任务 ID；渲染中的任务由工作线程轮询此集合后终止子进程。
    pub(super) cancelled: Mutex<HashSet<String>>,
    /// 工作线程存活标志，保证同一时刻只有一个任务在渲染。
    pub(super) worker_running: AtomicBool,
}

impl DanserRuntime {
    pub(crate) fn is_busy(&self) -> bool {
        self.worker_running
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}
