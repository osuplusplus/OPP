//! 本地 osu! 谱面与皮肤资源扫描、索引和查询。

// Tauri 命令入口：反序列化输入、按任务类型调度线程并调用服务层。
mod commands;
// lazer `client.realm` Realm 数据库的快照式读取（谱面集、皮肤、成绩表）。
pub(crate) mod lazer_realm;
// 与前端共享的领域模型和序列化契约。
mod models;
// 音乐播放资源描述符；索引被释放后仍可独立使用。
mod music;
// `.osu` 谱面与 `skin.ini` 解析、难度与 strain 计算。
pub(crate) mod parser;
// 扫描、索引、查询与资源访问的业务编排（目录模块）。
mod service;
// stable / lazer 安装与数据目录探测及用户覆盖路径持久化。
mod sources;

pub use commands::*;
pub(crate) use models::BeatmapQuery;
pub use models::{
    LocalBeatmapSummary, LocalClient, LocalPresenceState, LocalSkinSummary, SkinQuery, SkinSort,
    SortDirection, StrainAnalysis, StrainSeries,
};
pub(crate) use music::{MusicAsset, MusicCandidate};
pub use service::LocalAnalysisService;
/// 供收藏等功能做轻量联表的谱面资源元组：
/// `(beatmap_md5, 谱面摘要, tags, source)`，不触发 `.osu` 重读。
pub(crate) type CollectionResource = (Option<String>, LocalBeatmapSummary, String, String);
