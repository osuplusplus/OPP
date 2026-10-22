//! BeatmapHub（PackHub）谱面站集成：Ed25519 设备身份认证、
//! 曲包的浏览/发布/导入，并为 community 模块提供 HTTP 客户端复用。
mod commands;
mod models;
mod service;

pub use commands::*;
pub use service::BeatmapHubService;
