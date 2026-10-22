//! Small resource descriptors that remain usable after the analysis index is released.
use super::LocalClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 一首本地音乐的定位信息：保存的是磁盘路径而非索引引用，
/// 因此索引被释放后仍可继续播放。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MusicAsset {
    pub client: LocalClient,
    /// 关联谱面的资源 ID。
    pub resource_id: String,
    /// 资源根目录：stable 为谱面集目录，lazer 为 `files/` 仓库根。
    pub root: PathBuf,
    /// 音频文件路径。
    pub audio: PathBuf,
    /// 封面图路径；谱面未声明背景时为 `None`。
    pub artwork: Option<PathBuf>,
    /// 对应 `.osu` 文件路径，用于读取 `PreviewTime` 等元数据。
    pub beatmap: PathBuf,
}

/// 音乐模式的候选条目：谱面元数据加上可选的资源定位与查询匹配结果。
#[derive(Debug, Clone)]
pub(crate) struct MusicCandidate {
    pub client: LocalClient,
    pub resource_id: String,
    pub set_key: String,
    /// 在线谱面集 ID（仅保留正值）。
    pub set_id: Option<i32>,
    pub beatmap_id: Option<i32>,
    /// `.osu` 内容的 MD5，与游戏内收藏的标识方式一致。
    pub checksum: Option<String>,
    pub title: String,
    pub artist: String,
    /// 是否命中调用方给定的 `BeatmapQuery`；未提供查询时恒为 `true`。
    pub matches: bool,
    /// 音频路径可定位时才携带（封面允许缺失）。
    pub asset: Option<MusicAsset>,
}
