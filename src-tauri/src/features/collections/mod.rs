//! Stable 与 lazer 收藏的管理、分享、备份和下载。

// 子模块按职责拆分：archive（压缩包导入）、browser（浏览与搜索查询）、
// downloads（缺谱补齐下载）、lazer（lazer 收藏夹的 client.realm 本地读取）、
// service（业务编排与分片持久化）、stable（stable collection.db 同步）、
// task（后台任务的取消与进度）。
mod archive;
mod browser;
mod downloads;
mod lazer;
mod models;
// notebook 中的类型需被 tournament_pools 模块复用，因此放开为 crate 内可见。
pub(crate) mod notebook;
mod service;
mod share;
mod stable;
mod task;
pub use browser::*;

#[cfg(test)]
mod tests;

pub use archive::import_collection_archive;
pub use downloads::{
    get_collection_download_items, install_collection_downloads, open_collection_downloads,
};
pub use models::*;
pub use service::CollectionService;
pub use share::{export_collection_share, import_collection_share, preview_collection_share};
pub use stable::{
    add_collection_entries, create_collection, create_collection_backup, delete_collection,
    enable_collection_stable_sync, get_collection_backup_status, get_collection_sync_status,
    list_collections, refresh_collections, remove_collection_entry, rename_collection,
    restore_collection_backup, write_stable_collections,
};
pub use task::{begin_collection_task, cancel_collection_task};

// 分页查询命令：收藏夹摘要 + 条目分页，避免前端整包拉取大收藏夹数据。
mod paging;
pub use paging::{
    list_collection_summaries, query_collection_entries, refresh_collection_summaries,
};
