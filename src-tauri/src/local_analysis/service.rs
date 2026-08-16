#[path = "service_data.rs"]
mod service_data;
#[path = "service_query.rs"]
mod service_query;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
    },
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use image::{ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder, imageops::FilterType};
use md5::{Digest, Md5};
use rayon::prelude::*;
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::error::{CommandError, CommandResult};
#[cfg(test)]
use crate::models::Ruleset;

use super::{
    lazer_realm,
    models::{
        BeatmapQuery, Completeness, LocalBeatmapDetail, LocalBeatmapSetSummary,
        LocalBeatmapSummary, LocalClient, LocalIndexLoadPhase, LocalIndexLoadStatus,
        LocalLibrarySummary, LocalScanProgress, LocalSkinAssetPayload, LocalSkinAssetSummary,
        LocalSkinDetail, LocalSkinPreview, LocalSkinSummary, LocalSourceStatus, Page,
        ScanDiagnostic, SkinAssetKind, SkinQuery,
    },
    parser::{
        DIFFICULTY_ALGORITHM, calculate_strains, calculation_version, looks_like_beatmap,
        looks_like_skin_config, parse_beatmap, parse_skin, read_prefix, sha256,
    },
    sources::{ResolvedSource, SourceResolver},
};

use service_data::{
    Candidate, CandidateKind, INDEX_SCHEMA, IndexedData, IndexedEntry, LocalIndex,
    SkinAssetLocation, directory_stamp, load_index, modified_iso, persist_index, stamp,
};
use service_query::{
    apply_direction, audio_mime, beatmap_matches, compare_beatmap_sets,
    enumerate_lazer_skin_assets, enumerate_skin_assets, find_skin_entry, insert_bounded,
    option_f64_order, skin_root, text_order,
};
#[cfg(test)]
use service_query::{compare_beatmaps, page};

#[derive(Debug)]
/// 文件发现阶段的中间结果：候选文件、源文件统计与可展示的诊断信息。
struct Discovery {
    candidates: Vec<Candidate>,
    source_file_count: usize,
    source_bytes: u64,
    diagnostics: Vec<ScanDiagnostic>,
}

/// 扫描进度发送器：节流事件频率，并保证前端看到的百分比单调递增。
struct ProgressReporter {
    emit_event: Arc<dyn Fn(LocalScanProgress) + Send + Sync>,
    client: LocalClient,
    last_emit: Mutex<Instant>,
    last_percent: Mutex<f64>,
}

impl ProgressReporter {
    fn new(emit_event: Arc<dyn Fn(LocalScanProgress) + Send + Sync>, client: LocalClient) -> Self {
        Self {
            emit_event,
            client,
            last_emit: Mutex::new(Instant::now() - Duration::from_secs(1)),
            last_percent: Mutex::new(0.0),
        }
    }

    /// 常规更新最多每 100ms 一次；阶段结束时可用 `force` 立即发送。
    fn emit(&self, phase: &str, processed: usize, total: usize, percent: f64, force: bool) {
        let Ok(mut last_emit) = self.last_emit.lock() else {
            return;
        };
        if !force && last_emit.elapsed() < Duration::from_millis(100) {
            return;
        }
        let Ok(mut last_percent) = self.last_percent.lock() else {
            return;
        };
        let percent = percent.clamp(*last_percent, 100.0);
        *last_percent = percent;
        *last_emit = Instant::now();
        (self.emit_event)(LocalScanProgress {
            client: self.client,
            phase: phase.to_string(),
            processed,
            total,
            percent,
        });
    }
}

/// 本地库扫描与查询服务。
///
/// 维护按客户端隔离的索引、皮肤资源定位表和可取消的扫描任务；CPU 密集的
/// 解析工作由专用 Rayon 线程池完成，避免阻塞 Tauri 运行时。
pub struct LocalAnalysisService {
    cache_dir: PathBuf,
    sources: SourceResolver,
    indexes: RwLock<BTreeMap<LocalClient, Arc<LocalIndex>>>,
    skin_assets: RwLock<BTreeMap<String, SkinAssetLocation>>,
    scans: Mutex<BTreeMap<LocalClient, Arc<AtomicBool>>>,
    pool: rayon::ThreadPool,
    thumbnail_cache_limit_bytes: AtomicUsize,
    load_status: RwLock<LocalIndexLoadStatus>,
}

