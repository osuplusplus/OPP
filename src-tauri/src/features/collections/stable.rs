//! osu!stable `collection.db` 的二进制读写与收藏夹同步编排。
//! 文件格式：小端 `i32` 版本号、`i32` 收藏夹数量，随后每个收藏夹依次为
//! 名称字符串、`i32` 成员数量、数量个以难度 MD5 为内容的字符串；
//! 字符串采用 osu! 变长编码：空串为 `0x00`，否则 `0x0b` + ULEB128 长度 + UTF-8 字节。
//! 同步以 SHA-256 指纹识别外部修改，写回走“临时文件 + 备份 + 失败回滚”流程。

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use md5::Md5;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use super::adapter;
use super::{CollectionBackupStatus, CollectionManagerStatus};
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

/// collection.db 中单个收藏夹：名称与成员难度文件的 MD5 校验和列表。
#[derive(Debug, Clone)]
pub(super) struct StableCollection {
    pub(super) name: String,
    pub(super) checksums: Vec<String>,
}

/// 整个 collection.db 的内存镜像；版本号解析时保留，写回未知版本回退 `20200101`。
#[derive(Debug, Clone)]
pub(super) struct StableDb {
    pub(super) version: i32,
    pub(super) folders: Vec<StableCollection>,
}

/// 读取小端 `i32`；字节不足或偏移溢出时返回解析错误而非 panic。
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

/// 读取无符号 LEB128 变长整数，stable 用它编码字符串的字节长度。
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

/// 读取 osu! 字符串：`0x00` 表示空串；其余必须以 `0x0b` 开头，
/// 后随 ULEB128 字节长度与 UTF-8 内容。
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

/// 写入无符号 LEB128 变长整数，`read_uleb` 的逆操作。
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

/// 写入 osu! 字符串：空串写单个 `0x00`，否则 `0x0b` + ULEB128 长度 + 字节内容。
fn push_osu_string(value: &str, output: &mut Vec<u8>) {
    if value.is_empty() {
        output.push(0);
        return;
    }
    output.push(0x0b);
    push_uleb(value.len(), output);
    output.extend_from_slice(value.as_bytes());
}

/// 解析 collection.db 二进制；收藏夹与成员数量设上限，防止损坏文件引发巨量分配。
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

/// 将内存镜像编码为 collection.db 字节流；集合数量超出 `i32` 范围时拒绝写入。
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

/// 定位 osu!stable 安装目录下的 `collection.db`；目录未配置时报来源不可用。
fn stable_path(state: &AppState) -> CommandResult<PathBuf> {
    let source = state.local_analysis.source_status(LocalClient::Stable)?;
    let root = source.install_root.ok_or_else(|| {
        CommandError::new("COLLECTION_SOURCE_UNAVAILABLE", "未配置 osu!stable 目录")
    })?;
    Ok(PathBuf::from(root).join("collection.db"))
}

