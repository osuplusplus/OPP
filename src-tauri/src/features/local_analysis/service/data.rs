//! Internal index data and persistence for local analysis.
//!
//! Keeping the on-disk schema here makes cache compatibility explicit and keeps
//! the service focused on orchestration rather than serialization details.
//!
//! 磁盘形态为 JSON（主文件 + `.bak` 备份 + `.tmp` 临时文件）；
//! 标注 `#[serde(skip)]` 的派生索引不参与序列化，载入后统一重建。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::CommandResult;

use super::super::{
    models::{
        BeatmapSort, LocalBeatmapDetail, LocalBeatmapSummary, LocalClient, LocalLibrarySummary,
        LocalSkinAssetSummary, LocalSkinDetail, ScanDiagnostic, SkinSort,
    },
    parser::DIFFICULTY_ALGORITHM,
};
use super::lazer_realm::LazerRealmFile;
use super::query::{compare_beatmaps, compare_skins};

/// Bump this only when a serialized [`LocalIndex`] can no longer be read safely.
/// 版本不匹配的缓存会在 [`load_index`] 中被当作缓存未命中丢弃。
pub(super) const INDEX_SCHEMA: u32 = 7;

/// 文件指纹，增量扫描据此判断文件是否需要重新解析。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct FileStamp {
    /// 文件大小，单位字节。
    pub(super) bytes: u64,
    /// 修改时间，Unix 纪元起的毫秒数；取不到时为 0。
    pub(super) modified_ms: u128,
}

/// 单个索引条目的业务载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum IndexedData {
    /// 被扫描规则排除的文件：保留占位但不参与查询。
    Ignored,
    /// 谱面：`summary` 供列表与查询，`detail` 装箱以控制枚举整体体积。
    Beatmap {
        summary: LocalBeatmapSummary,
        detail: Box<LocalBeatmapDetail>,
    },
    /// 皮肤：仅详情（内含摘要）。
    Skin { detail: LocalSkinDetail },
}

/// 索引中的一条资源记录，对应一个谱面文件或一个皮肤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IndexedEntry {
    /// 条目稳定标识，与数据库快照的行键一一对应。
    pub(super) key: String,
    /// 条目文件在磁盘上的物理路径。
    pub(super) physical_path: PathBuf,
    /// 变更检测用的文件指纹。
    pub(super) stamp: FileStamp,
    pub(super) content_hash: Option<String>,
    /// 谱面文件内容的 MD5；查找表按小写归一，支持按 MD5 定位谱面。
    #[serde(default)]
    pub(super) beatmap_md5: Option<String>,
    /// Lazer：该条目所属谱面集 / Skin 在 Realm 中登记的完整文件清单
    /// （原始文件名 + 内容哈希），用于按需解析背景、音频与皮肤资源。
    #[serde(default)]
    pub(super) lazer_files: Option<Vec<LazerRealmFile>>,
    pub(super) data: IndexedData,
    /// 扫描该条目时产生的诊断（解析失败等）。
    pub(super) diagnostics: Vec<ScanDiagnostic>,
}