impl LocalAnalysisService {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let cache_dir = app_data_dir.join("local-analysis");
        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(cache_dir.join("thumbnails"))?;
        let sources = SourceResolver::load(&cache_dir)?;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .thread_name(|index| format!("opp-local-analysis-{index}"))
            .build()
            .map_err(|error| {
                CommandError::new(
                    "LOCAL_ANALYSIS_INIT_ERROR",
                    format!("无法初始化本地分析线程池：{error}"),
                )
            })?;
        Ok(Self {
            cache_dir,
            sources,
            indexes: RwLock::new(BTreeMap::new()),
            skin_assets: RwLock::new(BTreeMap::new()),
            scans: Mutex::new(BTreeMap::new()),
            pool,
            thumbnail_cache_limit_bytes: AtomicUsize::new(512 * 1024 * 1024),
            load_status: RwLock::new(LocalIndexLoadStatus {
                phase: LocalIndexLoadPhase::Loading,
                error: None,
            }),
        })
    }

    pub fn load_cached_indexes(&self) {
        let result = (|| -> CommandResult<()> {
            let mut loaded = BTreeMap::new();
            for client in [LocalClient::Stable, LocalClient::Lazer] {
                if let Some(index) = load_index(&self.cache_dir, client) {
                    loaded.insert(client, Arc::new(index));
                }
            }
            let mut indexes = self
                .indexes
                .write()
                .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "本地索引状态已损坏"))?;
            for (client, index) in loaded {
                indexes.entry(client).or_insert(index);
            }
            self.trim_thumbnail_cache()?;
            Ok(())
        })();
        if let Ok(mut status) = self.load_status.write() {
            *status = match result {
                Ok(()) => LocalIndexLoadStatus {
                    phase: LocalIndexLoadPhase::Ready,
                    error: None,
                },
                Err(error) => LocalIndexLoadStatus {
                    phase: LocalIndexLoadPhase::Error,
                    error: Some(error.message),
                },
            };
        }
    }

    pub fn index_load_status(&self) -> CommandResult<LocalIndexLoadStatus> {
        self.load_status
            .read()
            .map(|status| status.clone())
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "本地索引状态已损坏"))
    }

    pub fn set_thumbnail_cache_limit_mb(&self, limit_mb: u32) -> CommandResult<()> {
        let limit = usize::try_from(limit_mb.clamp(64, 10_240))
            .unwrap_or(512)
            .saturating_mul(1024 * 1024);
        self.thumbnail_cache_limit_bytes
            .store(limit, AtomicOrdering::Relaxed);
        self.trim_thumbnail_cache()
    }

    fn trim_thumbnail_cache(&self) -> CommandResult<()> {
        let directory = self.cache_dir.join("thumbnails");
        let mut files = fs::read_dir(&directory)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then(|| {
                    (
                        entry.path(),
                        metadata.len(),
                        metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    )
                })
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|(_, _, modified)| *modified);
        let limit = self
            .thumbnail_cache_limit_bytes
            .load(AtomicOrdering::Relaxed) as u64;
        let mut total = files.iter().map(|(_, size, _)| *size).sum::<u64>();
        for (path, size, _) in files {
            if total <= limit {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }

    pub fn source_statuses(&self) -> CommandResult<Vec<LocalSourceStatus>> {
        [LocalClient::Stable, LocalClient::Lazer]
            .into_iter()
            .map(|client| self.source_status(client))
            .collect()
    }

    pub fn source_status(&self, client: LocalClient) -> CommandResult<LocalSourceStatus> {
        let mut resolved = self.sources.resolve(client)?;
        if let Some(index) = self.current_index(client)?
            && source_matches(&resolved, &index.source_root)
        {
            resolved.status.last_scanned_at = Some(index.summary.scanned_at.clone());
        }
        Ok(resolved.status)
    }

    /// 供其他模块（如工具页的 lazer 去重）读取带用户覆盖配置的已解析源路径。
    pub fn resolved_source(&self, client: LocalClient) -> CommandResult<ResolvedSource> {
        self.sources.resolve(client)
    }

    pub fn set_source(
        &self,
        client: LocalClient,
        selected_path: &Path,
    ) -> CommandResult<LocalSourceStatus> {
        let mut resolved = self.sources.set_override(client, selected_path)?;
        if let Some(index) = self.current_index(client)?
            && source_matches(&resolved, &index.source_root)
        {
            resolved.status.last_scanned_at = Some(index.summary.scanned_at.clone());
        }
        Ok(resolved.status)
    }

    pub fn reset_source(&self, client: LocalClient) -> CommandResult<LocalSourceStatus> {
        let mut resolved = self.sources.reset(client)?;
        if let Some(index) = self.current_index(client)?
            && source_matches(&resolved, &index.source_root)
        {
            resolved.status.last_scanned_at = Some(index.summary.scanned_at.clone());
        }
        Ok(resolved.status)
    }

    pub fn summary(&self, client: LocalClient) -> CommandResult<Option<LocalLibrarySummary>> {
        let source = self.sources.resolve(client)?;
        Ok(self
            .current_index(client)?
            .filter(|index| source_matches(&source, &index.source_root))
            .map(|index| index.summary.clone()))
    }

    pub fn cancel_scan(&self, client: LocalClient) -> CommandResult<()> {
        if let Some(cancel) = self
            .scans
            .lock()
            .map_err(|_| CommandError::new("LOCAL_SCAN_STATE_ERROR", "扫描状态已损坏"))?
            .get(&client)
        {
            cancel.store(true, AtomicOrdering::Relaxed);
        }
        Ok(())
    }

    pub fn scan(
        &self,
        client: LocalClient,
        force: bool,
        emit_event: Arc<dyn Fn(LocalScanProgress) + Send + Sync>,
    ) -> CommandResult<LocalLibrarySummary> {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut scans = self
                .scans
                .lock()
                .map_err(|_| CommandError::new("LOCAL_SCAN_STATE_ERROR", "扫描状态已损坏"))?;
            if scans.contains_key(&client) {
                return Err(CommandError::new(
                    "SCAN_IN_PROGRESS",
                    format!("{client} 已有扫描正在进行"),
                ));
            }
            scans.insert(client, Arc::clone(&cancel));
        }

        let result = self.run_scan(client, force, emit_event, &cancel);
        if let Ok(mut scans) = self.scans.lock() {
            scans.remove(&client);
        }
        result
    }

    fn run_scan(
        &self,
        client: LocalClient,
        force: bool,
        emit_event: Arc<dyn Fn(LocalScanProgress) + Send + Sync>,
        cancel: &AtomicBool,
    ) -> CommandResult<LocalLibrarySummary> {
        let source = self.sources.resolve(client)?;
        if !source.status.valid {
            return Err(CommandError::new(
                "INVALID_LOCAL_SOURCE",
                source.status.validation_errors.join("；"),
            ));
        }
        let source_root = source_root(&source)
            .ok_or_else(|| CommandError::new("INVALID_LOCAL_SOURCE", "本地资源数据根目录不可用"))?;
        let reporter = Arc::new(ProgressReporter::new(emit_event, client));
        reporter.emit("discovery", 0, 0, 0.0, true);

        let discovery = discover(&source, client, cancel, &reporter)?;
        check_cancelled(cancel)?;
        let total = discovery.candidates.len();
        reporter.emit("indexing", 0, total, 5.0, true);

        let previous = if force {
            None
        } else {
            self.current_index(client)?.filter(|index| {
                index.schema == INDEX_SCHEMA
                    && index.difficulty_algorithm == DIFFICULTY_ALGORITHM
                    && index.source_root == source_root
            })
        };
        let previous_entries = previous
            .as_ref()
            .map(|index| {
                index
                    .entries
                    .iter()
                    .map(|entry| (entry.key.clone(), entry.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let processed = AtomicUsize::new(0);
        let reporter_for_pool = Arc::clone(&reporter);

        let mut entries = self.pool.install(|| {
            discovery
                .candidates
                .par_iter()
                .map(|candidate| {
                    if cancel.load(AtomicOrdering::Relaxed) {
                        return None;
                    }
                    let entry = previous_entries
                        .get(&candidate.key)
                        .filter(|cached| cached.stamp == candidate.stamp)
                        .cloned()
                        .unwrap_or_else(|| process_candidate(client, candidate));
                    let done = processed.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                    let percent = if total == 0 {
                        90.0
                    } else {
                        5.0 + (done as f64 / total as f64) * 85.0
                    };
                    reporter_for_pool.emit("beatmaps", done, total, percent, false);
                    Some(entry)
                })
                .collect::<Vec<_>>()
        });
        check_cancelled(cancel)?;
        let mut entries = entries.drain(..).flatten().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.key.cmp(&right.key));
        reporter.emit("difficulty", total, total, 94.0, true);
        reporter.emit("skins", total, total, 97.0, true);

        let mut diagnostics = discovery.diagnostics;
        diagnostics.extend(
            entries
                .iter()
                .flat_map(|entry| entry.diagnostics.iter().cloned()),
        );
        let summary = build_summary(
            client,
            &source_root,
            &entries,
            discovery.source_file_count,
            discovery.source_bytes,
            diagnostics.len(),
        );
        let mut index = LocalIndex {
            schema: INDEX_SCHEMA,
            difficulty_algorithm: DIFFICULTY_ALGORITHM.to_string(),
            source_root,
            summary: summary.clone(),
            diagnostics,
            entries,
            beatmap_md5_lookup: BTreeMap::new(),
            beatmap_sets: BTreeMap::new(),
            beatmap_orders: BTreeMap::new(),
            skin_orders: BTreeMap::new(),
        };
        index.rebuild_runtime_indexes();
        check_cancelled(cancel)?;
        reporter.emit("finalizing", total, total, 99.0, true);
        persist_index(&self.cache_dir, client, &index)?;
        self.indexes
            .write()
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "本地索引状态已损坏"))?
            .insert(client, Arc::new(index));
        reporter.emit("finalizing", total, total, 100.0, true);
        Ok(summary)
    }

    pub fn query_beatmaps(&self, query: BeatmapQuery) -> CommandResult<Page<LocalBeatmapSummary>> {
        let index = self.require_current_index(query.client)?;
        let search = query.search.trim().to_lowercase();
        let limit = query.limit.clamp(1, service_query::MAX_QUERY_LIMIT);
        let ordered = index
            .beatmap_orders
            .get(&query.sort)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let positions: Box<dyn Iterator<Item = &usize>> = match query.direction {
            super::models::SortDirection::Asc => Box::new(ordered.iter()),
            super::models::SortDirection::Desc => Box::new(ordered.iter().rev()),
        };
        let mut total = 0usize;
        let mut items = Vec::with_capacity(limit);
        for position in positions {
            let Some(IndexedEntry {
                data: IndexedData::Beatmap { summary, detail },
                ..
            }) = index.entries.get(*position)
            else {
                continue;
            };
            if beatmap_matches(summary, detail, &query, &search) {
                if total >= query.offset && items.len() < limit {
                    items.push(summary.clone());
                }
                total += 1;
            }
        }
        Ok(Page {
            items,
            total,
            offset: query.offset,
            limit,
        })
    }

    pub fn query_beatmap_sets(
        &self,
        query: BeatmapQuery,
    ) -> CommandResult<Page<LocalBeatmapSetSummary>> {
        let index = self.require_current_index(query.client)?;
        let search = query.search.trim().to_lowercase();
        let limit = query.limit.clamp(1, service_query::MAX_QUERY_LIMIT);
        let capacity = query
            .offset
            .saturating_add(limit)
            .min(index.beatmap_sets.len());
        let mut sets = Vec::with_capacity(capacity);
        let mut total = 0usize;
        for (set_key, positions) in &index.beatmap_sets {
            let mut maps = positions
                .iter()
                .filter_map(|position| match &index.entries.get(*position)?.data {
                    IndexedData::Beatmap { summary, detail }
                        if beatmap_matches(summary, detail, &query, &search) =>
                    {
                        Some((summary, detail.as_ref()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let Some(set) = (|| {
                maps.sort_by(|(left, _), (right, _)| {
                    option_f64_order(left.stars, right.stars)
                        .then_with(|| text_order(&left.difficulty_name, &right.difficulty_name))
                });
                let (representative, _) = *maps.first()?;
                let creators = maps
                    .iter()
                    .map(|(summary, _)| summary.creator.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let stars = maps
                    .iter()
                    .filter_map(|(summary, _)| summary.stars)
                    .collect::<Vec<_>>();
                let min_stars = stars.iter().copied().min_by(f64::total_cmp);
                let max_stars = stars.iter().copied().max_by(f64::total_cmp);
                let bpm = maps
                    .iter()
                    .map(|(summary, _)| summary.bpm)
                    .max_by(f64::total_cmp)
                    .unwrap_or_default();
                let length_ms = maps
                    .iter()
                    .map(|(summary, _)| summary.length_ms)
                    .max_by(f64::total_cmp)
                    .unwrap_or_default();
                let object_count = maps
                    .iter()
                    .map(|(summary, _)| summary.object_count)
                    .max()
                    .unwrap_or_default();
                let modified_at = maps
                    .iter()
                    .filter_map(|(summary, _)| summary.modified_at.clone())
                    .max();
                let background_resource_id = maps.iter().find_map(|(summary, detail)| {
                    (!detail.background_file.trim().is_empty())
                        .then(|| summary.resource.resource_id.clone())
                });
                Some(LocalBeatmapSetSummary {
                    set_key: set_key.clone(),
                    completeness: Completeness::Complete,
                    grouping_inferred: representative.set_grouping_inferred,
                    beatmap_set_id: representative.beatmap_set_id,
                    title: representative.title.clone(),
                    title_unicode: representative.title_unicode.clone(),
                    artist: representative.artist.clone(),
                    artist_unicode: representative.artist_unicode.clone(),
                    creators,
                    min_stars,
                    max_stars,
                    bpm,
                    length_ms,
                    object_count,
                    modified_at,
                    background_resource_id,
                    difficulties: maps
                        .into_iter()
                        .map(|(summary, _)| summary.clone())
                        .collect(),
                })
            })() else {
                continue;
            };
            total += 1;
            insert_bounded(&mut sets, set, capacity, |left, right| {
                apply_direction(
                    compare_beatmap_sets(left, right, query.sort),
                    query.direction,
                )
            });
        }
        let items = sets.into_iter().skip(query.offset).take(limit).collect();
        Ok(Page {
            items,
            total,
            offset: query.offset,
            limit,
        })
    }

    pub fn beatmap_detail(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<LocalBeatmapDetail> {
        let index = self.require_current_index(client)?;
        let entry = index
            .entries
            .iter()
            .find(|entry| {
                matches!(
                    &entry.data,
                    IndexedData::Beatmap { summary, .. }
                        if summary.resource.resource_id == resource_id
                )
            })
            .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该谱面资源"))?;
        let mut detail = match &entry.data {
            IndexedData::Beatmap { detail, .. } => detail.as_ref().clone(),
            _ => unreachable!("entry matched beatmap"),
        };
        let bytes = fs::read(&entry.physical_path).map_err(|error| {
            CommandError::new(
                "LOCAL_RESOURCE_READ_ERROR",
                format!("无法读取谱面资源：{error}"),
            )
        })?;
        detail.strains = Some(
            calculate_strains(&bytes)
                .map_err(|message| CommandError::new("LOCAL_DIFFICULTY_ERROR", message))?,
        );
        Ok(detail)
    }

    pub fn beatmap_file_path(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<String> {
        let index = self.require_current_index(client)?;
        let entry = index
            .entries
            .iter()
            .find(|entry| {
                matches!(
                    &entry.data,
                    IndexedData::Beatmap { summary, .. }
                        if summary.resource.resource_id == resource_id
                )
            })
            .ok_or_else(|| {
                CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "Local beatmap was not found")
            })?;
        let path = entry.physical_path.canonicalize().map_err(|error| {
            CommandError::new(
                "LOCAL_RESOURCE_NOT_FOUND",
                format!("The local beatmap file is unavailable: {error}"),
            )
        })?;
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("osu"))
        {
            return Err(CommandError::new(
                "LOCAL_RESOURCE_NOT_FOUND",
                "The local beatmap file is unavailable",
            ));
        }
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn find_beatmap_by_md5(
        &self,
        client: LocalClient,
        beatmap_md5: &str,
    ) -> CommandResult<Option<LocalBeatmapSummary>> {
        let index = self.require_current_index(client)?;
        let target = beatmap_md5.trim().to_ascii_lowercase();
        Ok(index
            .beatmap_md5_lookup
            .get(&target)
            .and_then(|position| index.entries.get(*position))
            .and_then(|entry| match &entry.data {
                IndexedData::Beatmap { summary, .. } => Some(summary.clone()),
                _ => None,
            }))
    }

    /// Resolves a collection of stable MD5 hashes in one pass over the local
    /// index. This avoids the quadratic file scan that results from resolving
    /// every game-collection entry independently.
    pub fn find_beatmaps_by_md5(
        &self,
        client: LocalClient,
        checksums: &BTreeSet<String>,
    ) -> CommandResult<BTreeMap<String, LocalBeatmapSummary>> {
        if checksums.is_empty() {
            return Ok(BTreeMap::new());
        }
        let index = self.require_current_index(client)?;
        let found = checksums
            .iter()
            .filter_map(|checksum| {
                let position = index.beatmap_md5_lookup.get(checksum)?;
                let entry = index.entries.get(*position)?;
                let IndexedData::Beatmap { summary, .. } = &entry.data else {
                    return None;
                };
                Some((checksum.clone(), summary.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        Ok(found)
    }

    pub fn beatmap_background(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<Option<String>> {
        let index = self.require_current_index(client)?;
        let entry = index
            .entries
            .iter()
            .find(|entry| {
                matches!(
                    &entry.data,
                    IndexedData::Beatmap { summary, .. }
                        if summary.resource.resource_id == resource_id
                )
            })
            .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该谱面资源"))?;
        let detail = match &entry.data {
            IndexedData::Beatmap { detail, .. } => detail,
            _ => unreachable!("entry matched beatmap"),
        };
        let background_name = detail.background_file.trim();
        if background_name.is_empty() {
            return Ok(None);
        }
        let background = match client {
            LocalClient::Stable => {
                let Some(beatmap_directory) = entry.physical_path.parent() else {
                    return Ok(None);
                };
                let Ok(directory) = beatmap_directory.canonicalize() else {
                    return Ok(None);
                };
                let Ok(background) = beatmap_directory.join(background_name).canonicalize() else {
                    return Ok(None);
                };
                if !background.starts_with(&directory) {
                    return Ok(None);
                }
                background
            }
            // Lazer：背景文件在谱面集的 Realm 文件清单里按原始文件名匹配，
            // 实际内容从 files/ 内容寻址目录按哈希取回。
            LocalClient::Lazer => {
                let Some(files) = entry.lazer_files.as_ref() else {
                    return Ok(None);
                };
                let Some(file) = files
                    .iter()
                    .find(|file| file.filename.eq_ignore_ascii_case(background_name))
                else {
                    return Ok(None);
                };
                let files_root = self.lazer_files_root(client)?;
                files_root.join(lazer_realm::blob_relative_path(&file.hash))
            }
        };
        let Ok(metadata) = fs::metadata(&background) else {
            return Ok(None);
        };
        if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
            return Ok(None);
        }

        let stamp = stamp(&metadata);
        let cache_key = sha256(
            format!(
                "{}:{}:{}",
                background.to_string_lossy(),
                stamp.bytes,
                stamp.modified_ms
            )
            .as_bytes(),
        );
        let thumbnail_path = self
            .cache_dir
            .join("thumbnails")
            .join(format!("{cache_key}.jpg"));
        let thumbnail = if let Ok(bytes) = fs::read(&thumbnail_path) {
            bytes
        } else {
            let mut reader =
                match ImageReader::open(&background).and_then(ImageReader::with_guessed_format) {
                    Ok(reader) => reader,
                    Err(_) => return Ok(None),
                };
            let mut limits = Limits::default();
            limits.max_image_width = Some(8_192);
            limits.max_image_height = Some(8_192);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            let Ok(image) = reader.decode() else {
                return Ok(None);
            };
            let thumbnail = image.resize(960, 540, FilterType::Triangle);
            let mut bytes = Vec::new();
            if JpegEncoder::new_with_quality(&mut bytes, 78)
                .encode_image(&thumbnail)
                .is_err()
            {
                return Ok(None);
            }
            let temporary = thumbnail_path.with_extension("jpg.tmp");
            if fs::write(&temporary, &bytes).is_ok() {
                let _ = fs::rename(temporary, &thumbnail_path);
                let _ = self.trim_thumbnail_cache();
            }
            bytes
        };
        Ok(Some(format!(
            "data:image/jpeg;base64,{}",
            BASE64_STANDARD.encode(thumbnail)
        )))
    }

    pub fn query_skins(&self, query: SkinQuery) -> CommandResult<Page<LocalSkinSummary>> {
        let index = self.require_current_index(query.client)?;
        let search = query.search.trim().to_lowercase();
        let limit = query.limit.clamp(1, service_query::MAX_QUERY_LIMIT);
        let ordered = index
            .skin_orders
            .get(&query.sort)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let positions: Box<dyn Iterator<Item = &usize>> = match query.direction {
            super::models::SortDirection::Asc => Box::new(ordered.iter()),
            super::models::SortDirection::Desc => Box::new(ordered.iter().rev()),
        };
        let mut total = 0usize;
        let mut items = Vec::with_capacity(limit);
        for position in positions {
            let Some(IndexedEntry {
                data: IndexedData::Skin { detail },
                ..
            }) = index.entries.get(*position)
            else {
                continue;
            };
            let item = &detail.summary;
            if search.is_empty()
                || [&item.name, &item.author, &item.version]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&search))
            {
                if total >= query.offset && items.len() < limit {
                    items.push(item.clone());
                }
                total += 1;
            }
        }
        Ok(Page {
            items,
            total,
            offset: query.offset,
            limit,
        })
    }

    pub fn skin_detail(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<LocalSkinDetail> {
        let index = self.require_current_index(client)?;
        index
            .entries
            .iter()
            .find_map(|entry| match &entry.data {
                IndexedData::Skin { detail }
                    if detail.summary.resource.resource_id == resource_id =>
                {
                    Some(detail.clone())
                }
                _ => None,
            })
            .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该 Skin 资源"))
    }

    pub(crate) fn workshop_skin_root(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<PathBuf> {
        let index = self.require_current_index(client)?;
        let entry = find_skin_entry(&index, resource_id)?;
        skin_root(entry)
    }

    pub(crate) fn workshop_stable_skins_root(&self) -> CommandResult<PathBuf> {
        self.sources
            .resolve(LocalClient::Stable)?
            .skin_root
            .ok_or_else(|| CommandError::new("LOCAL_SOURCE_INVALID", "未配置 Stable Skins 目录"))
    }

    pub(crate) fn workshop_refresh_stable(&self) -> CommandResult<()> {
        self.scan(LocalClient::Stable, true, Arc::new(|_| {}))
            .map(|_| ())
    }

    pub fn skin_preview(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<LocalSkinPreview> {
        let index = self.require_current_index(client)?;
        let entry = find_skin_entry(&index, resource_id)?;
        let completeness = match &entry.data {
            IndexedData::Skin { detail } => detail.summary.completeness,
            _ => unreachable!("entry matched skin"),
        };
        let assets = match client {
            LocalClient::Lazer => {
                let files_root = self.lazer_files_root(client)?;
                let files = entry.lazer_files.clone().unwrap_or_default();
                self.index_lazer_skin_assets(&files_root, &files, resource_id)?
                    .into_iter()
                    .map(|location: SkinAssetLocation| location.summary)
                    .collect::<Vec<_>>()
            }
            LocalClient::Stable => {
                let root = skin_root(entry)?;
                self.index_skin_assets(&root, resource_id)?
            }
        };
        Ok(LocalSkinPreview {
            skin_resource_id: resource_id.to_string(),
            completeness,
            images: assets
                .iter()
                .filter(|asset| asset.kind == SkinAssetKind::Image)
                .cloned()
                .collect(),
            sounds: assets
                .into_iter()
                .filter(|asset| asset.kind == SkinAssetKind::Audio)
                .collect(),
        })
    }

    pub fn skin_asset(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
        asset_resource_id: &str,
    ) -> CommandResult<LocalSkinAssetPayload> {
        let index = self.require_current_index(client)?;
        let entry = find_skin_entry(&index, skin_resource_id)?;
        // Lazer：皮肤资源按 Realm 登记的（文件名 → 哈希）清单解析，
        // 实际文件从 files/ 内容寻址目录取回；Stable 仍按目录解析。
        let (asset, path) = match client {
            LocalClient::Lazer => {
                let files_root = self.lazer_files_root(client)?;
                let files = entry.lazer_files.clone().unwrap_or_default();
                let locations =
                    self.index_lazer_skin_assets(&files_root, &files, skin_resource_id)?;
                let location = locations
                    .into_iter()
                    .find(|location| location.summary.resource_id == asset_resource_id)
                    .ok_or_else(|| {
                        CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该 Skin 预览资源")
                    })?;
                let hash = location.lazer_hash.clone().ok_or_else(|| {
                    CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该 Skin 预览资源")
                })?;
                let path = files_root.join(lazer_realm::blob_relative_path(&hash));
                (location.summary, path)
            }
            LocalClient::Stable => {
                let root = skin_root(entry)?;
                let cached = self
                    .skin_assets
                    .read()
                    .map_err(|_| {
                        CommandError::new("LOCAL_INDEX_STATE_ERROR", "Skin 预览索引状态已损坏")
                    })?
                    .get(asset_resource_id)
                    .filter(|asset| {
                        asset.skin_resource_id == skin_resource_id && asset.root == root
                    })
                    .cloned();
                let asset = match cached {
                    Some(asset) => asset,
                    None => {
                        self.index_skin_assets(&root, skin_resource_id)?;
                        self.skin_assets
                            .read()
                            .map_err(|_| {
                                CommandError::new(
                                    "LOCAL_INDEX_STATE_ERROR",
                                    "Skin 预览索引状态已损坏",
                                )
                            })?
                            .get(asset_resource_id)
                            .filter(|asset| {
                                asset.skin_resource_id == skin_resource_id && asset.root == root
                            })
                            .cloned()
                            .ok_or_else(|| {
                                CommandError::new(
                                    "LOCAL_RESOURCE_NOT_FOUND",
                                    "未找到该 Skin 预览资源",
                                )
                            })?
                    }
                };
                let asset = asset.summary;
                let path = root.join(Path::new(&asset.logical_path));
                let canonical_root = root.canonicalize().map_err(|error| {
                    CommandError::new(
                        "LOCAL_RESOURCE_READ_ERROR",
                        format!("无法解析 Skin 目录：{error}"),
                    )
                })?;
                let canonical_path = path.canonicalize().map_err(|error| {
                    CommandError::new(
                        "LOCAL_RESOURCE_READ_ERROR",
                        format!("无法解析 Skin 资源：{error}"),
                    )
                })?;
                if !canonical_path.starts_with(&canonical_root) {
                    return Err(CommandError::new(
                        "LOCAL_RESOURCE_OUTSIDE_ROOT",
                        "Skin 资源位于允许目录之外",
                    ));
                }
                (asset, canonical_path)
            }
        };

        let (mime_type, bytes) = match asset.kind {
            SkinAssetKind::Image => {
                let metadata = fs::metadata(&path).map_err(|error| {
                    CommandError::new(
                        "LOCAL_RESOURCE_READ_ERROR",
                        format!("无法读取 Skin 图片信息：{error}"),
                    )
                })?;
                if metadata.len() > 32 * 1024 * 1024 {
                    return Err(CommandError::new(
                        "LOCAL_RESOURCE_TOO_LARGE",
                        "Skin 图片超过 32 MB 预览上限",
                    ));
                }
                let mut reader = ImageReader::open(&path)
                    .and_then(ImageReader::with_guessed_format)
                    .map_err(|error| {
                        CommandError::new(
                            "LOCAL_IMAGE_DECODE_ERROR",
                            format!("无法识别 Skin 图片：{error}"),
                        )
                    })?;
                let mut limits = Limits::default();
                limits.max_image_width = Some(8_192);
                limits.max_image_height = Some(8_192);
                limits.max_alloc = Some(128 * 1024 * 1024);
                reader.limits(limits);
                let image = reader.decode().map_err(|error| {
                    CommandError::new(
                        "LOCAL_IMAGE_DECODE_ERROR",
                        format!("无法解码 Skin 图片：{error}"),
                    )
                })?;
                let preview = if image.width() > 1_024 || image.height() > 1_024 {
                    image.resize(1_024, 1_024, FilterType::Nearest)
                } else {
                    image
                };
                let mut cursor = Cursor::new(Vec::new());
                preview
                    .write_to(&mut cursor, ImageFormat::Png)
                    .map_err(|error| {
                        CommandError::new(
                            "LOCAL_IMAGE_ENCODE_ERROR",
                            format!("无法生成 Skin 图片预览：{error}"),
                        )
                    })?;
                ("image/png".to_string(), cursor.into_inner())
            }
            SkinAssetKind::Audio => {
                let bytes = fs::read(&path).map_err(|error| {
                    CommandError::new(
                        "LOCAL_RESOURCE_READ_ERROR",
                        format!("无法读取 Skin 音效：{error}"),
                    )
                })?;
                if bytes.len() > 24 * 1024 * 1024 {
                    return Err(CommandError::new(
                        "LOCAL_RESOURCE_TOO_LARGE",
                        "Skin 音效超过 24 MB 预览上限",
                    ));
                }
                let mime_type = audio_mime(&bytes).ok_or_else(|| {
                    CommandError::new("LOCAL_AUDIO_FORMAT_ERROR", "无法识别 Skin 音效格式")
                })?;
                (mime_type.to_string(), bytes)
            }
        };
        Ok(LocalSkinAssetPayload {
            resource_id: asset.resource_id,
            kind: asset.kind,
            data_url: format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes)),
            mime_type,
        })
    }

    pub fn replace_skin_asset(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
        asset_resource_id: &str,
        replacement_path: &Path,
        save_as_new: bool,
        new_skin_name: Option<&str>,
    ) -> CommandResult<()> {
        if client != LocalClient::Stable {
            return Err(CommandError::new(
                "LOCAL_SKIN_ASSET_UNAVAILABLE",
                "Lazer 皮肤文件以内容哈希存储，就地替换会破坏 Realm 登记与哈希的一致性，暂不支持替换",
            ));
        }
        let index = self.require_current_index(client)?;
        let entry = find_skin_entry(&index, skin_resource_id)?;
        let root = skin_root(entry)?;
        self.index_skin_assets(&root, skin_resource_id)?;
        let asset = self
            .skin_assets
            .read()
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "Skin 资源索引状态已损坏"))?
            .get(asset_resource_id)
            .filter(|item| item.skin_resource_id == skin_resource_id && item.root == root)
            .map(|item| item.summary.clone())
            .ok_or_else(|| {
                CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到要替换的 Skin 资源")
            })?;
        let source = replacement_path.canonicalize().map_err(|error| {
            CommandError::new(
                "SKIN_REPLACEMENT_READ_ERROR",
                format!("无法读取替换文件：{error}"),
            )
        })?;
        if !source.is_file() {
            return Err(CommandError::new(
                "SKIN_REPLACEMENT_INVALID",
                "替换目标必须是文件",
            ));
        }
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !extension.eq_ignore_ascii_case(&asset.extension) {
            return Err(CommandError::new(
                "SKIN_REPLACEMENT_FORMAT_MISMATCH",
                format!(
                    "请选择 .{} 格式的文件以替换 {}",
                    asset.extension, asset.name
                ),
            ));
        }
        let target_root = if save_as_new {
            let name = new_skin_name.unwrap_or("").trim();
            if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
                return Err(CommandError::new(
                    "SKIN_NAME_INVALID",
                    "请输入有效的新 Skin 名称",
                ));
            }
            let target = root.parent().unwrap_or(&root).join(name);
            if target.exists() {
                return Err(CommandError::new(
                    "SKIN_NAME_EXISTS",
                    "同名 Skin 已存在，请使用其他名称",
                ));
            }
            copy_skin_directory(&root, &target)?;
            set_skin_name(&target.join("skin.ini"), name)?;
            target
        } else {
            root.clone()
        };
        let target = target_root.join(&asset.logical_path);
        replace_file_with_backup(&source, &target)?;
        self.skin_assets
            .write()
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "Skin 资源索引状态已损坏"))?
            .clear();
        self.scan(client, true, Arc::new(|_| {}))?;
        Ok(())
    }

    /// 把一个谱面集导出为 .osz。Lazer 按 Realm 文件清单从内容寻址存储
    /// 取回原始文件；Stable 直接打包谱面集目录。
    pub fn export_beatmap_set_osz(
        &self,
        client: LocalClient,
        set_key: &str,
        out_dir: &Path,
    ) -> CommandResult<String> {
        let index = self.require_current_index(client)?;
        let positions = index
            .beatmap_sets
            .get(set_key)
            .filter(|positions| !positions.is_empty())
            .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该谱面集"))?;
        let first = &index.entries[positions[0]];
        let IndexedData::Beatmap { summary, .. } = &first.data else {
            return Err(CommandError::new(
                "LOCAL_RESOURCE_NOT_FOUND",
                "未找到该谱面集",
            ));
        };
        let file_name = sanitize_export_name(&format!(
            "{} - {} ({})",
            summary.artist, summary.title, summary.creator
        ));
        let out_path = out_dir.join(format!("{file_name}.osz"));

        let sources = match client {
            LocalClient::Lazer => {
                let files_root = self.lazer_files_root(client)?;
                let files = positions
                    .iter()
                    .filter_map(|position| index.entries[*position].lazer_files.clone())
                    .next()
                    .ok_or_else(|| {
                        CommandError::new(
                            "LOCAL_EXPORT_UNAVAILABLE",
                            "该谱面集缺少 Realm 文件清单，请重新扫描后重试",
                        )
                    })?;
                files
                    .iter()
                    .map(|file| {
                        (
                            file.filename.clone(),
                            files_root.join(lazer_realm::blob_relative_path(&file.hash)),
                        )
                    })
                    .collect::<Vec<_>>()
            }
            LocalClient::Stable => {
                let root = first
                    .physical_path
                    .parent()
                    .ok_or_else(|| {
                        CommandError::new("LOCAL_EXPORT_UNAVAILABLE", "谱面集目录不可用")
                    })?
                    .to_path_buf();
                collect_directory_entries(&root)?
            }
        };
        write_export_zip(&sources, &out_path)?;
        Ok(out_path.to_string_lossy().into_owned())
    }

    /// 把一个皮肤导出为 .osk。Lazer 按 Realm 文件清单取回原始文件；
    /// Stable 直接打包皮肤目录。
    pub fn export_skin_osk(
        &self,
        client: LocalClient,
        skin_resource_id: &str,
        out_dir: &Path,
    ) -> CommandResult<String> {
        let index = self.require_current_index(client)?;
        let entry = find_skin_entry(&index, skin_resource_id)?;
        let IndexedData::Skin { detail } = &entry.data else {
            return Err(CommandError::new(
                "LOCAL_RESOURCE_NOT_FOUND",
                "未找到该 Skin 资源",
            ));
        };
        let author = detail.summary.author.trim();
        let base_name = if author.is_empty() || author.eq_ignore_ascii_case("unknown") {
            detail.summary.name.clone()
        } else {
            format!("{} ({})", detail.summary.name, author)
        };
        let out_path = out_dir.join(format!("{}.osk", sanitize_export_name(&base_name)));

        let sources = match client {
            LocalClient::Lazer => {
                let files_root = self.lazer_files_root(client)?;
                let files = entry.lazer_files.clone().ok_or_else(|| {
                    CommandError::new(
                        "LOCAL_EXPORT_UNAVAILABLE",
                        "该皮肤缺少 Realm 文件清单，请重新扫描后重试",
                    )
                })?;
                files
                    .iter()
                    .map(|file| {
                        (
                            file.filename.clone(),
                            files_root.join(lazer_realm::blob_relative_path(&file.hash)),
                        )
                    })
                    .collect::<Vec<_>>()
            }
            LocalClient::Stable => {
                let root = skin_root(entry)?;
                collect_directory_entries(&root)?
            }
        };
        write_export_zip(&sources, &out_path)?;
        Ok(out_path.to_string_lossy().into_owned())
    }

    fn index_skin_assets(
        &self,
        root: &Path,
        skin_resource_id: &str,
    ) -> CommandResult<Vec<LocalSkinAssetSummary>> {
        let assets = enumerate_skin_assets(root, skin_resource_id);
        let mut cache = self
            .skin_assets
            .write()
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "Skin 预览索引状态已损坏"))?;
        for asset in &assets {
            cache.insert(
                asset.resource_id.clone(),
                SkinAssetLocation {
                    skin_resource_id: skin_resource_id.to_string(),
                    root: root.to_path_buf(),
                    summary: asset.clone(),
                    lazer_hash: None,
                },
            );
        }
        Ok(assets)
    }

    /// 为 lazer 皮肤建立资源索引：清单来自 Realm 登记的文件列表，
    /// 每个条目携带内容哈希，读取时按 files/ 内容寻址解析。
    fn index_lazer_skin_assets(
        &self,
        files_root: &Path,
        files: &[lazer_realm::LazerRealmFile],
        skin_resource_id: &str,
    ) -> CommandResult<Vec<SkinAssetLocation>> {
        let mut cache = self
            .skin_assets
            .write()
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "Skin 预览索引状态已损坏"))?;
        let locations = enumerate_lazer_skin_assets(files, skin_resource_id)
            .into_iter()
            .map(|summary| {
                let hash = files
                    .iter()
                    .find(|file| file.filename == summary.logical_path)
                    .map(|file| file.hash.clone())
                    .unwrap_or_default();
                SkinAssetLocation {
                    skin_resource_id: skin_resource_id.to_string(),
                    root: files_root.to_path_buf(),
                    summary,
                    lazer_hash: Some(hash),
                }
            })
            .collect::<Vec<_>>();
        for location in &locations {
            cache.insert(location.summary.resource_id.clone(), location.clone());
        }
        Ok(locations)
    }

    fn current_index(&self, client: LocalClient) -> CommandResult<Option<Arc<LocalIndex>>> {
        self.indexes
            .read()
            .map(|indexes| indexes.get(&client).cloned())
            .map_err(|_| CommandError::new("LOCAL_INDEX_STATE_ERROR", "本地索引状态已损坏"))
    }

    fn require_current_index(&self, client: LocalClient) -> CommandResult<Arc<LocalIndex>> {
        let source = self.sources.resolve(client)?;
        self.current_index(client)?
            .filter(|index| source_matches(&source, &index.source_root))
            .ok_or_else(|| {
                CommandError::new("LOCAL_SCAN_REQUIRED", format!("请先扫描 {client} 本地资源"))
            })
    }

    /// lazer 的 files/ 内容寻址根目录。
    fn lazer_files_root(&self, client: LocalClient) -> CommandResult<PathBuf> {
        let source = self.sources.resolve(client)?;
        source
            .repository_root
            .clone()
            .ok_or_else(|| CommandError::new("INVALID_LOCAL_SOURCE", "lazer files 仓库不可用"))
    }
}

/// 遍历已解析的数据源，筛选出可能的谱面和皮肤配置文件。
fn copy_skin_directory(source: &Path, target: &Path) -> CommandResult<()> {
    for entry in WalkDir::new(source).follow_links(false) {
        let entry =
            entry.map_err(|error| CommandError::new("SKIN_COPY_ERROR", error.to_string()))?;
        let relative = entry.path().strip_prefix(source).map_err(|error| {
            CommandError::new(
                "SKIN_COPY_ERROR",
                format!("无法确定 Skin 文件路径：{error}"),
            )
        })?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}

fn set_skin_name(config: &Path, name: &str) -> CommandResult<()> {
    let text = fs::read_to_string(config).map_err(|error| {
        CommandError::new(
            "SKIN_CONFIG_WRITE_ERROR",
            format!("无法读取新 Skin 配置：{error}"),
        )
    })?;
    let mut in_general = false;
    let mut changed = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_general && !changed {
                lines.push(format!("Name: {name}"));
                changed = true;
            }
            in_general = trimmed.eq_ignore_ascii_case("[General]");
        }
        if in_general
            && trimmed
                .split_once(':')
                .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("Name"))
        {
            lines.push(format!("Name: {name}"));
            changed = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if in_general && !changed {
        lines.push(format!("Name: {name}"));
        changed = true;
    }
    if !changed {
        lines.insert(0, "[General]".into());
        lines.insert(1, format!("Name: {name}"));
    }
    fs::write(config, format!("{}\n", lines.join("\n"))).map_err(|error| {
        CommandError::new(
            "SKIN_CONFIG_WRITE_ERROR",
            format!("无法更新新 Skin 名称：{error}"),
        )
    })?;
    Ok(())
}

fn replace_file_with_backup(source: &Path, target: &Path) -> CommandResult<()> {
    let parent = target
        .parent()
        .ok_or_else(|| CommandError::new("SKIN_REPLACEMENT_WRITE_ERROR", "Skin 资源路径无效"))?;
    fs::create_dir_all(parent)?;
    let temporary = target.with_extension(format!(
        "{}.opp-replace",
        target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("tmp")
    ));
    let backup = target.with_extension(format!(
        "{}.opp-backup",
        target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bak")
    ));
    fs::copy(source, &temporary).map_err(|error| {
        CommandError::new(
            "SKIN_REPLACEMENT_WRITE_ERROR",
            format!("无法写入替换文件：{error}"),
        )
    })?;
    if target.exists() {
        fs::rename(target, &backup).map_err(|error| {
            CommandError::new(
                "SKIN_REPLACEMENT_WRITE_ERROR",
                format!("无法备份原资源：{error}"),
            )
        })?;
    }
    if let Err(error) = fs::rename(&temporary, target) {
        if backup.exists() {
            let _ = fs::rename(&backup, target);
        }
        return Err(CommandError::new(
            "SKIN_REPLACEMENT_WRITE_ERROR",
            format!("无法完成资源替换：{error}"),
        ));
    }
    if backup.exists() {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn discover(
    source: &ResolvedSource,
    client: LocalClient,
    cancel: &AtomicBool,
    reporter: &ProgressReporter,
) -> CommandResult<Discovery> {
    let mut discovery = Discovery {
        candidates: Vec::new(),
        source_file_count: 0,
        source_bytes: 0,
        diagnostics: Vec::new(),
    };

    match client {
        LocalClient::Stable => {
            if let Some(root) = source.beatmap_root.as_deref() {
                discover_stable_tree(root, root, false, cancel, reporter, &mut discovery)?;
            }
            if let Some(root) = source.skin_root.as_deref() {
                discover_stable_tree(root, root, true, cancel, reporter, &mut discovery)?;
            }
        }
        LocalClient::Lazer => {
            let files_root = source.repository_root.as_deref().ok_or_else(|| {
                CommandError::new("INVALID_LOCAL_SOURCE", "lazer files 仓库不可用")
            })?;
            let data_root = files_root.parent().unwrap_or(files_root);
            let realm_path = data_root.join("client.realm");
            if !realm_path.is_file() {
                return Err(CommandError::new(
                    "REALM_NOT_FOUND",
                    format!("未找到 client.realm：{}", realm_path.display()),
                ));
            }
            // 以 Realm 为权威数据源：只索引 Realm 中登记的谱面 / 皮肤，
            // 元数据、谱面集归属与文件清单都来自数据库而不是目录推断。
            let realm = lazer_realm::read_realm_data(&realm_path)
                .map_err(|message| CommandError::new("REALM_READ_FAILED", message))?;
            for set in &realm.sets {
                check_cancelled(cancel)?;
                let set = Arc::new(set.clone());
                discovery.source_file_count += set.files.len();
                discovery.source_bytes = discovery
                    .source_bytes
                    .saturating_add(set.files.iter().map(|file| file.size).sum::<u64>());
                for beatmap in &set.beatmaps {
                    let physical =
                        files_root.join(lazer_realm::blob_relative_path(&beatmap.sha256));
                    let Ok(metadata) = fs::metadata(&physical) else {
                        discovery.diagnostics.push(diagnostic(
                            "REALM_FILE_MISSING",
                            format!("Realm 登记的文件不存在：{}", beatmap.sha256),
                            None,
                        ));
                        continue;
                    };
                    discovery.candidates.push(Candidate {
                        key: format!("lazer:{}", lazer_realm::blob_relative_path(&beatmap.sha256)),
                        physical_path: physical,
                        logical_path: beatmap.sha256.clone(),
                        known_hash: Some(beatmap.sha256.clone()),
                        stamp: stamp(&metadata),
                        kind: CandidateKind::Beatmap,
                        lazer_beatmap: Some(Box::new(beatmap.clone())),
                        lazer_set: Some(Arc::clone(&set)),
                        lazer_skin: None,
                    });
                }
                reporter.emit("discovery", discovery.candidates.len(), 0, 2.0, false);
            }
            for skin in &realm.skins {
                check_cancelled(cancel)?;
                let skin = Arc::new(skin.clone());
                let Some(skin_ini) = skin.skin_ini.as_ref() else {
                    continue;
                };
                discovery.source_file_count += skin.files.len();
                discovery.source_bytes = discovery
                    .source_bytes
                    .saturating_add(skin.files.iter().map(|file| file.size).sum::<u64>());
                let physical = files_root.join(lazer_realm::blob_relative_path(&skin_ini.hash));
                let Ok(metadata) = fs::metadata(&physical) else {
                    discovery.diagnostics.push(diagnostic(
                        "REALM_FILE_MISSING",
                        format!("Realm 登记的文件不存在：{}", skin_ini.hash),
                        None,
                    ));
                    continue;
                };
                discovery.candidates.push(Candidate {
                    key: format!("lazer:{}", lazer_realm::blob_relative_path(&skin_ini.hash)),
                    physical_path: physical,
                    logical_path: skin_ini.hash.clone(),
                    known_hash: Some(skin_ini.hash.clone()),
                    stamp: stamp(&metadata),
                    kind: CandidateKind::Skin {
                        root: files_root.to_path_buf(),
                    },
                    lazer_beatmap: None,
                    lazer_set: None,
                    lazer_skin: Some(skin),
                });
                reporter.emit("discovery", discovery.candidates.len(), 0, 2.0, false);
            }
        }
    }

    Ok(discovery)
}

fn discover_stable_tree(
    tree_root: &Path,
    logical_root: &Path,
    skins: bool,
    cancel: &AtomicBool,
    reporter: &ProgressReporter,
    discovery: &mut Discovery,
) -> CommandResult<()> {
    for entry in WalkDir::new(tree_root).follow_links(false) {
        check_cancelled(cancel)?;
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                discovery
                    .diagnostics
                    .push(diagnostic("DISCOVERY_ERROR", error.to_string(), None));
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                discovery.diagnostics.push(diagnostic(
                    "METADATA_ERROR",
                    error.to_string(),
                    Some(relative_path(logical_root, entry.path())),
                ));
                continue;
            }
        };
        discovery.source_file_count += 1;
        discovery.source_bytes = discovery.source_bytes.saturating_add(metadata.len());
        let logical = relative_path(logical_root, entry.path());
        let kind = if skins
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("skin.ini")
        {
            let root = entry.path().parent().unwrap_or(tree_root).to_path_buf();
            Some(CandidateKind::Skin { root })
        } else if !skins
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("osu"))
        {
            Some(CandidateKind::Beatmap)
        } else {
            None
        };
        if let Some(kind) = kind {
            let candidate_stamp = match &kind {
                CandidateKind::Skin { root } => directory_stamp(root),
                _ => stamp(&metadata),
            };
            discovery.candidates.push(Candidate {
                lazer_beatmap: None,
                lazer_set: None,
                lazer_skin: None,
                key: format!(
                    "stable:{}:{}",
                    if skins { "skin" } else { "beatmap" },
                    entry.path().to_string_lossy().to_lowercase()
                ),
                physical_path: entry.path().to_path_buf(),
                logical_path: format!("{}/{}", if skins { "Skins" } else { "Songs" }, logical),
                known_hash: None,
                stamp: candidate_stamp,
                kind,
            });
        }
        reporter.emit("discovery", discovery.source_file_count, 0, 2.0, false);
    }
    Ok(())
}

