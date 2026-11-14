//! 缺失谱面补齐：核对收藏条目的本地存在性、反查仅有旧 MD5 的条目、
//! 从下载的 .osz 曲包提取精确 MD5 回填条目，并把曲包解压进 osu!stable 的 Songs。

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use futures_util::{StreamExt, stream};
use md5::{Digest, Md5};
use tauri::{AppHandle, State};

use super::{
    CollectionDownloadItem, CollectionInstallResult,
    service::{LocalPresenceCacheEntry, cached_local_presence, touch},
    task::{emit_collection_progress, ensure_collection_task_active},
};
use crate::{
    error::{CommandError, CommandResult},
    features::account::ensure_access_token,
    features::local_analysis::LocalClient,
    state::AppState,
};

// 曲包处理防护上限：数量、单包与累计体积、文件数、压缩比，拦截异常归档与解压炸弹。
const MAX_INSTALL_ARCHIVES: usize = 500;
const MAX_ARCHIVE_COMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 10_000;
const MAX_OSU_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const MAX_INSTALL_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;

/// 从 .osz 内 .osu 文件解析出的谱面信息与按文件字节计算的 MD5，用于回填收藏条目。
#[derive(Debug, Clone)]
pub(super) struct DownloadedBeatmap {
    pub(super) beatmapset_id: Option<i32>,
    pub(super) checksum: String,
    pub(super) ruleset: Option<String>,
    pub(super) difficulty_name: String,
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) creator: String,
}