/// 一次完整扫描得到的全量索引；既是内存查询结构，也是 JSON 与 SQLite 的持久化载体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LocalIndex {
    /// 预生成的搜索文本，与 `entries` 按下标对齐；载入后重建。
    #[serde(skip)]
    pub(super) search_fields: Vec<Option<SearchText>>,
    /// `resource_id` → `entries` 下标；载入后重建。
    #[serde(skip)]
    pub(super) resource_lookup: BTreeMap<String, usize>,
    /// `beatmap_id` → `entries` 下标（同 ID 取首个条目）；载入后重建。
    #[serde(skip)]
    pub(super) beatmap_id_lookup: BTreeMap<i32, usize>,
    /// 库内出现过的 `beatmap_set_id` 集合；载入后重建。
    #[serde(skip)]
    pub(super) set_id_lookup: BTreeSet<i32>,
    /// 结构版本号，载入时必须等于 `INDEX_SCHEMA`。
    pub(super) schema: u32,
    /// 生成难度所用的算法标识，与当前 `DIFFICULTY_ALGORITHM` 不符时索引失效。
    pub(super) difficulty_algorithm: String,
    /// 扫描时的库根目录；与当前配置的来源不一致时索引被视为过期。
    pub(super) source_root: String,
    /// 库级统计；其 `scanned_at` 同时充当持久化快照的修订号。
    pub(super) summary: LocalLibrarySummary,
    /// 库级诊断信息。
    pub(super) diagnostics: Vec<ScanDiagnostic>,
    /// 全部条目，其余查找表与排序表均由它派生。
    pub(super) entries: Vec<IndexedEntry>,
    /// 小写归一的 MD5 → `entries` 下标；载入后重建。
    #[serde(skip)]
    pub(super) beatmap_md5_lookup: BTreeMap<String, usize>,
    /// `set_key` → 该谱面集全部条目下标；载入后重建。
    #[serde(skip)]
    pub(super) beatmap_sets: BTreeMap<String, Vec<usize>>,
    /// 每种谱面排序键预排好的条目下标序列；载入后重建。
    #[serde(skip)]
    pub(super) beatmap_orders: BTreeMap<BeatmapSort, Vec<usize>>,
    /// 每种皮肤排序键预排好的条目下标序列；载入后重建。
    #[serde(skip)]
    pub(super) skin_orders: BTreeMap<SkinSort, Vec<usize>>,
}

/// 谱面条目的预生成搜索文本，避免每次查询重复做小写归一。
#[derive(Debug, Clone)]
pub(super) struct SearchText {
    /// 已小写归一的 8 个可搜索字段（标题、艺术家、作者、难度名、来源、标签等）。
    fields: [String; 8],
    /// `beatmap_id` 与 `beatmap_set_id` 的字符串形式，支持整词精确匹配。
    ids: [Option<String>; 2],
}

impl LocalIndex {
    /// 判断下标对应的条目是否同时满足结构化查询与关键词搜索。
    /// 搜索词按空白拆分，每个词须命中任一字段（小写包含）或精确等于某个
    /// ID；条目尚未生成搜索文本时回退到即时过滤。
    pub(super) fn matches(
        &self,
        position: usize,
        query: &super::super::models::BeatmapQuery,
        search: &str,
    ) -> bool {
        let IndexedData::Beatmap { summary, detail } = &self.entries[position].data else {
            return false;
        };
        if !super::query::beatmap_matches(summary, detail, query, "") {
            return false;
        }
        if search.trim().is_empty() {
            return true;
        }
        let Some(text) = self.search_fields.get(position).and_then(Option::as_ref) else {
            return super::query::beatmap_matches(summary, detail, query, search);
        };
        search.split_whitespace().all(|term| {
            text.fields.iter().any(|field| field.contains(term))
                || text.ids.iter().flatten().any(|id| id == term)
        })
    }

    /// 按 `resource_id` 精确查找条目。
    pub(super) fn resource(&self, id: &str) -> Option<&IndexedEntry> {
        self.resource_lookup
            .get(id)
            .and_then(|position| self.entries.get(*position))
    }