/// 按候选类型解析单个文件；失败会编码为索引诊断而不是中止整个扫描。
fn process_candidate(client: LocalClient, candidate: &Candidate) -> IndexedEntry {
    let mut diagnostics = Vec::new();
    let mut lazer_files: Option<Vec<lazer_realm::LazerRealmFile>> = None;
    let data = match &candidate.kind {
        CandidateKind::Beatmap => match fs::read(&candidate.physical_path) {
            Ok(bytes) => match parse_beatmap(
                client,
                &bytes,
                &candidate.logical_path,
                modified_iso(&candidate.physical_path),
                candidate.known_hash.as_deref(),
            ) {
                Ok(mut parsed) => {
                    if let Some(message) = parsed.warning {
                        diagnostics.push(diagnostic(
                            "DIFFICULTY_SKIPPED",
                            message,
                            Some(candidate.logical_path.clone()),
                        ));
                    }
                    // Lazer：用 Realm 的权威元数据覆盖推断结果——
                    // 谱面集归属、unicode 元数据、在线 ID、MD5 全部来自数据库。
                    if let (Some(beatmap), Some(set)) = (
                        candidate.lazer_beatmap.as_ref(),
                        candidate.lazer_set.as_ref(),
                    ) {
                        parsed.summary.set_key = format!("realm:{}", set.id);
                        parsed.summary.set_grouping_inferred = false;
                        parsed.summary.beatmap_set_id =
                            (set.online_id > 0).then_some(set.online_id as i32);
                        parsed.summary.beatmap_id =
                            (beatmap.online_id > 0).then_some(beatmap.online_id as i32);
                        parsed.summary.artist = set.artist.clone();
                        parsed.summary.artist_unicode = set.artist_unicode.clone();
                        parsed.summary.title = set.title.clone();
                        parsed.summary.title_unicode = set.title_unicode.clone();
                        parsed.summary.creator = set.creator.clone();
                        parsed.summary.resource.logical_path = None;
                        parsed.detail.summary = parsed.summary.clone();
                        lazer_files = Some(set.files.clone());
                    }
                    IndexedData::Beatmap {
                        summary: parsed.summary,
                        detail: Box::new(parsed.detail),
                    }
                }
                Err(message) => {
                    diagnostics.push(diagnostic(
                        "BEATMAP_PARSE_ERROR",
                        message,
                        Some(candidate.logical_path.clone()),
                    ));
                    IndexedData::Ignored
                }
            },
            Err(error) => {
                diagnostics.push(diagnostic(
                    "RESOURCE_READ_ERROR",
                    error.to_string(),
                    Some(candidate.logical_path.clone()),
                ));
                IndexedData::Ignored
            }
        },
        CandidateKind::Skin { root } => match fs::read(&candidate.physical_path) {
            Ok(bytes) => {
                // Lazer：皮肤文件分散在内容寻址存储中，不能按目录枚举，
                // 清单（inventory）改由 Realm 登记的文件列表填充。
                let skin_root = (client == LocalClient::Stable).then_some(root.as_path());
                match parse_skin(
                    client,
                    &bytes,
                    &candidate.logical_path,
                    modified_iso(&candidate.physical_path),
                    candidate.known_hash.as_deref(),
                    skin_root,
                ) {
                    Ok(mut detail) => {
                        if let Some(skin) = candidate.lazer_skin.as_ref() {
                            detail.summary.completeness = Completeness::Complete;
                            detail.summary.resource.logical_path = None;
                            detail.inventory = Some(lazer_skin_inventory(&skin.files));
                            detail.summary.resource_count =
                                detail.inventory.as_ref().map(|value| value.file_count);
                            detail.summary.total_bytes =
                                detail.inventory.as_ref().map(|value| value.total_bytes);
                            lazer_files = Some(skin.files.clone());
                        }
                        IndexedData::Skin { detail }
                    }
                    Err(message) => {
                        diagnostics.push(diagnostic(
                            "SKIN_PARSE_ERROR",
                            message,
                            Some(candidate.logical_path.clone()),
                        ));
                        IndexedData::Ignored
                    }
                }
            }
            Err(error) => {
                diagnostics.push(diagnostic(
                    "RESOURCE_READ_ERROR",
                    error.to_string(),
                    Some(candidate.logical_path.clone()),
                ));
                IndexedData::Ignored
            }
        },
        CandidateKind::Unknown => process_unknown(client, candidate, &mut diagnostics),
    };
    let content_hash = match &data {
        IndexedData::Beatmap { summary, .. } => Some(summary.resource.content_hash.clone()),
        IndexedData::Skin { detail } => Some(detail.summary.resource.content_hash.clone()),
        IndexedData::Ignored => candidate.known_hash.clone(),
    };

    // MD5 优先取 Realm 登记值（收藏夹 / 游戏会话按 MD5 查谱面依赖它），
    // 缺失时回退为直接对文件计算。
    let beatmap_md5 = match (&data, candidate.lazer_beatmap.as_ref()) {
        (IndexedData::Beatmap { .. }, Some(plan)) if !plan.md5.is_empty() => {
            Some(plan.md5.to_ascii_lowercase())
        }
        _ => matches!(data, IndexedData::Beatmap { .. })
            .then(|| fs::read(&candidate.physical_path).ok())
            .flatten()
            .map(|bytes| format!("{:x}", Md5::digest(bytes))),
    };
    IndexedEntry {
        key: candidate.key.clone(),
        physical_path: candidate.physical_path.clone(),
        stamp: candidate.stamp.clone(),
        content_hash,
        beatmap_md5,
        lazer_files,
        data,
        diagnostics,
    }
}