/// Tauri 命令：计算所选收藏夹的缺失谱面清单。
/// 依次执行本地存在性核对（绑定扫描时间的缓存）与旧 MD5 的 osu! API 反查（需登录）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_collection_download_items(
    folder_ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<CollectionDownloadItem>> {
    ensure_collection_task_active(&state)?;
    let selected = folder_ids.into_iter().collect::<HashSet<_>>();
    let resolution_candidates = {
        let file = state
            .collections
            .value
            .lock()
            .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
        file.folders
            .iter()
            .filter(|folder| selected.contains(&folder.id))
            .flat_map(|folder| {
                folder.entries.iter().filter_map(|entry| {
                    entry.checksum.clone().map(|checksum| {
                        (
                            folder.id.clone(),
                            entry.id.clone(),
                            checksum.to_ascii_lowercase(),
                            entry.resolved,
                        )
                    })
                })
            })
            .collect::<Vec<_>>()
    };
    let stable_scan_at = state
        .local_analysis
        .summary(LocalClient::Stable)?
        .map(|summary| summary.scanned_at);
    let presence_cache = state
        .collections
        .value
        .lock()
        .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?
        .local_presence_cache
        .clone();
    // 缓存仅当记录的扫描时间与最近一次 stable 扫描一致时命中，否则重新核对。
    let cached_resolution = resolution_candidates
        .iter()
        .filter_map(|(_, _, checksum, _)| {
            cached_local_presence(&presence_cache, checksum, &stable_scan_at)
                .map(|present| (checksum.clone(), present))
        })
        .collect::<HashMap<_, _>>();
    let checksums_to_scan = resolution_candidates
        .iter()
        .map(|(_, _, checksum, _)| checksum.clone())
        .filter(|checksum| !cached_resolution.contains_key(checksum))
        .collect::<BTreeSet<_>>();
    let mut scanned_resolution = HashMap::new();
    if stable_scan_at.is_some() && !checksums_to_scan.is_empty() {
        emit_collection_progress(
            &app,
            "checking",
            cached_resolution.len(),
            cached_resolution.len() + checksums_to_scan.len(),
            format!(
                "已命中 {} 条缓存，正在核对 {} 个本地谱面 MD5",
                cached_resolution.len(),
                checksums_to_scan.len()
            ),
        );
        ensure_collection_task_active(&state)?;
        let found = state
            .local_analysis
            .find_beatmaps_by_md5(LocalClient::Stable, &checksums_to_scan)
            .unwrap_or_default();
        scanned_resolution.extend(
            checksums_to_scan
                .iter()
                .map(|checksum| (checksum.clone(), found.contains_key(checksum))),
        );
    } else if !cached_resolution.is_empty() {
        emit_collection_progress(
            &app,
            "checking",
            cached_resolution.len(),
            cached_resolution.len(),
            format!("已从缓存确认 {} 个谱面 MD5", cached_resolution.len()),
        );
    }
    if !cached_resolution.is_empty() || !scanned_resolution.is_empty() {
        state.collections.update(|file| {
            for (checksum, present) in &scanned_resolution {
                file.local_presence_cache.insert(
                    checksum.clone(),
                    LocalPresenceCacheEntry {
                        present: *present,
                        scan_at: stable_scan_at.clone(),
                    },
                );
            }
            for folder in &mut file.folders {
                if !selected.contains(&folder.id) {
                    continue;
                }
                for entry in &mut folder.entries {
                    let Some(checksum) = entry.checksum.as_deref() else {
                        continue;
                    };
                    if let Some(resolved) = cached_resolution
                        .get(checksum)
                        .or_else(|| scanned_resolution.get(checksum))
                    {
                        entry.resolved = *resolved;
                    }
                }
            }
            Ok(())
        })?;
    }

    let unresolved_checksums = {
        let file = state
            .collections
            .value
            .lock()
            .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
        file.folders
            .iter()
            .filter(|folder| selected.contains(&folder.id))
            .flat_map(|folder| folder.entries.iter())
            .filter(|entry| !entry.resolved && entry.beatmapset_id.is_none())
            .filter_map(|entry| entry.checksum.as_deref().map(str::to_ascii_lowercase))
            .collect::<HashSet<_>>()
    };

    if !unresolved_checksums.is_empty() {
        emit_collection_progress(
            &app,
            "checking",
            0,
            unresolved_checksums.len(),
            format!("正在查询 {} 个旧 MD5", unresolved_checksums.len()),
        );
        let access_token = ensure_access_token(&state).await.map_err(|_| {
            CommandError::new(
                "COLLECTION_LOOKUP_AUTH_REQUIRED",
                format!(
                    "有 {} 个缺失谱面只有旧 MD5。请先登录 osu! 账号，以便查询对应谱面后自动下载",
                    unresolved_checksums.len()
                ),
            )
        })?;
        let mut resolved = HashMap::<String, serde_json::Value>::new();
        // 4 路并发反查；每项处理前检查任务开关，用户取消后立即中止。
        let lookups = stream::iter(unresolved_checksums.iter().cloned())
            .map(|checksum| {
                let access_token = &access_token;
                let state = &state;
                async move {
                    ensure_collection_task_active(state)?;
                    let value = state
                        .api
                        .lookup_beatmap_by_checksum(access_token, &checksum)
                        .await
                        .ok()
                        .filter(|value| {
                            value
                                .get("id")
                                .and_then(serde_json::Value::as_i64)
                                .is_some()
                                && value
                                    .get("beatmapset_id")
                                    .and_then(serde_json::Value::as_i64)
                                    .is_some()
                        });
                    Ok::<_, CommandError>((checksum, value))
                }
            })
            .buffer_unordered(4);
        futures_util::pin_mut!(lookups);
        let mut processed = 0usize;
        while let Some(result) = lookups.next().await {
            let (checksum, value) = result?;
            if let Some(value) = value {
                resolved.insert(checksum, value);
            }
            processed += 1;
            emit_collection_progress(
                &app,
                "checking",
                processed,
                unresolved_checksums.len(),
                format!(
                    "正在解析旧收藏条目 {}/{}",
                    processed,
                    unresolved_checksums.len()
                ),
            );
        }
        if !resolved.is_empty() {
            state.collections.update(|file| {
                for folder in file
                    .folders
                    .iter_mut()
                    .filter(|folder| selected.contains(&folder.id))
                {
                    for entry in &mut folder.entries {
                        let Some(value) = entry
                            .checksum
                            .as_deref()
                            .and_then(|checksum| resolved.get(&checksum.to_ascii_lowercase()))
                        else {
                            continue;
                        };
                        entry.beatmap_id = value
                            .get("id")
                            .and_then(serde_json::Value::as_i64)
                            .and_then(|id| i32::try_from(id).ok());
                        entry.beatmapset_id = value
                            .get("beatmapset_id")
                            .and_then(serde_json::Value::as_i64)
                            .and_then(|id| i32::try_from(id).ok());
                        if let Some(version) =
                            value.get("version").and_then(serde_json::Value::as_str)
                        {
                            entry.difficulty_name = version.to_string();
                        }
                        if let Some(mode) = value.get("mode").and_then(serde_json::Value::as_str) {
                            entry.ruleset = Some(mode.to_string());
                        }
                        if let Some(set) = value.get("beatmapset") {
                            if let Some(title) = set
                                .get("title_unicode")
                                .or_else(|| set.get("title"))
                                .and_then(serde_json::Value::as_str)
                            {
                                entry.title = title.to_string();
                            }
                            if let Some(artist) = set
                                .get("artist_unicode")
                                .or_else(|| set.get("artist"))
                                .and_then(serde_json::Value::as_str)
                            {
                                entry.artist = artist.to_string();
                            }
                            if let Some(creator) =
                                set.get("creator").and_then(serde_json::Value::as_str)
                            {
                                entry.creator = creator.to_string();
                            }
                        }
                    }
                }
                Ok(())
            })?;
        }
    }

    let file = state
        .collections
        .value
        .lock()
        .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
    // 汇总仍未解析但已知 beatmapset 的条目，按谱面集去重得到待下载清单。
    let mut seen = HashSet::new();
    let items = file
        .folders
        .iter()
        .filter(|folder| selected.contains(&folder.id))
        .flat_map(|folder| folder.entries.iter())
        .filter(|entry| !entry.resolved)
        .filter_map(|entry| {
            entry
                .beatmapset_id
                .filter(|id| seen.insert(*id))
                .map(|beatmapset_id| CollectionDownloadItem {
                    beatmapset_id,
                    artist: entry.artist.clone(),
                    title: entry.title.clone(),
                })
        })
        .collect::<Vec<_>>();
    if items.is_empty() && !unresolved_checksums.is_empty() {
        return Err(CommandError::new(
            "COLLECTION_LOOKUP_FAILED",
            format!(
                "{} 个缺失谱面只有旧 MD5，osu! 官网未能找到对应谱面，当前无法自动下载",
                unresolved_checksums.len()
            ),
        ));
    }
    Ok(items)
}