/// 计算文件内容的 SHA-256 指纹，用于识别 OPP 之外的外部修改。
fn file_fingerprint(path: &Path) -> CommandResult<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// 汇总 stable 与 lazer 两个来源的可用性：
/// stable 目录有效即可读写；lazer 还必须配置外部 CollectionManager，否则不可用。
pub(super) fn source_statuses(state: &AppState) -> Vec<CollectionSourceStatus> {
    let manager_configured = state
        .store
        .settings_snapshot()
        .ok()
        .and_then(|settings| settings.collection_manager_path)
        .is_some_and(|path| Path::new(&path).is_file());
    [LocalClient::Stable, LocalClient::Lazer]
        .into_iter()
        .map(|client| {
            let source = state.local_analysis.source_status(client).ok();
            let source_available = source.as_ref().is_some_and(|value| value.valid);
            let available =
                source_available && (client == LocalClient::Stable || manager_configured);
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
                    !manager_configured,
                    if !source_available {
                        "请先在设置中配置 osu!lazer 数据目录"
                    } else if !manager_configured {
                        "未配置有效的 CollectionManager，无法读取或写回 lazer 收藏夹"
                    } else {
                        "已配置 CollectionManager；读取与写回能力需通过其协议验证"
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

/// Tauri 命令：返回全部收藏夹与各来源可用性组成的快照。
#[tauri::command(async)]
pub async fn list_collections(app: AppHandle) -> CommandResult<CollectionSnapshot> {
    crate::infrastructure::tasks::blocking_io("list_collections", move || {
        let state = app.state::<AppState>();
        state.collections.snapshot(source_statuses(&state))
    })
    .await?
}

/// Tauri 命令：检查与 `collection.db` 的同步状态。
/// 以磁盘文件 SHA-256 与内存指纹对比，区分“OPP 待写回”与“游戏侧外部修改”，
/// 并统计缺失谱面中可自动下载（已知谱面集）与仅有旧 MD5 的数量。
#[tauri::command(async)]
pub async fn get_collection_sync_status(app: AppHandle) -> CommandResult<CollectionSyncStatus> {
    crate::infrastructure::tasks::blocking_io("get_collection_sync_status", move || {
        let state = app.state::<AppState>();
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
                    sources: vec![],
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
            .any(|folder| folder.participates_in_stable() && folder.pending_write);
        let game_changed = file.stable_fingerprint.as_deref().unwrap_or("") != current_fingerprint;
        let mut downloadable_sets = HashSet::new();
        let mut missing_unresolved_count = 0;
        for entry in file
            .folders
            .iter()
            .filter(|folder| folder.participates_in_stable())
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
            sources: vec![],
        })
    })
    .await?
}

/// 由校验和构造收藏条目，按优先级回退：本地索引命中则填全元数据，
/// 否则沿用旧条目（仅刷新校验和），两者皆无时生成“未解析谱面”占位。
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

/// 读取磁盘上的 collection.db 并合并进内部副本：
/// 按名称与现有 stable 文件夹逐一配对更新，新名称导入，未出现的 stable 文件夹移除；
/// 文件不存在时视为空库（版本 `20200101`）。
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
            // 重名收藏夹按出现顺序逐个配对，`matched` 防止多个内部文件夹指向同一来源。
            let existing = file
                .folders
                .iter()
                .enumerate()
                .find(|(index, folder)| {
                    folder.participates_in_stable()
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
                    stable_sync: true,
                    pool: None,
                    entries,
                    external_id: None,
                    external_fingerprint: None,
                    last_read_at: Some(now.clone()),
                    backup_path: None,
                    backup_fingerprint: None,
                    backup_confirmed_at: None,
                });
            }
        }
        let retained_ids = matched
            .into_iter()
            .filter_map(|index| file.folders.get(index).map(|folder| folder.id.clone()))
            .collect::<HashSet<_>>();
        // 仅移除本次 collection.db 中未出现的 stable 来源文件夹，其余来源保持不变。
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

/// Tauri 命令：刷新指定客户端的收藏夹并返回最新快照（stable 合并、lazer 全量替换）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：刷新外部状态。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn refresh_collections(
    client: LocalClient,
    app: AppHandle,
) -> CommandResult<CollectionSnapshot> {
    refresh_impl(client, &app.state::<AppState>()).await?;
    list_collections(app).await
}

/// 刷新实现：lazer 通过外部 CollectionManager 协议读取并全量替换镜像；
/// stable 交给后台线程做文件解析与合并，避免阻塞调用方。
pub(super) async fn refresh_impl(client: LocalClient, state: &AppState) -> CommandResult<()> {
    // 刷新会将 stable 的外部变化合并进内部副本，而不是覆盖本地创建的收藏夹。
    if client == LocalClient::Lazer {
        let folders = adapter::invoke::<Vec<CollectionFolder>, ()>(state, "read", None).await?;
        let collections = state.collections.clone();
        crate::infrastructure::tasks::blocking_io("refresh_lazer_collections", move || {
            collections.update(|file| {
                file.folders.retain(|f| f.source != CollectionSource::Lazer);
                file.folders.extend(folders);
                Ok(())
            })
        })
        .await??;
        return Ok(());
    }
    let path = stable_path(state)?;
    let collections = std::sync::Arc::clone(&state.collections);
    let local_analysis = std::sync::Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("refresh_stable_collections", move || {
        refresh_stable_collections(path, collections, local_analysis)
    })
    .await??;
    Ok(())
}