/// 清理导出文件名中的非法字符，并限制长度。
fn sanitize_export_name(raw: &str) -> String {
    let mut name: String = raw
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            other => other,
        })
        .take(120)
        .collect();
    name = name.trim().trim_end_matches('.').to_string();
    if name.is_empty() {
        "export".to_string()
    } else {
        name
    }
}

/// 递归收集目录下所有文件，zip 条目名为相对路径（统一使用 `/`）。
fn collect_directory_entries(root: &Path) -> CommandResult<Vec<(String, PathBuf)>> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| {
                    CommandError::new(
                        "LOCAL_EXPORT_UNAVAILABLE",
                        format!("无法计算相对路径：{error}"),
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            Ok((relative, entry.path().to_path_buf()))
        })
        .collect()
}

/// 已压缩格式（音频 / 图片 / 视频）使用存储模式，避免二次压缩浪费时间。
fn zip_stored(name: &str) -> bool {
    const STORED_EXTENSIONS: [&str; 9] = [
        "mp3", "ogg", "wav", "jpg", "jpeg", "png", "webp", "gif", "mp4",
    ];
    std::path::Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| STORED_EXTENSIONS.contains(&value.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn write_export_zip(sources: &[(String, PathBuf)], out_path: &Path) -> CommandResult<()> {
    let file = fs::File::create(out_path).map_err(|error| {
        CommandError::new(
            "LOCAL_EXPORT_WRITE_ERROR",
            format!("无法创建导出文件：{error}"),
        )
    })?;
    let mut archive = ZipWriter::new(file);
    for (name, source) in sources {
        let mut options = SimpleFileOptions::default().large_file(true);
        if zip_stored(name) {
            options = options.compression_method(CompressionMethod::Stored);
        } else {
            options = options.compression_method(CompressionMethod::Deflated);
        }
        archive.start_file(name.clone(), options).map_err(|error| {
            CommandError::new(
                "LOCAL_EXPORT_WRITE_ERROR",
                format!("写入 zip 失败：{error}"),
            )
        })?;
        let mut reader = fs::File::open(source).map_err(|error| {
            CommandError::new(
                "LOCAL_EXPORT_READ_ERROR",
                format!("无法读取 {}：{error}", source.display()),
            )
        })?;
        std::io::copy(&mut reader, &mut archive).map_err(|error| {
            CommandError::new(
                "LOCAL_EXPORT_WRITE_ERROR",
                format!("写入 zip 失败：{error}"),
            )
        })?;
    }
    archive.finish().map_err(|error| {
        CommandError::new(
            "LOCAL_EXPORT_WRITE_ERROR",
            format!("完成 zip 失败：{error}"),
        )
    })?;
    Ok(())
}

/// 由 Realm 登记的皮肤文件清单构建资源统计。
fn lazer_skin_inventory(files: &[lazer_realm::LazerRealmFile]) -> super::models::SkinInventory {
    let mut by_extension = BTreeMap::new();
    for file in files {
        let extension = std::path::Path::new(&file.filename)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        *by_extension.entry(extension).or_default() += 1;
    }
    super::models::SkinInventory {
        file_count: files.len(),
        total_bytes: files.iter().map(|file| file.size).sum(),
        by_extension,
    }
}

fn process_unknown(
    client: LocalClient,
    candidate: &Candidate,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> IndexedData {
    let prefix = match read_prefix(&candidate.physical_path, 64 * 1024) {
        Ok(prefix) => prefix,
        Err(message) => {
            diagnostics.push(diagnostic(
                "RESOURCE_READ_ERROR",
                message,
                Some(candidate.logical_path.clone()),
            ));
            return IndexedData::Ignored;
        }
    };
    if !looks_like_beatmap(&prefix) && !looks_like_skin_config(&prefix) {
        return IndexedData::Ignored;
    }
    let bytes = match fs::read(&candidate.physical_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            diagnostics.push(diagnostic(
                "RESOURCE_READ_ERROR",
                error.to_string(),
                Some(candidate.logical_path.clone()),
            ));
            return IndexedData::Ignored;
        }
    };
    if looks_like_beatmap(&prefix) {
        match parse_beatmap(
            client,
            &bytes,
            &candidate.logical_path,
            modified_iso(&candidate.physical_path),
            candidate.known_hash.as_deref(),
        ) {
            Ok(mut parsed) => {
                parsed.summary.resource.logical_path = None;
                parsed.detail.summary.resource.logical_path = None;
                if let Some(message) = parsed.warning {
                    diagnostics.push(diagnostic("DIFFICULTY_SKIPPED", message, None));
                }
                IndexedData::Beatmap {
                    summary: parsed.summary,
                    detail: Box::new(parsed.detail),
                }
            }
            Err(message) => {
                diagnostics.push(diagnostic("BEATMAP_PARSE_ERROR", message, None));
                IndexedData::Ignored
            }
        }
    } else {
        match parse_skin(
            client,
            &bytes,
            &candidate.logical_path,
            modified_iso(&candidate.physical_path),
            candidate.known_hash.as_deref(),
            None,
        ) {
            Ok(mut detail) => {
                detail.summary.resource.logical_path = None;
                IndexedData::Skin { detail }
            }
            Err(message) => {
                diagnostics.push(diagnostic("SKIN_PARSE_ERROR", message, None));
                IndexedData::Ignored
            }
        }
    }
}

/// 从完整索引聚合库级统计；推断分组会单独记录，避免与在线谱面集混淆。
fn build_summary(
    client: LocalClient,
    source_root: &str,
    entries: &[IndexedEntry],
    source_file_count: usize,
    source_bytes: u64,
    diagnostic_count: usize,
) -> LocalLibrarySummary {
    let beatmaps = entries.iter().filter_map(|entry| match &entry.data {
        IndexedData::Beatmap { summary, .. } => Some(summary),
        _ => None,
    });
    let mut set_keys = BTreeSet::new();
    let mut beatmap_count = 0usize;
    let mut inferred = false;
    let mut mode_counts = BTreeMap::new();
    for beatmap in beatmaps {
        beatmap_count += 1;
        set_keys.insert(beatmap.set_key.clone());
        inferred |= beatmap.set_grouping_inferred;
        *mode_counts
            .entry(beatmap.ruleset.to_string())
            .or_insert(0usize) += 1;
    }
    let skin_count = entries
        .iter()
        .filter(|entry| matches!(entry.data, IndexedData::Skin { .. }))
        .count();

    LocalLibrarySummary {
        client,
        // Lazer 现在以 Realm 为权威数据源，索引完整性与 Stable 一致。
        completeness: Completeness::Complete,
        source_root: source_root.to_string(),
        scanned_at: Utc::now().to_rfc3339(),
        beatmap_count,
        beatmap_set_count: set_keys.len(),
        beatmap_set_count_inferred: inferred,
        skin_count,
        source_file_count,
        source_bytes,
        diagnostic_count,
        mode_counts,
        calculation: calculation_version(),
    }
}

fn source_root(source: &ResolvedSource) -> Option<String> {
    source.status.data_root.clone()
}

fn source_matches(source: &ResolvedSource, indexed_root: &str) -> bool {
    source.status.valid && source_root(source).as_deref() == Some(indexed_root)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
    logical_path: Option<String>,
) -> ScanDiagnostic {
    ScanDiagnostic {
        code: code.into(),
        message: message.into(),
        logical_path,
        resource_id: None,
    }
}

fn check_cancelled(cancel: &AtomicBool) -> CommandResult<()> {
    if cancel.load(AtomicOrdering::Relaxed) {
        Err(CommandError::new("SCAN_CANCELLED", "本地资源扫描已取消"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::super::models::BeatmapSort;
    use super::*;

    const OSU_FIXTURE: &str = r#"osu file format v14

[General]
AudioFilename: audio.mp3
Mode: 0

[Metadata]
Title:Fixture
TitleUnicode:测试谱面
Artist:Artist
ArtistUnicode:艺术家
Creator:Mapper
Version:Normal
BeatmapID:-1
BeatmapSetID:-1

[Difficulty]
HPDrainRate:5
CircleSize:4
OverallDifficulty:6
ApproachRate:7
SliderMultiplier:1.4
SliderTickRate:1

[Events]
0,0,"bg.jpg",0,0
2,1200,1400

[TimingPoints]
0,500,4,2,0,100,1,0

[HitObjects]
256,192,1000,1,0,0:0:0:0:
256,192,1500,1,0,0:0:0:0:
256,192,2000,8,0,2500
"#;

    fn beatmap(resource_id: &str, title: &str, stars: f64) -> LocalBeatmapSummary {
        LocalBeatmapSummary {
            resource: super::super::models::LocalResourceRef {
                resource_id: resource_id.into(),
                client: LocalClient::Stable,
                content_hash: "hash".into(),
                logical_path: None,
            },
            set_key: "set".into(),
            set_grouping_inferred: false,
            beatmap_id: None,
            beatmap_set_id: None,
            title: title.into(),
            title_unicode: title.into(),
            artist: "artist".into(),
            artist_unicode: "artist".into(),
            creator: "creator".into(),
            difficulty_name: "diff".into(),
            ruleset: Ruleset::Osu,
            format_version: 14,
            stars: Some(stars),
            max_pp: Some(100.0),
            max_combo: Some(1),
            bpm: 180.0,
            length_ms: 1_000.0,
            object_count: 1,
            cs: 4.0,
            ar: 8.0,
            od: 7.0,
            hp: 5.0,
            average_nps: 1.0,
            peak_nps: 1.0,
            modified_at: None,
            analysis_status: "ready".into(),
        }
    }

    #[test]
    fn pagination_caps_page_size() {
        let result = page((0..600).collect::<Vec<_>>(), 10, 1_000);
        assert_eq!(result.total, 600);
        assert_eq!(result.limit, 500);
        assert_eq!(result.items.len(), 500);
    }

    #[test]
    fn difficulty_sort_places_missing_values_last() {
        assert_eq!(option_f64_order(Some(2.0), None), Ordering::Less);
        assert_eq!(
            compare_beatmaps(
                &beatmap("a", "z", 2.0),
                &beatmap("b", "a", 3.0),
                BeatmapSort::Stars
            ),
            Ordering::Less
        );
    }

    #[test]
    fn corrupted_cache_is_ignored() {
        let directory = tempfile::tempdir().expect("cache");
        fs::write(
            service_data::index_path(directory.path(), LocalClient::Stable),
            "not json",
        )
        .expect("write");
        assert!(load_index(directory.path(), LocalClient::Stable).is_none());
    }

    fn empty_index(algorithm: &str) -> LocalIndex {
        LocalIndex {
            schema: INDEX_SCHEMA,
            difficulty_algorithm: algorithm.into(),
            source_root: "fixture".into(),
            summary: LocalLibrarySummary {
                client: LocalClient::Stable,
                completeness: Completeness::Complete,
                source_root: "fixture".into(),
                scanned_at: Utc::now().to_rfc3339(),
                beatmap_count: 0,
                beatmap_set_count: 0,
                beatmap_set_count_inferred: false,
                skin_count: 0,
                source_file_count: 0,
                source_bytes: 0,
                diagnostic_count: 0,
                mode_counts: BTreeMap::new(),
                calculation: calculation_version(),
            },
            diagnostics: Vec::new(),
            entries: Vec::new(),
            beatmap_md5_lookup: BTreeMap::new(),
            beatmap_sets: BTreeMap::new(),
            beatmap_orders: BTreeMap::new(),
            skin_orders: BTreeMap::new(),
        }
    }

    #[test]
    fn cache_recovers_from_backup_and_invalidates_algorithm_changes() {
        let directory = tempfile::tempdir().expect("cache");
        fs::write(
            service_data::index_path(directory.path(), LocalClient::Stable),
            "corrupted",
        )
        .expect("primary");
        fs::write(
            directory.path().join("stable-index.json.bak"),
            serde_json::to_vec(&empty_index(DIFFICULTY_ALGORITHM)).expect("json"),
        )
        .expect("backup");
        assert!(load_index(directory.path(), LocalClient::Stable).is_some());

        fs::write(
            directory.path().join("stable-index.json.bak"),
            serde_json::to_vec(&empty_index("old algorithm")).expect("json"),
        )
        .expect("old backup");
        assert!(load_index(directory.path(), LocalClient::Stable).is_none());
    }

    fn fixture_service() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        LocalAnalysisService,
        PathBuf,
    ) {
        let app_data = tempfile::tempdir().expect("app data");
        let stable = tempfile::tempdir().expect("stable");
        fs::write(stable.path().join("osu!.exe"), []).expect("exe");
        fs::write(
            stable.path().join("osu!.fixture.cfg"),
            "BeatmapDirectory = Songs\nLastVersion = 20260725",
        )
        .expect("config");
        let set = stable.path().join("Songs").join("1 Fixture");
        fs::create_dir_all(&set).expect("songs");
        let beatmap = set.join("fixture.osu");
        fs::write(&beatmap, OSU_FIXTURE).expect("beatmap");
        image::RgbImage::from_pixel(8, 8, image::Rgb([24, 48, 72]))
            .save(set.join("bg.jpg"))
            .expect("background");
        let skin = stable.path().join("Skins").join("Fixture");
        fs::create_dir_all(&skin).expect("skin");
        fs::write(
            skin.join("skin.ini"),
            "[General]\nName: Fixture\nAuthor: OPP\nVersion: 2.7\n[Colours]\nCombo1: 255,120,180\n",
        )
        .expect("skin config");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 128]))
            .save(skin.join("cursor.png"))
            .expect("skin image");
        fs::write(
            skin.join("normal-hitnormal.wav"),
            b"RIFF\x04\x00\x00\x00WAVE",
        )
        .expect("skin sound");

        let service = LocalAnalysisService::new(app_data.path()).expect("service");
        service
            .set_source(LocalClient::Stable, stable.path())
            .expect("source");
        (app_data, stable, service, beatmap)
    }

    #[test]
    fn stable_fixture_scans_queries_and_inventories_resources() {
        let (_app_data, _stable, service, _beatmap) = fixture_service();
        let cancel = AtomicBool::new(false);
        let summary = service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect("scan");
        assert_eq!(summary.beatmap_count, 1);
        assert_eq!(summary.beatmap_set_count, 1);
        assert_eq!(summary.skin_count, 1);

        let maps = service
            .query_beatmaps(BeatmapQuery::default())
            .expect("beatmaps");
        assert_eq!(maps.total, 1);
        assert_eq!(maps.items[0].title_unicode, "测试谱面");
        assert_eq!(maps.items[0].ruleset, Ruleset::Osu);
        let background = service
            .beatmap_background(LocalClient::Stable, &maps.items[0].resource.resource_id)
            .expect("background")
            .expect("background data");
        assert!(background.starts_with("data:image/jpeg;base64,"));

        let skins = service.query_skins(SkinQuery::default()).expect("skins");
        assert_eq!(skins.total, 1);
        let detail = service
            .skin_detail(LocalClient::Stable, &skins.items[0].resource.resource_id)
            .expect("skin detail");
        assert_eq!(detail.summary.accent_colors, vec![vec![255, 120, 180]]);
        assert_eq!(detail.inventory.expect("inventory").file_count, 3);
        let preview = service
            .skin_preview(LocalClient::Stable, &skins.items[0].resource.resource_id)
            .expect("skin preview");
        assert_eq!(preview.images.len(), 1);
        assert_eq!(preview.sounds.len(), 1);
        let image = service
            .skin_asset(
                LocalClient::Stable,
                &skins.items[0].resource.resource_id,
                &preview.images[0].resource_id,
            )
            .expect("skin image payload");
        assert!(image.data_url.starts_with("data:image/png;base64,"));
        let audio = service
            .skin_asset(
                LocalClient::Stable,
                &skins.items[0].resource.resource_id,
                &preview.sounds[0].resource_id,
            )
            .expect("skin audio payload");
        assert!(audio.data_url.starts_with("data:audio/wav;base64,"));
    }

    #[test]
    fn groups_difficulties_and_applies_structural_filters() {
        let (_app_data, _stable, service, beatmap) = fixture_service();
        fs::write(
            beatmap.with_file_name("hard.osu"),
            OSU_FIXTURE
                .replace("Version:Normal", "Version:Hard")
                .replace("ApproachRate:7", "ApproachRate:9"),
        )
        .expect("hard");
        let cancel = AtomicBool::new(false);
        service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect("scan");

        let sets = service
            .query_beatmap_sets(BeatmapQuery::default())
            .expect("sets");
        assert_eq!(sets.total, 1);
        assert_eq!(sets.items[0].difficulties.len(), 2);

        let filtered = service
            .query_beatmap_sets(BeatmapQuery {
                min_ar: Some(8.0),
                ..BeatmapQuery::default()
            })
            .expect("filtered sets");
        assert_eq!(filtered.total, 1);
        assert_eq!(filtered.items[0].difficulties.len(), 1);
        assert_eq!(filtered.items[0].difficulties[0].difficulty_name, "Hard");
    }

    #[test]
    fn incremental_scan_tracks_additions_and_deletions() {
        let (_app_data, _stable, service, beatmap) = fixture_service();
        let cancel = AtomicBool::new(false);
        service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect("first scan");

        let added = beatmap.with_file_name("added.osu");
        fs::write(
            &added,
            OSU_FIXTURE.replace("Version:Normal", "Version:Hard"),
        )
        .expect("add beatmap");
        let summary = service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect("incremental add");
        assert_eq!(summary.beatmap_count, 2);

        fs::remove_file(beatmap).expect("remove beatmap");
        let summary = service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect("incremental delete");
        assert_eq!(summary.beatmap_count, 1);
    }

    #[test]
    fn md5_lookup_is_persisted_and_reloaded_without_reading_the_library() {
        let (app_data, _stable, service, beatmap) = fixture_service();
        let checksum = format!(
            "{:x}",
            Md5::digest(fs::read(&beatmap).expect("beatmap bytes"))
        );
        let cancel = AtomicBool::new(false);
        service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect("scan");
        assert!(
            service
                .find_beatmap_by_md5(LocalClient::Stable, &checksum)
                .expect("lookup")
                .is_some()
        );

        let reloaded = LocalAnalysisService::new(app_data.path()).expect("reloaded service");
        reloaded.load_cached_indexes();
        assert!(
            reloaded
                .find_beatmap_by_md5(LocalClient::Stable, &checksum)
                .expect("reloaded lookup")
                .is_some()
        );
    }

    #[test]
    fn cancelled_scan_does_not_create_an_index() {
        let (_app_data, _stable, service, _beatmap) = fixture_service();
        let cancel = AtomicBool::new(true);
        let error = service
            .run_scan(LocalClient::Stable, false, Arc::new(|_| {}), &cancel)
            .expect_err("cancelled");
        assert_eq!(error.code, "SCAN_CANCELLED");
        assert!(
            service
                .summary(LocalClient::Stable)
                .expect("summary")
                .is_none()
        );
    }

    #[test]
    #[ignore = "requires explicit OPP_TEST_STABLE_ROOT / OPP_TEST_LAZER_ROOT"]
    fn scans_opt_in_machine_acceptance_sources() {
        let app_data = tempfile::tempdir().expect("app data");
        let service = LocalAnalysisService::new(app_data.path()).expect("service");
        let cancel = AtomicBool::new(false);
        let mut scanned = 0usize;

        for (variable, client) in [
            ("OPP_TEST_STABLE_ROOT", LocalClient::Stable),
            ("OPP_TEST_LAZER_ROOT", LocalClient::Lazer),
        ] {
            let Ok(path) = std::env::var(variable) else {
                continue;
            };
            service
                .set_source(client, Path::new(&path))
                .expect("acceptance source");
            let summary = service
                .run_scan(client, true, Arc::new(|_| {}), &cancel)
                .expect("acceptance scan");
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).expect("summary json")
            );
            assert!(summary.beatmap_count > 0);
            if client == LocalClient::Stable {
                let skins = service
                    .query_skins(SkinQuery {
                        limit: 500,
                        ..SkinQuery::default()
                    })
                    .expect("acceptance skins");
                let mut image_count = 0usize;
                let mut sound_count = 0usize;
                let mut decoded_image = false;
                for skin in &skins.items {
                    let preview = service
                        .skin_preview(client, &skin.resource.resource_id)
                        .expect("acceptance skin preview");
                    image_count += preview.images.len();
                    sound_count += preview.sounds.len();
                    if !decoded_image {
                        decoded_image = preview.images.iter().any(|asset| {
                            service
                                .skin_asset(client, &skin.resource.resource_id, &asset.resource_id)
                                .is_ok()
                        });
                    }
                }
                println!("stable skin preview assets: {image_count} images, {sound_count} sounds");
                assert!(image_count > 0);
                assert!(sound_count > 0);
                assert!(decoded_image);
            }
            if client == LocalClient::Lazer {
                // Realm 驱动后：谱面集分组、皮肤预览、背景都应可用。
                assert!(!summary.beatmap_set_count_inferred);
                let sets = service
                    .query_beatmap_sets(BeatmapQuery {
                        client,
                        limit: 5,
                        ..BeatmapQuery::default()
                    })
                    .expect("acceptance lazer sets");
                assert!(!sets.items.is_empty());
                assert_eq!(sets.items[0].completeness, Completeness::Complete);
                let mut decoded_background = false;
                for set in &sets.items {
                    for difficulty in &set.difficulties {
                        if service
                            .beatmap_background(client, &difficulty.resource.resource_id)
                            .expect("acceptance background call")
                            .is_some()
                        {
                            decoded_background = true;
                        }
                    }
                }
                println!("lazer background decoded: {decoded_background}");
                let skins = service
                    .query_skins(SkinQuery {
                        client,
                        limit: 500,
                        ..SkinQuery::default()
                    })
                    .expect("acceptance lazer skins");
                let mut image_count = 0usize;
                let mut decoded_image = false;
                for skin in &skins.items {
                    let preview = service
                        .skin_preview(client, &skin.resource.resource_id)
                        .expect("acceptance lazer skin preview");
                    image_count += preview.images.len();
                    if !decoded_image {
                        decoded_image = preview.images.iter().any(|asset| {
                            service
                                .skin_asset(client, &skin.resource.resource_id, &asset.resource_id)
                                .is_ok()
                        });
                    }
                }
                println!(
                    "lazer skin preview assets: {image_count} images, decoded: {decoded_image}"
                );
                assert!(image_count > 0);
                assert!(decoded_image);

                // 导出 .osz / .osk：写出后重新打开校验条目数与内容。
                let export_dir = tempfile::tempdir().expect("export dir");
                let osz_path = service
                    .export_beatmap_set_osz(client, &sets.items[0].set_key, export_dir.path())
                    .expect("acceptance osz export");
                let osz = std::fs::File::open(&osz_path).expect("osz exists");
                let osz_entries = zip::ZipArchive::new(osz).expect("osz readable").len();
                println!("osz exported: {osz_path} ({osz_entries} entries)");
                assert!(osz_entries > sets.items[0].difficulties.len());

                let osk_path = service
                    .export_skin_osk(
                        client,
                        &skins.items[0].resource.resource_id,
                        export_dir.path(),
                    )
                    .expect("acceptance osk export");
                let osk = std::fs::File::open(&osk_path).expect("osk exists");
                let osk_entries = zip::ZipArchive::new(osk).expect("osk readable").len();
                println!("osk exported: {osk_path} ({osk_entries} entries)");
                assert!(osk_entries > 1);
            }
            scanned += 1;
        }

        assert!(scanned > 0, "set at least one OPP_TEST_*_ROOT variable");
    }
}