    /// 由 `entries` 重建全部派生索引：搜索文本、各类查找表与预排序表。
    pub(super) fn rebuild_runtime_indexes(&mut self) {
        // 持久化数据只存稳定字段；派生查询索引在载入后重建以兼顾兼容性和查询速度。
        self.search_fields = Vec::with_capacity(self.entries.len());
        self.resource_lookup.clear();
        self.beatmap_id_lookup.clear();
        self.set_id_lookup.clear();
        for (position, entry) in self.entries.iter().enumerate() {
            let text = match &entry.data {
                IndexedData::Beatmap { summary, detail } => {
                    self.resource_lookup
                        .entry(summary.resource.resource_id.clone())
                        .or_insert(position);
                    if let Some(id) = summary.beatmap_id {
                        self.beatmap_id_lookup.entry(id).or_insert(position);
                    }
                    if let Some(id) = summary.beatmap_set_id {
                        self.set_id_lookup.insert(id);
                    }
                    Some(SearchText {
                        fields: [
                            &summary.title,
                            &summary.title_unicode,
                            &summary.artist,
                            &summary.artist_unicode,
                            &summary.creator,
                            &summary.difficulty_name,
                            &detail.source,
                            &detail.tags,
                        ]
                        .map(|text| text.to_lowercase()),
                        ids: [
                            summary.beatmap_id.map(|id| id.to_string()),
                            summary.beatmap_set_id.map(|id| id.to_string()),
                        ],
                    })
                }
                IndexedData::Skin { detail } => {
                    self.resource_lookup
                        .entry(detail.summary.resource.resource_id.clone())
                        .or_insert(position);
                    None
                }
                _ => None,
            };
            self.search_fields.push(text);
        }
        // 第二轮遍历补齐 MD5 查表与谱面集分组，再生成各排序键的预排序表。
        self.beatmap_md5_lookup.clear();
        self.beatmap_sets.clear();
        self.beatmap_orders.clear();
        self.skin_orders.clear();
        for (position, entry) in self.entries.iter().enumerate() {
            if let Some(checksum) = entry.beatmap_md5.as_ref() {
                self.beatmap_md5_lookup
                    .entry(checksum.to_ascii_lowercase())
                    .or_insert(position);
            }
            if let IndexedData::Beatmap { summary, .. } = &entry.data {
                self.beatmap_sets
                    .entry(summary.set_key.clone())
                    .or_default()
                    .push(position);
            }
        }
        let beatmap_positions = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(position, entry)| {
                matches!(entry.data, IndexedData::Beatmap { .. }).then_some(position)
            })
            .collect::<Vec<_>>();
        for sort in BeatmapSort::ALL {
            let mut positions = beatmap_positions.clone();
            positions.sort_by(|left, right| {
                match (&self.entries[*left].data, &self.entries[*right].data) {
                    (
                        IndexedData::Beatmap { summary: left, .. },
                        IndexedData::Beatmap { summary: right, .. },
                    ) => compare_beatmaps(left, right, sort),
                    _ => std::cmp::Ordering::Equal,
                }
            });
            self.beatmap_orders.insert(sort, positions);
        }
        let skin_positions = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(position, entry)| {
                matches!(entry.data, IndexedData::Skin { .. }).then_some(position)
            })
            .collect::<Vec<_>>();
        for sort in SkinSort::ALL {
            let mut positions = skin_positions.clone();
            positions.sort_by(|left, right| {
                match (&self.entries[*left].data, &self.entries[*right].data) {
                    (IndexedData::Skin { detail: left }, IndexedData::Skin { detail: right }) => {
                        compare_skins(&left.summary, &right.summary, sort)
                    }
                    _ => std::cmp::Ordering::Equal,
                }
            });
            self.skin_orders.insert(sort, positions);
        }
    }
}

/// 扫描候选的资源类型。
#[derive(Debug, Clone)]
pub(super) enum CandidateKind {
    Beatmap,
    /// 皮肤：`root` 为皮肤根目录。
    Skin {
        root: PathBuf,
    },
    /// 尚未识别的类型；因当前无构造点而标记 `allow(dead_code)`。
    #[allow(dead_code)]
    Unknown,
}

/// 扫描阶段发现的一个待处理文件候选，携带与旧索引对账所需的信息。
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    /// 与旧条目对账用的稳定键。
    pub(super) key: String,
    pub(super) physical_path: PathBuf,
    /// 相对库根的逻辑路径。
    pub(super) logical_path: String,
    /// 已知的内容哈希（如 Lazer Realm 登记值），无则为 `None`。
    pub(super) known_hash: Option<String>,
    pub(super) stamp: FileStamp,
    pub(super) kind: CandidateKind,
    /// Lazer（Realm 驱动）：谱面候选携带的权威元数据与所属集文件清单。
    pub(super) lazer_beatmap: Option<Box<super::lazer_realm::LazerRealmBeatmap>>,
    /// Lazer（Realm 驱动）：谱面集级别的归属与元数据。
    pub(super) lazer_set: Option<Arc<super::lazer_realm::LazerRealmSet>>,
    /// Lazer（Realm 驱动）：皮肤候选携带的文件清单。
    pub(super) lazer_skin: Option<Arc<super::lazer_realm::LazerRealmSkin>>,
}

