//! Event payloads emitted while o!rdr processes a replay.

use serde::Serialize;

/// 单个渲染任务的进度事件载荷，经 `ordr-render-progress` 推送给前端。
#[derive(Debug, Clone, Serialize)]
pub struct ReplayRenderProgress {
    /// o!rdr 分配的渲染任务 ID，前端用它关联自己的提交。
    pub render_id: u32,
    /// 任务状态：`queued` / `rendering` / `completed` / `failed` / `monitor_error`。
    pub status: String,
    /// 面向用户的进度描述文本。
    pub description: String,
    /// 仅在 `completed` 时给出结果视频地址。
    pub video_url: Option<String>,
}
