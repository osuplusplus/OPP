//! 相似谱面索引、特征查询与推荐。

mod commands;
pub mod dataset;
mod models;
mod query;
mod recommendation;
mod source;

pub use commands::{
    configure_similarity_index, get_similarity_index_status, query_similar_beatmaps,
    recommend_similar_beatmaps,
};
pub use dataset::SimilarityRuntime;