/// 提取 .osu 文本 `[Metadata]` 段中指定键的值；键名忽略大小写，容忍 BOM。
fn osu_metadata_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut in_metadata = false;
    for raw_line in text.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.starts_with('[') && line.ends_with(']') {
            in_metadata = line.eq_ignore_ascii_case("[Metadata]");
            continue;
        }
        if !in_metadata {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value.trim());
        }
    }
    None
}

/// 解析 .osu 文本：`BeatmapID` 缺失或无法解析即失败；
/// MD5 按文件原始字节计算，与 collection.db 记录的难度校验和一致。
pub(super) fn parse_archive_beatmap(bytes: &[u8]) -> (Option<i32>, DownloadedBeatmap) {
    let text = String::from_utf8_lossy(bytes);
    // 2014 前的老 .osu 元数据没有任何 ID 字段（返回 None），靠标题回填兜底。
    let beatmap_id = osu_metadata_value(&text, "BeatmapID")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|id| *id > 0);
    let beatmapset_id = osu_metadata_value(&text, "BeatmapSetID")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|id| *id > 0);
    let mode = text.lines().find_map(|raw_line| {
        let line = raw_line.trim();
        line.strip_prefix("Mode:")
            .and_then(|value| value.trim().parse::<u8>().ok())
    });
    let ruleset = mode.map(|value| match value {
        1 => "taiko",
        2 => "fruits",
        3 => "mania",
        _ => "osu",
    });
    (
        beatmap_id,
        DownloadedBeatmap {
            beatmapset_id,
            checksum: format!("{:x}", Md5::digest(bytes)),
            ruleset: ruleset.map(str::to_string),
            difficulty_name: osu_metadata_value(&text, "Version")
                .unwrap_or("")
                .to_string(),
            title: osu_metadata_value(&text, "TitleUnicode")
                .filter(|value| !value.is_empty())
                .or_else(|| osu_metadata_value(&text, "Title"))
                .unwrap_or("")
                .to_string(),
            artist: osu_metadata_value(&text, "ArtistUnicode")
                .filter(|value| !value.is_empty())
                .or_else(|| osu_metadata_value(&text, "Artist"))
                .unwrap_or("")
                .to_string(),
            creator: osu_metadata_value(&text, "Creator")
                .unwrap_or("")
                .to_string(),
        },
    )
}