/// Tauri 命令：查询外部 CollectionManager 的连接状态。
#[tauri::command(async)]
pub async fn get_collection_manager_status(
    state: State<'_, AppState>,
) -> CommandResult<CollectionManagerStatus> {
    Ok(adapter::status(&state).await)
}

/// Tauri 命令：设置或清空 CollectionManager 路径；空白字符串按未配置处理。
#[tauri::command(async)]
pub fn set_collection_manager_path(
    path: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state
        .store
        .update(|s| s.settings.collection_manager_path = path.filter(|p| !p.trim().is_empty()))
}

/// Tauri 命令：查询备份状态，含目标文件指纹与备份目录（可在设置中指定）内的文件列表。
#[tauri::command(async)]
pub fn get_collection_backup_status(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<CollectionBackupStatus> {
    let target = match client {
        LocalClient::Stable => stable_path(&state).ok(),
        LocalClient::Lazer => state
            .local_analysis
            .source_status(LocalClient::Lazer)
            .ok()
            .and_then(|s| s.install_root)
            .map(|r| PathBuf::from(r).join("client.realm")),
    };
    let dir = state
        .store
        .settings_snapshot()?
        .collection_backup_directory
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("collection-backups"));
    let backups = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flat_map(|it| it.filter_map(Result::ok))
        .filter_map(|e| e.path().to_str().map(str::to_string))
        .collect::<Vec<_>>();
    let fingerprint = target
        .as_ref()
        .filter(|p| p.is_file())
        .and_then(|p| file_fingerprint(p).ok());
    Ok(CollectionBackupStatus {
        client,
        target: target.map(|p| p.display().to_string()),
        fingerprint,
        latest: backups.last().cloned(),
        backups,
    })
}

