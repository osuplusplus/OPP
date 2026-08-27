//! Event payloads emitted while o!rdr processes a replay.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ReplayRenderProgress {
    pub render_id: u32,
    pub status: String,
    pub description: String,
    pub video_url: Option<String>,
}