/// Tauri 命令：读取已下载曲包、解压进 stable 的 Songs 目录并回填精确 MD5。
/// 曲包由 OPP 直接解压（不依赖 osu! 启动时序）；解压进 Songs 在 osu! 运行时
/// 也是安全的（游戏 F5/重启时自动发现新谱面），collection.db 写回在其后统一
/// 进行且自行校验游戏状态，因此这里不再拒绝游戏运行中的调用。
#[tauri::command(async)]
pub fn install_collection_downloads(
    folder_ids: Vec<String>,
    archive_paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<CollectionInstallResult> {
    ensure_collection_task_active(&state)?;
    let source = state.local_analysis.source_status(LocalClient::Stable)?;
    if !source.valid || source.install_root.is_none() {
        return Err(CommandError::new(
            "COLLECTION_SOURCE_UNAVAILABLE",
            "未配置有效的 osu!stable 目录",
        ));
    }

    let mut installed_sets = 0usize;
    let mut extracted_sets = 0usize;
    let mut downloaded = HashMap::<i32, DownloadedBeatmap>::new();
    // 老格式（无 ID）.osu 收集于此，靠标题/难度名回填条目。
    let mut legacy_maps: Vec<DownloadedBeatmap> = Vec::new();
    let archive_total = archive_paths.len();
    if archive_total > MAX_INSTALL_ARCHIVES {
        return Err(CommandError::new(
            "COLLECTION_ARCHIVE_LIMIT",
            "单次最多处理 500 个曲包",
        ));
    }
    // 曲包解压目标：stable 的 Songs 目录（不存在则创建，缺失通常是首次安装）。
    let songs_root = PathBuf::from(source.install_root.clone().unwrap_or_default()).join("Songs");
    fs::create_dir_all(&songs_root)?;
    let mut task_expanded_size = 0u64;
    emit_collection_progress(
        &app,
        "installing",
        0,
        archive_total,
        format!("正在解压 {archive_total} 个曲包进 Songs 并计算 MD5"),
    );
    // 每个曲包（及包内文件）处理前都检查任务开关，保证可随时取消。
    for value in archive_paths {
        ensure_collection_task_active(&state)?;
        let archive_path = PathBuf::from(value);
        if !archive_path.is_file()
            || !archive_path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("osz"))
        {
            continue;
        }
        let compressed_size = fs::metadata(&archive_path)?.len();
        if compressed_size > MAX_ARCHIVE_COMPRESSED_BYTES {
            return Err(CommandError::new(
                "COLLECTION_ARCHIVE_TOO_LARGE",
                "单个曲包压缩体积不得超过 512 MB",
            ));
        }
        let archive_file = fs::File::open(&archive_path)?;
        let mut archive = zip::ZipArchive::new(BufReader::new(archive_file))
            .map_err(|error| CommandError::new("INVALID_ARCHIVE", error.to_string()))?;
        if archive.len() > MAX_ARCHIVE_FILES {
            return Err(CommandError::new(
                "INVALID_ARCHIVE",
                "曲包内文件数量超出限制",
            ));
        }
        let mut archive_maps = Vec::<(Option<i32>, DownloadedBeatmap)>::new();
        for index in 0..archive.len() {
            ensure_collection_task_active(&state)?;
            let mut file = archive
                .by_index(index)
                .map_err(|error| CommandError::new("INVALID_ARCHIVE", error.to_string()))?;
            task_expanded_size = task_expanded_size.saturating_add(file.size());
            if task_expanded_size > MAX_INSTALL_EXPANDED_BYTES {
                return Err(CommandError::new(
                    "INVALID_ARCHIVE",
                    "本次任务的累计展开体积超过 1 GB",
                ));
            }
            // 非空文件若压缩大小为零或解压体积超过压缩体积 200 倍，视为解压炸弹。
            let compressed = file.compressed_size();
            if file.size() > 0
                && (compressed == 0
                    || file.size() > compressed.saturating_mul(MAX_COMPRESSION_RATIO))
            {
                return Err(CommandError::new(
                    "INVALID_ARCHIVE",
                    "曲包包含压缩比异常的文件",
                ));
            }
            if !file.name().to_ascii_lowercase().ends_with(".osu") {
                continue;
            }
            if file.size() > MAX_OSU_FILE_BYTES {
                return Err(CommandError::new(
                    "INVALID_ARCHIVE",
                    "单个 .osu 文件不得超过 16 MB",
                ));
            }
            let mut map_bytes = Vec::with_capacity(file.size() as usize);
            (&mut file)
                .take(MAX_OSU_FILE_BYTES + 1)
                .read_to_end(&mut map_bytes)?;
            if map_bytes.len() as u64 > MAX_OSU_FILE_BYTES {
                return Err(CommandError::new(
                    "INVALID_ARCHIVE",
                    "单个 .osu 文件不得超过 16 MB",
                ));
            }
            let (beatmap_id, map) = parse_archive_beatmap(&map_bytes);
            archive_maps.push((beatmap_id, map));
        }
        // 解压进 Songs：不依赖 osu! 启动时序（交给游戏进口在启动竞态下会
        // 整批丢失参数并弹系统报错），stable 启动/刷新时自动发现新谱面；
        // 已安装同前缀目录的曲包跳过，下载文件保留原处。
        if extract_into_songs(&songs_root, &archive_path, &mut archive, &archive_maps)? {
            extracted_sets += 1;
        }
        for (beatmap_id, map) in archive_maps {
            match beatmap_id {
                Some(beatmap_id) => {
                    downloaded.insert(beatmap_id, map);
                }
                None => legacy_maps.push(map),
            }
        }
        installed_sets += 1;
        emit_collection_progress(
            &app,
            "installing",
            installed_sets,
            archive_total,
            format!("已解压 {installed_sets}/{archive_total} 个曲包"),
        );
    }

    let selected = folder_ids.into_iter().collect::<HashSet<_>>();
    let cache_scan_at = state
        .local_analysis
        .summary(LocalClient::Stable)?
        .map(|summary| summary.scanned_at);
    // 谱面集级回退索引：每个集合取最小 BeatmapID 的难度作为代表，
    // 供没有难度 ID 的占位条目（备份恢复、曲包未解析集合）回填。
    let mut downloaded_by_set = HashMap::<i32, (i32, DownloadedBeatmap)>::new();
    for (&beatmap_id, map) in &downloaded {
        let Some(set_id) = map.beatmapset_id else {
            continue;
        };
        match downloaded_by_set.get_mut(&set_id) {
            Some(current) if current.0 <= beatmap_id => {}
            current_slot => {
                let value = (beatmap_id, map.clone());
                match current_slot {
                    Some(current) => *current = value,
                    None => {
                        downloaded_by_set.insert(set_id, value);
                    }
                }
            }
        }
    }
    // 老格式（无 ID）.osu 的标题回退索引：先按（标题, 难度名）精确命中，
    // 再退回同标题首个难度，供经典老图的条目回填 MD5。
    let mut legacy_by_difficulty = HashMap::<(String, String), &DownloadedBeatmap>::new();
    let mut legacy_by_title = HashMap::<String, &DownloadedBeatmap>::new();
    for map in &legacy_maps {
        let title = map.title.to_lowercase();
        legacy_by_title.entry(title.clone()).or_insert(map);
        if !map.difficulty_name.is_empty() {
            legacy_by_difficulty
                .entry((title, map.difficulty_name.to_lowercase()))
                .or_insert(map);
        }
    }
    ensure_collection_task_active(&state)?;
    // 以曲包内 .osu 的精确 MD5 回填未解析条目，并预写存在性缓存避免重复扫描。
    state.collections.update(|file| {
        let mut resolved_entries = 0usize;
        let mut unresolved_entries = 0usize;
        for folder in &mut file.folders {
            if !selected.contains(&folder.id) {
                continue;
            }
            let mut changed = false;
            for entry in &mut folder.entries {
                if entry.resolved {
                    continue;
                }
                // 优先按难度 ID 精确命中；占位条目没有难度 ID，退回按
                // 谱面集代表难度回填；老格式曲包再按（标题, 难度名）/标题命中。
                let representative = entry
                    .beatmap_id
                    .and_then(|id| downloaded.get(&id).map(|map| (Some(id), map)))
                    .or_else(|| {
                        entry.beatmapset_id.and_then(|id| {
                            downloaded_by_set
                                .get(&id)
                                .map(|(map_id, map)| (Some(*map_id), map))
                        })
                    })
                    .or_else(|| {
                        let title = entry.title.to_lowercase();
                        if title.is_empty() {
                            return None;
                        }
                        let by_difficulty = (!entry.difficulty_name.is_empty())
                            .then(|| {
                                legacy_by_difficulty
                                    .get(&(title.clone(), entry.difficulty_name.to_lowercase()))
                                    .copied()
                            })
                            .flatten();
                        let map = by_difficulty.or_else(|| legacy_by_title.get(&title).copied())?;
                        // 难度 ID 未知时保留条目自身的 ID（可能来自线上补全）。
                        Some((entry.beatmap_id, map))
                    });
                let Some((beatmap_id, map)) = representative else {
                    unresolved_entries += 1;
                    continue;
                };
                if beatmap_id.is_some() {
                    entry.beatmap_id = beatmap_id;
                }
                entry.beatmapset_id = map.beatmapset_id.or(entry.beatmapset_id);
                entry.checksum = Some(map.checksum.clone());
                entry.ruleset = map.ruleset.clone().or(entry.ruleset.clone());
                entry.difficulty_name = map.difficulty_name.clone();
                entry.title = map.title.clone();
                entry.artist = map.artist.clone();
                entry.creator = map.creator.clone();
                entry.resolved = true;
                resolved_entries += 1;
                changed = true;
            }
            if changed {
                touch(folder);
            }
        }
        for map in downloaded.values().chain(legacy_maps.iter()) {
            file.local_presence_cache.insert(
                map.checksum.clone(),
                LocalPresenceCacheEntry {
                    present: true,
                    scan_at: cache_scan_at.clone(),
                },
            );
        }
        Ok(CollectionInstallResult {
            installed_sets,
            extracted_sets,
            resolved_entries,
            unresolved_entries,
        })
    })
}