/// Tauri 命令：复制收藏夹文件生成“客户端-时间戳-指纹前 12 位”命名的备份。
#[tauri::command(async)]
pub fn create_collection_backup(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<CollectionBackupStatus> {
    let status = get_collection_backup_status(client, state.clone())?;
    let target =
        status.target.clone().map(PathBuf::from).ok_or_else(|| {
            CommandError::new("COLLECTION_SOURCE_UNAVAILABLE", "收藏夹文件不可用")
        })?;
    if !target.is_file() {
        return Ok(status);
    }
    let dir = PathBuf::from("collection-backups");
    fs::create_dir_all(&dir)?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let fp = status.fingerprint.clone().unwrap_or_default();
    let out = dir.join(format!(
        "{}-{}-{}.bak",
        client,
        stamp,
        &fp[..fp.len().min(12)]
    ));
    fs::copy(&target, &out)?;
    get_collection_backup_status(client, state)
}

/// Tauri 命令：用备份文件覆盖当前收藏夹；对应客户端游戏运行中时拒绝执行。
#[tauri::command(async)]
pub fn restore_collection_backup(
    client: LocalClient,
    backup_path: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    if get_game_status(state.clone())?
        .clients
        .iter()
        .any(|c| c.client == client && c.running)
    {
        return Err(CommandError::new(
            "GAME_RUNNING",
            "请关闭游戏后再恢复收藏夹",
        ));
    }
    let target = get_collection_backup_status(client, state)?
        .target
        .ok_or_else(|| CommandError::new("COLLECTION_SOURCE_UNAVAILABLE", "收藏夹文件不可用"))?;
    let source = PathBuf::from(backup_path);
    if !source.is_file() {
        return Err(CommandError::new("BACKUP_NOT_FOUND", "备份文件不存在"));
    }
    fs::copy(source, target)?;
    Ok(())
}

/// Tauri 命令：把 lazer 来源的收藏夹经 CollectionManager 协议写回。
#[tauri::command(async)]
pub async fn write_lazer_collections(
    state: State<'_, AppState>,
) -> CommandResult<CollectionWriteResult> {
    let folders = state
        .collections
        .value
        .lock()
        .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?
        .folders
        .iter()
        .filter(|f| f.source == CollectionSource::Lazer)
        .cloned()
        .collect::<Vec<_>>();
    let _: serde_json::Value = adapter::invoke(&state, "write", Some(folders)).await?;
    Ok(CollectionWriteResult {
        written_folders: 1,
        skipped_entries: 0,
        backup_path: None,
    })
}

/// Tauri 命令：新建 OPP 本地收藏夹，默认参与 stable 同步并立即进入待写回状态。
#[tauri::command(async)]
pub async fn create_collection(
    name: String,
    creator: String,
    app: AppHandle,
) -> CommandResult<CollectionFolder> {
    crate::infrastructure::tasks::blocking_io("create_collection", move || {
        let state = app.state::<AppState>();

        state.collections.create(&name, &creator)
    })
    .await?
}
/// Tauri 命令：重命名收藏夹；只读文件夹会被拒绝。
#[tauri::command(async)]
pub async fn rename_collection(
    folder_id: String,
    name: String,
    app: AppHandle,
) -> CommandResult<()> {
    crate::infrastructure::tasks::blocking_io("rename_collection", move || {
        let state = app.state::<AppState>();

        state.collections.rename(&folder_id, &name)
    })
    .await?
}
/// Tauri 命令：删除收藏夹，同时清理其笔记本数据。
#[tauri::command(async)]
pub async fn delete_collection(folder_id: String, app: AppHandle) -> CommandResult<()> {
    crate::infrastructure::tasks::blocking_io("delete_collection", move || {
        let state = app.state::<AppState>();

        state.collections.delete(&folder_id)
    })
    .await?
}
/// Tauri 命令：批量添加条目；未携带校验和的本地候选现场读取 .osu 文件计算 MD5。
#[tauri::command(async)]
pub async fn add_collection_entries(
    folder_id: String,
    mut candidates: Vec<CollectionCandidate>,
    app: AppHandle,
) -> CommandResult<()> {
    crate::infrastructure::tasks::blocking_io("add_collection_entries", move || {
        let state = app.state::<AppState>();

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
    })
    .await?
}
/// Tauri 命令：从收藏夹移除单个条目。
#[tauri::command(async)]
pub async fn remove_collection_entry(
    folder_id: String,
    entry_id: String,
    app: AppHandle,
) -> CommandResult<()> {
    crate::infrastructure::tasks::blocking_io("remove_collection_entry", move || {
        let state = app.state::<AppState>();

        state.collections.remove_entry(&folder_id, &entry_id)
    })
    .await?
}

/// Tauri 命令：把参与 stable 同步的收藏夹写回 `collection.db`。
/// 前置校验：游戏运行中拒绝；磁盘指纹与内存指纹不一致（外部已修改）时要求先刷新。
/// 落盘流程：写临时文件 → 旧库改名 `.db.bak` → 原子替换，任一步失败回滚备份。
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
        let (db, skipped_entries) = stable_db_for_write(file);
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
            if folder.participates_in_stable() {
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

/// Tauri 命令：让指定收藏夹开始参与 `collection.db` 写回。
#[tauri::command]
pub async fn enable_collection_stable_sync(folder_id: String, app: AppHandle) -> CommandResult<()> {
    crate::infrastructure::tasks::blocking_io("enable_collection_stable_sync", move || {
        let state = app.state::<AppState>();
        state.collections.enable_stable_sync(&folder_id)
    })
    .await?
}

/// 从内部状态构造待写回的 `StableDb`：空校验和的条目跳过并计数，版本号沿用解析值。
pub(super) fn stable_db_for_write(file: &super::service::CollectionFile) -> (StableDb, usize) {
    let mut skipped_entries = 0usize;
    let folders = file
        .folders
        .iter()
        .filter(|folder| folder.participates_in_stable())
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
    (db, skipped_entries)
}
