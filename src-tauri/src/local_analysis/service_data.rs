//! Internal index data and persistence for local analysis.
//!
//! Keeping the on-disk schema here makes cache compatibility explicit and keeps
//! the service focused on orchestration rather than serialization details.

use std::{
    collections::BTreeMap,
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
use super::service_query::{compare_beatmaps, compare_skins};

/// Bump this only when a serialized [`LocalIndex`] can no longer be read safely.
pub(super) const INDEX_SCHEMA: u32 = 7;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct FileStamp {
    pub(super) bytes: u64,
    pub(super) modified_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum IndexedData {
    Ignored,
    Beatmap {
        summary: LocalBeatmapSummary,
        detail: Box<LocalBeatmapDetail>,
    },
    Skin {
        detail: LocalSkinDetail,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IndexedEntry {
    pub(super) key: String,
    pub(super) physical_path: PathBuf,
    pub(super) stamp: FileStamp,
    pub(super) content_hash: Option<String>,
    #[serde(default)]
    pub(super) beatmap_md5: Option<String>,
    /// Lazer：该条目所属谱面集 / Skin 在 Realm 中登记的完整文件清单
    /// （原始文件名 + 内容哈希），用于按需解析背景、音频与皮肤资源。
    #[serde(default)]
    pub(super) lazer_files: Option<Vec<LazerRealmFile>>,
    pub(super) data: IndexedData,
    pub(super) diagnostics: Vec<ScanDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LocalIndex {
    pub(super) schema: u32,
    pub(super) difficulty_algorithm: String,
    pub(super) source_root: String,
    pub(super) summary: LocalLibrarySummary,
    pub(super) diagnostics: Vec<ScanDiagnostic>,
    pub(super) entries: Vec<IndexedEntry>,
    #[serde(skip)]
    pub(super) beatmap_md5_lookup: BTreeMap<String, usize>,
    #[serde(skip)]
    pub(super) beatmap_sets: BTreeMap<String, Vec<usize>>,
    #[serde(skip)]
    pub(super) beatmap_orders: BTreeMap<BeatmapSort, Vec<usize>>,
    #[serde(skip)]
    pub(super) skin_orders: BTreeMap<SkinSort, Vec<usize>>,
}

impl LocalIndex {
    pub(super) fn rebuild_runtime_indexes(&mut self) {
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

#[derive(Debug, Clone)]
pub(super) enum CandidateKind {
    Beatmap,
    Skin {
        root: PathBuf,
    },
    #[allow(dead_code)]
    Unknown,
}

#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub(super) key: String,
    pub(super) physical_path: PathBuf,
    pub(super) logical_path: String,
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

#[derive(Debug, Clone)]
pub(super) struct SkinAssetLocation {
    pub(super) skin_resource_id: String,
    pub(super) root: PathBuf,
    pub(super) summary: LocalSkinAssetSummary,
    /// Lazer：资源内容哈希；存在时资源路径按 files/ 内容寻址解析，
    /// `root` 仅为来源记录，不参与路径拼接。
    pub(super) lazer_hash: Option<String>,
}

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

pub(super) fn modified_iso(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(modified).to_rfc3339())
}

pub(super) fn index_path(cache_dir: &Path, client: LocalClient) -> PathBuf {
    cache_dir.join(format!("{client}-index.json"))
}

/// Loads the primary cache first, then its last-known-good backup.
/// Incompatible or corrupted indexes are deliberately treated as cache misses.
pub(super) fn load_index(cache_dir: &Path, client: LocalClient) -> Option<LocalIndex> {
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
