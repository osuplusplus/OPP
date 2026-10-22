//! 相似谱面索引、特征查询与推荐。

mod commands;
pub mod dataset;
mod models;
mod query;
mod recommendation;
pub(crate) mod source;

// 对 `crate::commands` 注册表暴露的 Tauri 命令边界（实现见 `commands.rs`）。
pub use commands::{
    configure_similarity_index, get_similarity_index_status, query_similar_beatmaps,
    recommend_similar_beatmaps,
};
// 共享的数据集缓存运行时，由 `AppState` 持有并跨命令复用。
pub use dataset::SimilarityRuntime;