/// 皮肤内单个可预览资源的定位信息。
#[derive(Debug, Clone)]
pub(super) struct SkinAssetLocation {
    /// 所属皮肤的 `resource_id`。
    pub(super) skin_resource_id: String,
    pub(super) root: PathBuf,
    pub(super) summary: LocalSkinAssetSummary,
    /// Lazer：资源内容哈希；存在时资源路径按 files/ 内容寻址解析，
    /// `root` 仅为来源记录，不参与路径拼接。
    pub(super) lazer_hash: Option<String>,
}

/// 由文件元数据生成指纹；修改时间不可得时按 0 处理。
pub(super) fn stamp(metadata: &fs::Metadata) -> FileStamp {
    FileStamp {
        bytes: metadata.len(),
        modified_ms: metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_millis()),
    }
}

/// 汇总目录内全部文件：字节数求和、修改时间取最大值，
/// 用于皮肤等目录级变更检测。
pub(super) fn directory_stamp(root: &Path) -> FileStamp {
    let mut bytes = 0u64;
    let mut modified_ms = 0u128;
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            let current = stamp(&metadata);
            bytes = bytes.saturating_add(current.bytes);
            modified_ms = modified_ms.max(current.modified_ms);
        }
    }
    FileStamp { bytes, modified_ms }
}

/// 读取文件修改时间并格式化为 RFC 3339 字符串；任何一步失败返回 `None`。
pub(super) fn modified_iso(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(modified).to_rfc3339())
}

/// 每个客户端独享一个 JSON 缓存文件（如 `stable-index.json`）。
pub(super) fn index_path(cache_dir: &Path, client: LocalClient) -> PathBuf {
    cache_dir.join(format!("{client}-index.json"))
}

/// Loads the primary cache first, then its last-known-good backup.
/// Incompatible or corrupted indexes are deliberately treated as cache misses.
/// 先读主文件再读 `.bak` 备份；反序列化失败或版本不匹配都按缓存未命中
/// 处理，交由下一次扫描整体重建。
pub(super) fn load_index(cache_dir: &Path, client: LocalClient) -> Option<LocalIndex> {
    // 主索引损坏时尝试备份；架构或难度算法版本不匹配时主动失效并在下次扫描重建。
    let target = index_path(cache_dir, client);
    let backup = cache_dir.join(format!("{client}-index.json.bak"));
    [target, backup].into_iter().find_map(|path| {
        let bytes = fs::read(path).ok()?;
        let mut index: LocalIndex = serde_json::from_slice(&bytes).ok()?;
        if index.schema != INDEX_SCHEMA || index.difficulty_algorithm != DIFFICULTY_ALGORITHM {
            return None;
        }
        index.rebuild_runtime_indexes();
        Some(index)
    })
}

/// Uses a replace-with-backup flow so an interrupted write keeps one valid cache.
/// 写入流程：先落 `.tmp`，旧主文件转存为 `.bak` 后再原子替换；替换失败时
/// 回滚备份，保证任一时刻至少存在一份可用缓存。
pub(super) fn persist_index(
    cache_dir: &Path,
    client: LocalClient,
    index: &LocalIndex,
) -> CommandResult<()> {
    let target = index_path(cache_dir, client);
    let temporary = cache_dir.join(format!("{client}-index.json.tmp"));
    let bytes = serde_json::to_vec(index)?;
    fs::write(&temporary, bytes)?;
    if target.exists() {
        let backup = cache_dir.join(format!("{client}-index.json.bak"));
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(&target, &backup)?;
        match fs::rename(&temporary, &target) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
            }
            Err(error) => {
                let _ = fs::rename(backup, target);
                return Err(error.into());
            }
        }
    } else {
        fs::rename(temporary, target)?;
    }
    Ok(())
}
