use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use md5::Md5;
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use super::{
    CollectionCandidate, CollectionEntry, CollectionFolder, CollectionSnapshot, CollectionSource,
    CollectionSourceStatus, CollectionSyncStatus, CollectionWriteResult,
    service::CollectionService, task::ensure_collection_task_active,
};
use crate::{
    error::{CommandError, CommandResult},
    features::{
        game_session::get_game_status,
        local_analysis::{LocalBeatmapSummary, LocalClient},
    },
    state::AppState,
};

#[derive(Debug, Clone)]
pub(super) struct StableCollection {
    pub(super) name: String,
    pub(super) checksums: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct StableDb {
    pub(super) version: i32,
    pub(super) folders: Vec<StableCollection>,
}

fn read_i32(bytes: &[u8], offset: &mut usize) -> CommandResult<i32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹文件长度无效"))?;
    let slice = bytes
        .get(*offset..end)
        .ok_or_else(|| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹文件不完整"))?;
    *offset = end;
    Ok(i32::from_le_bytes(slice.try_into().expect("i32 slice")))
}

pub(super) fn read_uleb(bytes: &[u8], offset: &mut usize) -> CommandResult<usize> {
    // Stable 数据库使用无符号 LEB128；每次读取都检查边界和移位长度。
    let mut value = 0usize;
    let mut shift = 0usize;
    loop {
        let byte = *bytes
            .get(*offset)
            .ok_or_else(|| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹字符串不完整"))?;
        *offset += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift > 56 {
            return Err(CommandError::new(
                "COLLECTION_PARSE_FAILED",
                "收藏夹字符串长度无效",
            ));
        }
    }
}

fn read_osu_string(bytes: &[u8], offset: &mut usize) -> CommandResult<String> {
    let marker = *bytes
        .get(*offset)
        .ok_or_else(|| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹字符串不完整"))?;
    *offset += 1;
    if marker == 0 {
        return Ok(String::new());
    }
    if marker != 0x0b {
        return Err(CommandError::new(
            "COLLECTION_PARSE_FAILED",
            "收藏夹字符串标记无效",
        ));
    }
    let length = read_uleb(bytes, offset)?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹字符串长度无效"))?;
    let slice = bytes
        .get(*offset..end)
        .ok_or_else(|| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹字符串不完整"))?;
    *offset = end;
    String::from_utf8(slice.to_vec())
        .map_err(|_| CommandError::new("COLLECTION_PARSE_FAILED", "收藏夹字符串不是 UTF-8"))
}

pub(super) fn push_uleb(value: usize, output: &mut Vec<u8>) {
    let mut value = value;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_osu_string(value: &str, output: &mut Vec<u8>) {
    if value.is_empty() {
        output.push(0);
        return;
    }
    output.push(0x0b);
    push_uleb(value.len(), output);
    output.extend_from_slice(value.as_bytes());
}

pub(super) fn parse_stable_db(bytes: &[u8]) -> CommandResult<StableDb> {
    // 只解析 collections.db 所需字段，保留未知字段的二进制边界检查以兼容版本变更。
    let mut offset = 0;
    let version = read_i32(bytes, &mut offset)?;
    let count = read_i32(bytes, &mut offset)?;
    if !(0..=10_000).contains(&count) {
        return Err(CommandError::new(
            "COLLECTION_PARSE_FAILED",
            "收藏夹数量超出限制",
        ));
    }
    let mut folders = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name = read_osu_string(bytes, &mut offset)?;
        let maps = read_i32(bytes, &mut offset)?;
        if !(0..=100_000).contains(&maps) {
            return Err(CommandError::new(
                "COLLECTION_PARSE_FAILED",
                "收藏夹谱面数超出限制",
            ));
        }
        let mut checksums = Vec::with_capacity(maps as usize);
        for _ in 0..maps {
            checksums.push(read_osu_string(bytes, &mut offset)?);
        }
        folders.push(StableCollection { name, checksums });
    }
    Ok(StableDb { version, folders })
}

pub(super) fn encode_stable_db(db: &StableDb) -> CommandResult<Vec<u8>> {
    // 写回时使用与 stable 客户端相同的字段顺序和字符串编码，保证客户端可继续读取。
    let mut output = Vec::new();
    output.extend_from_slice(&db.version.to_le_bytes());
    let count = i32::try_from(db.folders.len())
        .map_err(|_| CommandError::new("COLLECTION_WRITE_FAILED", "收藏夹数量过多"))?;
    output.extend_from_slice(&count.to_le_bytes());
    for folder in &db.folders {
        push_osu_string(&folder.name, &mut output);
        let maps = i32::try_from(folder.checksums.len())
            .map_err(|_| CommandError::new("COLLECTION_WRITE_FAILED", "收藏夹谱面过多"))?;
        output.extend_from_slice(&maps.to_le_bytes());
        for checksum in &folder.checksums {
            push_osu_string(checksum, &mut output);
        }
    }
    Ok(output)
}

fn stable_path(state: &AppState) -> CommandResult<PathBuf> {
    let source = state.local_analysis.source_status(LocalClient::Stable)?;
    let root = source.install_root.ok_or_else(|| {
        CommandError::new("COLLECTION_SOURCE_UNAVAILABLE", "未配置 osu!stable 目录")
    })?;
    Ok(PathBuf::from(root).join("collection.db"))
}

fn file_fingerprint(path: &Path) -> CommandResult<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn source_statuses(state: &AppState) -> Vec<CollectionSourceStatus> {
    [LocalClient::Stable, LocalClient::Lazer]
        .into_iter()
        .map(|client| {
            let source = state.local_analysis.source_status(client).ok();
            let available = source.as_ref().is_some_and(|value| value.valid);
            let (read_only, message) = match client {
                LocalClient::Stable => (
                    false,
                    if available {
                        "可读取和写回 collection.db"
                    } else {
                        "请先在设置中配置 osu!stable 目录"
                    },
                ),
                LocalClient::Lazer => (
                    true,
                    if available {
                        "lazer 收藏夹当前为只读；此版本不会修改 client.realm"
                    } else {
                        "请先在设置中配置 osu!lazer 数据目录"
                    },
                ),
            };
            CollectionSourceStatus {
                client,
                available,
                read_only,
                message: message.into(),
                refreshed_at: None,
            }
        })
        .collect()
}

#[tauri::command(async)]
pub fn list_collections(state: State<'_, AppState>) -> CommandResult<CollectionSnapshot> {
    state.collections.snapshot(source_statuses(&state))
}

#[tauri::command(async)]
pub fn get_collection_sync_status(
    state: State<'_, AppState>,
) -> CommandResult<CollectionSyncStatus> {
    let path = match stable_path(&state) {
        Ok(path) => path,
        Err(_) => {
            return Ok(CollectionSyncStatus {
                available: false,
                in_sync: false,
                pending_changes: false,
                game_changed: false,
                missing_downloadable_count: 0,
                missing_unresolved_count: 0,
            });
        }
    };
    let current_fingerprint = if path.is_file() {
        file_fingerprint(&path)?
    } else {
        String::new()
    };
    let file = state
        .collections
        .value
        .lock()
        .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
    let pending_changes = file
        .folders
        .iter()
        .any(|folder| folder.source != CollectionSource::Lazer && folder.pending_write);
    let game_changed = file.stable_fingerprint.as_deref().unwrap_or("") != current_fingerprint;
    let mut downloadable_sets = HashSet::new();
    let mut missing_unresolved_count = 0;
    for entry in file
        .folders
        .iter()
        .filter(|folder| folder.source != CollectionSource::Lazer)
        .flat_map(|folder| &folder.entries)
        .filter(|entry| !entry.resolved)
    {
        if let Some(beatmapset_id) = entry.beatmapset_id {
            downloadable_sets.insert(beatmapset_id);
        } else {
            missing_unresolved_count += 1;
        }
    }
    Ok(CollectionSyncStatus {
        available: true,
        in_sync: !pending_changes && !game_changed,
        pending_changes,
        game_changed,
        missing_downloadable_count: downloadable_sets.len(),
        missing_unresolved_count,
    })
}

pub(super) fn stable_collection_entry(
    checksum: String,
    local: Option<&LocalBeatmapSummary>,
    previous: Option<&CollectionEntry>,
) -> CollectionEntry {
    if let Some(local) = local {
        return CollectionEntry {
            id: previous
                .map(|entry| entry.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            checksum: Some(checksum.to_ascii_lowercase()),
            beatmap_id: local.beatmap_id,
            beatmapset_id: local.beatmap_set_id,
            ruleset: Some(local.ruleset.to_string()),
            difficulty_name: local.difficulty_name.clone(),
            title: local.title_unicode.clone(),
            artist: local.artist_unicode.clone(),
            creator: local.creator.clone(),
            resolved: true,
        };
    }
    if let Some(previous) = previous {
        let mut preserved = previous.clone();
        preserved.checksum = Some(checksum.to_ascii_lowercase());
        return preserved;
    }
    CollectionEntry {
        id: Uuid::new_v4().to_string(),
        checksum: Some(checksum.to_ascii_lowercase()),
        beatmap_id: None,
        beatmapset_id: None,
        ruleset: None,
        difficulty_name: "未解析难度".into(),
        title: "未解析谱面".into(),
        artist: String::new(),
        creator: String::new(),
        resolved: false,
    }
}

fn refresh_stable_collections(
    path: PathBuf,
    collections: std::sync::Arc<CollectionService>,
    local_analysis: std::sync::Arc<crate::features::local_analysis::LocalAnalysisService>,
) -> CommandResult<()> {
    // Refresh the incremental local index first so collection.db hashes are
    // resolved against the files that are currently in the Songs directory.
    // A concurrent scan is harmless: the existing index remains available.
    let _ = local_analysis.scan(LocalClient::Stable, false, std::sync::Arc::new(|_| {}));
    let (db, fingerprint) = if path.is_file() {
        let bytes = fs::read(&path)?;
        (
            parse_stable_db(&bytes)?,
            format!("{:x}", Sha256::digest(&bytes)),
        )
    } else {
        (
            StableDb {
                version: 20200101,
                folders: Vec::new(),
            },
            String::new(),
        )
    };
    let checksums = db
        .folders
        .iter()
        .flat_map(|folder| folder.checksums.iter())
        .map(|checksum| checksum.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    let resolved = local_analysis
        .find_beatmaps_by_md5(LocalClient::Stable, &checksums)
        .unwrap_or_default();
    collections.update(|file| {
        let now = Utc::now().to_rfc3339();
        let mut matched = HashSet::new();
        let mut imported = Vec::new();
        for item in db.folders {
            let existing = file
                .folders
                .iter()
                .enumerate()
                .find(|(index, folder)| {
                    folder.source != CollectionSource::Lazer
                        && folder.name == item.name
                        && !matched.contains(index)
                })
                .map(|(index, _)| index);
            let previous_entries = existing
                .map(|index| file.folders[index].entries.clone())
                .unwrap_or_default();
            let entries = item
                .checksums
                .into_iter()
                .map(|checksum| {
                    let local = resolved.get(&checksum.to_ascii_lowercase());
                    let previous = previous_entries.iter().find(|entry| {
                        entry
                            .checksum
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case(&checksum))
                    });
                    stable_collection_entry(checksum, local, previous)
                })
                .collect();
            if let Some(index) = existing {
                matched.insert(index);
                let folder = &mut file.folders[index];
                folder.entries = entries;
                folder.pending_write = false;
                folder.updated_at = now.clone();
            } else {
                imported.push(CollectionFolder {
                    id: Uuid::new_v4().to_string(),
                    name: item.name,
                    creator: "本地游戏收藏夹".into(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    source: CollectionSource::Stable,
                    read_only: false,
                    pending_write: false,
                    entries,
                });
            }
        }
        let retained_ids = matched
            .into_iter()
            .filter_map(|index| file.folders.get(index).map(|folder| folder.id.clone()))
            .collect::<HashSet<_>>();
        file.folders.retain(|folder| {
            folder.source != CollectionSource::Stable || retained_ids.contains(&folder.id)
        });
        file.folders.extend(imported);
        file.stable_version = Some(db.version);
        file.stable_fingerprint = Some(fingerprint);
        file.refreshed_at = Some(now);
        Ok(())
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：刷新外部状态。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn refresh_collections(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<CollectionSnapshot> {
    // 刷新会将 stable 的外部变化合并进内部副本，而不是覆盖本地创建的收藏夹。
    if client == LocalClient::Lazer {
        return state.collections.snapshot(source_statuses(&state));
    }
    let path = stable_path(&state)?;
    let collections = std::sync::Arc::clone(&state.collections);
    let local_analysis = std::sync::Arc::clone(&state.local_analysis);
    tokio::task::spawn_blocking(move || {
        refresh_stable_collections(path, collections, local_analysis)
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "COLLECTION_REFRESH_TASK_ERROR",
            format!("收藏夹刷新任务异常结束：{error}"),
        )
    })??;
    state.collections.snapshot(source_statuses(&state))
}

#[tauri::command(async)]
pub fn create_collection(
    name: String,
    creator: String,
    state: State<'_, AppState>,
) -> CommandResult<CollectionFolder> {
    state.collections.create(&name, &creator)
}
#[tauri::command(async)]
pub fn rename_collection(
    folder_id: String,
    name: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.collections.rename(&folder_id, &name)
}
#[tauri::command(async)]
pub fn delete_collection(folder_id: String, state: State<'_, AppState>) -> CommandResult<()> {
    state.collections.delete(&folder_id)
}
#[tauri::command(async)]
pub fn add_collection_entries(
    folder_id: String,
    mut candidates: Vec<CollectionCandidate>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    for candidate in &mut candidates {
        if candidate.checksum.is_none()
            && let (Some(client), Some(resource_id)) = (
                candidate.local_client,
                candidate.local_resource_id.as_deref(),
            )
            && let Ok(path) = state.local_analysis.beatmap_file_path(client, resource_id)
            && let Ok(bytes) = fs::read(path)
        {
            candidate.checksum = Some(format!("{:x}", Md5::digest(bytes)));
        }
    }
    state.collections.add_entries(&folder_id, candidates)
}
#[tauri::command(async)]
pub fn remove_collection_entry(
    folder_id: String,
    entry_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.collections.remove_entry(&folder_id, &entry_id)
}

#[tauri::command(async)]
pub fn write_stable_collections(
    state: State<'_, AppState>,
) -> CommandResult<CollectionWriteResult> {
    ensure_collection_task_active(&state)?;
    if get_game_status(state.clone())?
        .clients
        .iter()
        .any(|client| client.client == LocalClient::Stable && client.running)
    {
        return Err(CommandError::new(
            "GAME_RUNNING",
            "请关闭 osu!stable 后再写回收藏夹",
        ));
    }
    let path = stable_path(&state)?;
    state.collections.update(|file| {
        let current = if path.is_file() {
            file_fingerprint(&path)?
        } else {
            String::new()
        };
        if file.stable_fingerprint.as_deref().unwrap_or("") != current {
            return Err(CommandError::new(
                "COLLECTION_CONFLICT",
                "游戏收藏夹已在 OPP 外被修改，请先刷新后再写回",
            ));
        }
        let mut skipped_entries = 0usize;
        let folders = file
            .folders
            .iter()
            .filter(|folder| folder.source != CollectionSource::Lazer)
            .map(|folder| {
                let checksums = folder
                    .entries
                    .iter()
                    .filter_map(|entry| {
                        match entry
                            .checksum
                            .as_deref()
                            .filter(|checksum| !checksum.is_empty())
                        {
                            Some(value) => Some(value.to_string()),
                            None => {
                                skipped_entries += 1;
                                None
                            }
                        }
                    })
                    .collect();
                StableCollection {
                    name: folder.name.clone(),
                    checksums,
                }
            })
            .collect::<Vec<_>>();
        let db = StableDb {
            version: file.stable_version.unwrap_or(20200101),
            folders,
        };
        let bytes = encode_stable_db(&db)?;
        let temporary = path.with_extension("db.tmp");
        fs::write(&temporary, bytes)?;
        let backup = if path.exists() {
            let backup = path.with_extension("db.bak");
            if backup.exists() {
                fs::remove_file(&backup)?;
            }
            fs::rename(&path, &backup)?;
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(&temporary, &path) {
            if let Some(backup) = &backup {
                let _ = fs::rename(backup, &path);
            }
            return Err(CommandError::from(error));
        }
        file.stable_fingerprint = Some(file_fingerprint(&path)?);
        file.refreshed_at = Some(Utc::now().to_rfc3339());
        for folder in &mut file.folders {
            if folder.source != CollectionSource::Lazer {
                folder.pending_write = false;
            }
        }
        Ok(CollectionWriteResult {
            written_folders: db.folders.len(),
            skipped_entries,
            backup_path: backup.map(|value| value.display().to_string()),
        })
    })
}