/// 把单个 .osz 曲包解压进 Songs 目录；已安装同前缀目录时跳过（返回 false）。
/// 目录名沿用 stable 的 `{BeatmapSetID} {Artist} - {Title}` 约定；条目路径经
/// zip 的 `enclosed_name` 校验（拒绝绝对路径与 `..` 越界），未通过则跳过该条目。
fn extract_into_songs(
    songs_root: &Path,
    archive_path: &Path,
    archive: &mut zip::ZipArchive<BufReader<fs::File>>,
    maps: &[(Option<i32>, DownloadedBeatmap)],
) -> CommandResult<bool> {
    // 谱面集 ID 优先取包内解析结果，老格式回退到文件名前缀（下载命名保证以 ID 开头）。
    let set_id = maps
        .iter()
        .find_map(|(_, map)| map.beatmapset_id)
        .or_else(|| {
            archive_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.split_whitespace().next())
                .and_then(|first| first.parse::<i32>().ok())
                .filter(|id| *id > 0)
        });
    let Some(set_id) = set_id else {
        return Ok(false);
    };
    // 已安装检测：Songs 内存在同 ID 前缀目录即视为已安装（含上次解压或游戏自行导入）。
    let prefix = format!("{set_id} ");
    let already_installed = fs::read_dir(songs_root)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name == set_id.to_string() || name.starts_with(&prefix)
        });
    if already_installed {
        return Ok(false);
    }
    let representative = maps.first().map(|(_, map)| map);
    let folder_name = match representative {
        Some(map) if !map.title.is_empty() => {
            format!("{set_id} {} - {}", map.artist.trim(), map.title.trim())
        }
        _ => set_id.to_string(),
    };
    let destination = songs_root.join(sanitize_path_component(&folder_name));
    fs::create_dir_all(&destination)?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| CommandError::new("INVALID_ARCHIVE", error.to_string()))?;
        // 只接受压缩包内部相对路径，拒绝绝对路径与 `..` 越界（zip-slip）。
        let Some(relative) = file.enclosed_name() else {
            continue;
        };
        let relative = sanitize_relative_path(&relative);
        let target = destination.join(relative);
        if file.is_dir() {
            fs::create_dir_all(target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        std::io::copy(&mut file, &mut output)?;
    }
    Ok(true)
}

