//! 本地 osu! 谱面与皮肤资源扫描、索引和查询。

mod commands;
pub(crate) mod lazer_realm;
mod models;
mod music;
pub(crate) mod parser;
mod service;
mod sources;

pub use commands::*;
pub(crate) use models::BeatmapQuery;
pub use models::{
    LocalBeatmapSummary, LocalClient, LocalSkinSummary, SkinQuery, SkinSort, SortDirection,
    StrainAnalysis, StrainSeries,
};
pub(crate) use music::{MusicAsset, MusicCandidate};
pub use service::LocalAnalysisService;
pub(crate) type CollectionResource = (Option<String>, LocalBeatmapSummary, String, String);
