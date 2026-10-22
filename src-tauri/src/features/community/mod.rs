//! 社区功能：赛事浏览与房间的创建/管理，
//! 经 `CommunityClient` 复用 BeatmapHub 的认证与传输。
mod commands;
mod models;
mod service;
pub use commands::*;