/// 独立的 stable 导入入口（供下载完成后的自动导入与“导入游戏”按钮复用）：
/// 逐个校验 .osz、解析包内 .osu 后解压进 Songs。返回（成功处理数, 失败描述），
/// 成功数包含“已安装而跳过”的曲包；stable 未配置/目录无效返回 Err。
pub(crate) fn import_archives_into_stable_songs(
    state: &AppState,
    archive_paths: &[String],
) -> CommandResult<(usize, Vec<String>)> {
    let source = state.local_analysis.source_status(LocalClient::Stable)?;
    let Some(root) = source.install_root.filter(|_| source.valid) else {
        return Err(CommandError::new(
            "COLLECTION_SOURCE_UNAVAILABLE",
            "未配置有效的 osu!stable 目录",
        ));
    };
    let songs_root = PathBuf::from(root).join("Songs");
    fs::create_dir_all(&songs_root)?;
    let mut handled = 0usize;
    let mut failures = Vec::new();
    for value in archive_paths {
        let path = PathBuf::from(value);
        let valid = path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("osz"));
        if !valid {
            failures.push(format!("曲包文件不存在：{value}"));
            continue;
        }
        let result = (|| -> CommandResult<bool> {
            let archive_file = fs::File::open(&path)?;
            let mut archive = zip::ZipArchive::new(BufReader::new(archive_file))
                .map_err(|error| CommandError::new("INVALID_ARCHIVE", error.to_string()))?;
            if archive.len() > MAX_ARCHIVE_FILES {
                return Err(CommandError::new(
                    "INVALID_ARCHIVE",
                    "曲包内文件数量超出限制",
                ));
            }
            let mut maps = Vec::new();
            for index in 0..archive.len() {
                let mut file = archive
                    .by_index(index)
                    .map_err(|error| CommandError::new("INVALID_ARCHIVE", error.to_string()))?;
                if !file.name().to_ascii_lowercase().ends_with(".osu")
                    || file.size() > MAX_OSU_FILE_BYTES
                {
                    continue;
                }
                let mut map_bytes = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut map_bytes)?;
                maps.push(parse_archive_beatmap(&map_bytes));
            }
            extract_into_songs(&songs_root, &path, &mut archive, &maps)
        })();
        match result {
            Ok(_) => handled += 1,
            Err(error) => failures.push(format!("无法导入 {}：{}", path.display(), error.message)),
        }
    }
    Ok((handled, failures))
}

/// 清洗单个路径组件：替换 Windows 非法字符与控制符，截断长度，防空串。
pub(crate) fn sanitize_path_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '_'
            } else {
                character
            }
        })
        .take(150)
        .collect();
    let sanitized = sanitized.trim_matches([' ', '.']).to_string();
    if sanitized.is_empty() {
        "beatmap".into()
    } else {
        sanitized
    }
}

/// 逐组件清洗相对路径，保留目录层级。
fn sanitize_relative_path(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(sanitize_path_component)
        .fold(PathBuf::new(), |accumulated, part| accumulated.join(part))
}
