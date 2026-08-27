use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{BufReader, Read},
    path::PathBuf,
    time::Duration,
};

use futures_util::{StreamExt, stream};
use md5::{Digest, Md5};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::{
    CollectionDownloadItem, CollectionInstallResult, CollectionOpenResult,
    service::{LocalPresenceCacheEntry, cached_local_presence, touch},
    task::{emit_collection_progress, ensure_collection_task_active},
};
use crate::{
    error::{CommandError, CommandResult},
    features::account::ensure_access_token,
    features::{game_session::get_game_status, local_analysis::LocalClient},
    state::AppState,
};

const MAX_INSTALL_ARCHIVES: usize = 500;
const MAX_ARCHIVE_COMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 10_000;
const MAX_OSU_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const MAX_INSTALL_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;

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

pub(super) fn parse_downloaded_beatmap(bytes: &[u8]) -> Option<(i32, DownloadedBeatmap)> {
    let text = String::from_utf8_lossy(bytes);
    let beatmap_id = osu_metadata_value(&text, "BeatmapID")?.parse().ok()?;
    let beatmapset_id =
        osu_metadata_value(&text, "BeatmapSetID").and_then(|value| value.parse().ok());
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
    Some((
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
    ))
}

/// Reads downloaded archives and hydrates compact share-code entries with the
/// exact MD5 required by collection.db. The archives are intentionally opened
/// by osu! only after collection.db has been written.
#[tauri::command(async)]
pub fn install_collection_downloads(
    folder_ids: Vec<String>,
    archive_paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<CollectionInstallResult> {
    ensure_collection_task_active(&state)?;
    if get_game_status(state.clone())?
        .clients
        .iter()
        .any(|client| client.client == LocalClient::Stable && client.running)
    {
        return Err(CommandError::new(
            "GAME_RUNNING",
            "请关闭 osu!stable 后再准备缺失谱面并写回收藏夹",
        ));
    }
    let source = state.local_analysis.source_status(LocalClient::Stable)?;
    if !source.valid || source.install_root.is_none() {
        return Err(CommandError::new(
            "COLLECTION_SOURCE_UNAVAILABLE",
            "未配置有效的 osu!stable 目录",
        ));
    }

    let mut installed_sets = 0usize;
    let mut downloaded = HashMap::<i32, DownloadedBeatmap>::new();
    let archive_total = archive_paths.len();
    if archive_total > MAX_INSTALL_ARCHIVES {
        return Err(CommandError::new(
            "COLLECTION_ARCHIVE_LIMIT",
            "单次最多处理 500 个曲包",
        ));
    }
    let mut task_expanded_size = 0u64;
    emit_collection_progress(
        &app,
        "installing",
        0,
        archive_total,
        format!("正在读取 {archive_total} 个曲包并计算 MD5"),
    );
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
        let mut archive_maps = Vec::<(i32, DownloadedBeatmap)>::new();
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
            if let Some(map) = parse_downloaded_beatmap(&map_bytes) {
                archive_maps.push(map);
            }
        }
        downloaded.extend(archive_maps);
        installed_sets += 1;
        emit_collection_progress(
            &app,
            "installing",
            installed_sets,
            archive_total,
            format!("已读取 {installed_sets}/{archive_total} 个曲包"),
        );
    }

    let selected = folder_ids.into_iter().collect::<HashSet<_>>();
    let cache_scan_at = state
        .local_analysis
        .summary(LocalClient::Stable)?
        .map(|summary| summary.scanned_at);
    ensure_collection_task_active(&state)?;
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
                let Some(map) = entry.beatmap_id.and_then(|id| downloaded.get(&id)) else {
                    unresolved_entries += 1;
                    continue;
                };
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
        for map in downloaded.values() {
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
            resolved_entries,
            unresolved_entries,
        })
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn open_collection_downloads(
    archive_paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<CollectionOpenResult> {
    ensure_collection_task_active(&state)?;
    let total = archive_paths.len();
    let mut opened = 0usize;
    let mut failures = Vec::new();
    emit_collection_progress(&app, "opening", 0, total, "正在调用 osu! 导入曲包");
    for value in archive_paths {
        ensure_collection_task_active(&state)?;
        let path = PathBuf::from(&value);
        let valid = path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("osz"));
        if !valid {
            failures.push(format!("曲包文件不存在：{value}"));
        } else if let Err(error) = app.opener().open_path(path.to_string_lossy(), None::<&str>) {
            failures.push(format!("无法打开 {}：{error}", path.display()));
        } else {
            opened += 1;
        }
        emit_collection_progress(
            &app,
            "opening",
            opened + failures.len(),
            total,
            format!(
                "已交给游戏处理 {}/{} 个曲包",
                opened + failures.len(),
                total
            ),
        );
        tokio::time::sleep(Duration::from_millis(180)).await;
    }
    Ok(CollectionOpenResult {
        opened,
        failed: failures.len(),
        failures,
    })
}
