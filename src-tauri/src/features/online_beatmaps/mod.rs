//! 在线谱面搜索、详情查询、下载与本地导入。

mod artwork_cache;
mod batch;
mod commands;
mod download;
mod models;
pub(crate) mod providers;
mod tools;

// 背景图缓存实例由 `AppState` 持有，这里仅向 crate 内导出类型。
pub(crate) use artwork_cache::OnlineArtworkCache;
// 导出本模块的 Tauri 命令，供 crate 根的命令注册表统一登记。
pub use commands::*;
// 下载请求与进度等前后端共享的数据契约。
pub use models::{BeatmapDownloadItem, BeatmapDownloadProgress, BeatmapDownloadRequest};
