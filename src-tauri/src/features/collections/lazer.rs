//! osu!lazer 收藏夹的本地读取：直接解析数据目录中 `client.realm` 的
//! `class_BeatmapCollection` 表，不再依赖外部 CollectionManager。
//!
//! Realm 解析库为只读，无法写回游戏；lazer 来源的收藏夹因此是只读镜像，
//! 编辑与 Stable 写回均不适用（`read_only` 恒为真）。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::Utc;

use super::stable::stable_collection_entry;
use super::{CollectionEntry, CollectionFolder, CollectionSource};
use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::{
        LocalBeatmapSummary, LocalClient,
        lazer_realm::{LazerRealmCollection, read_realm_collections},
    },
    state::AppState,
};
use std::path::PathBuf;

/// 定位 lazer 数据目录下的 `client.realm`；目录未配置时报来源不可用。
/// 注意收藏夹在数据目录（`data_root`）而非安装目录，自动探测时两者不同。
pub(super) fn lazer_realm_path(state: &AppState) -> CommandResult<PathBuf> {
    let source = state.local_analysis.source_status(LocalClient::Lazer)?;
    let root = source.data_root.ok_or_else(|| {
        CommandError::new("COLLECTION_SOURCE_UNAVAILABLE", "未配置 osu!lazer 数据目录")
    })?;
    Ok(PathBuf::from(root).join("client.realm"))
}

/// 把 Realm 收藏行转换为内部收藏夹镜像：
/// 文件夹 ID 用 `lazer:<Realm UUID>`，跨刷新保持稳定，条目 ID 也能沿用旧值；
/// 条目按 MD5 命中本地索引则填全元数据，否则沿用旧条目或生成占位。
pub(super) fn build_lazer_folders(
    realm_collections: &[LazerRealmCollection],
    resolved: &BTreeMap<String, LocalBeatmapSummary>,
    previous_by_id: &HashMap<String, Vec<CollectionEntry>>,
    now: &str,
) -> Vec<CollectionFolder> {
    realm_collections
        .iter()
        .map(|collection| {
            let folder_id = format!("lazer:{}", collection.id);
            let previous_entries = previous_by_id
                .get(&folder_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let entries = collection
                .md5_hashes
                .iter()
                .map(|checksum| {
                    let local = resolved.get(&checksum.to_ascii_lowercase());
                    let previous = previous_entries.iter().find(|entry| {
                        entry
                            .checksum
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case(checksum))
                    });
                    stable_collection_entry(checksum.clone(), local, previous)
                })
                .collect();
            CollectionFolder {
                id: folder_id,
                name: collection.name.clone(),
                creator: "本地游戏收藏夹".into(),
                created_at: now.to_string(),
                updated_at: collection
                    .last_modified
                    .clone()
                    .unwrap_or_else(|| now.to_string()),
                source: CollectionSource::Lazer,
                read_only: true,
                pending_write: false,
                stable_sync: false,
                pool: None,
                entries,
                external_id: None,
                external_fingerprint: None,
                last_read_at: Some(now.to_string()),
                backup_path: None,
                backup_fingerprint: None,
                backup_confirmed_at: None,
            }
        })
        .collect()
}

/// 刷新 lazer 收藏夹：增量更新本地索引 → 快照读取 `client.realm` →
/// 按 MD5 解析条目元数据 → 全量替换 lazer 来源的镜像文件夹。
pub(super) async fn refresh_lazer_collections(state: &AppState) -> CommandResult<()> {
    let realm_path = lazer_realm_path(state)?;
    if !realm_path.is_file() {
        return Err(CommandError::new(
            "COLLECTION_SOURCE_UNAVAILABLE",
            "未找到 osu!lazer 的 client.realm",
        ));
    }
    let collections = std::sync::Arc::clone(&state.collections);
    let local_analysis = std::sync::Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("refresh_lazer_collections", move || {
        // 先刷新增量本地索引，让收藏夹 MD5 能对上当前 files/ 里的谱面；
        // 并发扫描无害：既有索引在扫描期间仍可查询。
        // 扫描尽力而为：损坏的 Realm 会让扫描线程 panic，这里捕获后继续
        // 走收藏读取自身的结构化错误（否则整个刷新只会得到 TASK_PANICKED）。
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            local_analysis.scan(LocalClient::Lazer, false, std::sync::Arc::new(|_| {}))
        }));
        let realm_collections = read_realm_collections(&realm_path)
            .map_err(|message| CommandError::new("COLLECTION_PARSE_FAILED", message))?;
        let checksums = realm_collections
            .iter()
            .flat_map(|collection| collection.md5_hashes.iter())
            .map(|checksum| checksum.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let resolved = local_analysis
            .find_beatmaps_by_md5(LocalClient::Lazer, &checksums)
            .unwrap_or_default();
        collections.update(|file| {
            let now = Utc::now().to_rfc3339();
            let previous_by_id: HashMap<String, Vec<CollectionEntry>> = file
                .folders
                .iter()
                .filter(|folder| folder.source == CollectionSource::Lazer)
                .map(|folder| (folder.id.clone(), folder.entries.clone()))
                .collect();
            let folders = build_lazer_folders(&realm_collections, &resolved, &previous_by_id, &now);
            file.folders.retain(|f| f.source != CollectionSource::Lazer);
            file.folders.extend(folders);
            Ok(())
        })
    })
    .await?
}
