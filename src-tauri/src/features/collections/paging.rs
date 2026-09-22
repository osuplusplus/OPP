use super::{
    CollectionEntry, CollectionFolder, CollectionService, CollectionSource, CollectionSourceStatus,
};
use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};
use serde::Serialize;
use std::collections::HashSet;
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize)]
pub struct CollectionFolderSummary {
    pub id: String,
    pub name: String,
    pub creator: String,
    pub source: CollectionSource,
    pub read_only: bool,
    pub pending_write: bool,
    pub stable_sync: bool,
    pub entry_count: usize,
    pub missing_count: usize,
    pub beatmapset_count: usize,
    pub revision: u64,
}
#[derive(Debug, Serialize)]
pub struct CollectionSummaries {
    pub folders: Vec<CollectionFolderSummary>,
    pub sources: Vec<CollectionSourceStatus>,
}
#[derive(Debug, Serialize)]
pub struct CollectionEntryPage {
    pub items: Vec<CollectionEntry>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub revision: u64,
}
fn summary(folder: &CollectionFolder, revision: u64) -> CollectionFolderSummary {
    let missing_count = folder
        .entries
        .iter()
        .filter(|entry| !entry.resolved)
        .filter_map(|entry| {
            entry
                .beatmapset_id
                .map(|id| format!("id:{id}"))
                .or_else(|| entry.checksum.as_ref().map(|hash| format!("hash:{hash}")))
        })
        .collect::<HashSet<_>>()
        .len();
    CollectionFolderSummary {
        id: folder.id.clone(),
        name: folder.name.clone(),
        creator: folder.creator.clone(),
        source: folder.source.clone(),
        read_only: folder.read_only,
        pending_write: folder.pending_write && folder.participates_in_stable(),
        stable_sync: folder.stable_sync,
        entry_count: folder.entries.len(),
        missing_count,
        beatmapset_count: folder
            .entries
            .iter()
            .filter_map(|entry| entry.beatmapset_id)
            .filter(|id| *id != 0)
            .collect::<HashSet<_>>()
            .len(),
        revision,
    }
}
impl CollectionService {
    pub(super) fn summaries(
        &self,
        sources: Vec<CollectionSourceStatus>,
    ) -> CommandResult<CollectionSummaries> {
        let file = self.value.lock()?;
        Ok(CollectionSummaries {
            folders: file
                .folders
                .iter()
                .map(|folder| summary(folder, *file.revisions.get(&folder.id).unwrap_or(&0)))
                .collect(),
            sources,
        })
    }
    pub(super) fn entry_page(
        &self,
        folder_id: &str,
        offset: usize,
        limit: usize,
        revision: u64,
    ) -> CommandResult<CollectionEntryPage> {
        let file = self.value.lock()?;
        let folder = file
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
            .ok_or_else(|| CommandError::new("COLLECTION_NOT_FOUND", "未找到收藏夹"))?;
        let current = *file.revisions.get(folder_id).unwrap_or(&0);
        if current != revision {
            return Err(CommandError::new(
                "COLLECTION_REVISION_CHANGED",
                "收藏夹已更新，请重新加载",
            ));
        }
        let limit = limit.clamp(1, 100);
        Ok(CollectionEntryPage {
            items: folder
                .entries
                .iter()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect(),
            total: folder.entries.len(),
            offset,
            limit,
            revision: current,
        })
    }
}

#[tauri::command]
pub async fn list_collection_summaries(app: AppHandle) -> CommandResult<CollectionSummaries> {
    crate::infrastructure::tasks::blocking_io("list_collection_summaries", move || {
        let state = app.state::<AppState>();
        state
            .collections
            .summaries(super::stable::source_statuses(&state))
    })
    .await?
}
#[tauri::command]
pub async fn query_collection_entries(
    app: AppHandle,
    folder_id: String,
    offset: usize,
    limit: usize,
    revision: u64,
) -> CommandResult<CollectionEntryPage> {
    crate::infrastructure::tasks::blocking_io("query_collection_entries", move || {
        app.state::<AppState>()
            .collections
            .entry_page(&folder_id, offset, limit, revision)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "release performance measurement"]
    fn performance_collection_payloads() {
        for size in [0, 1_000, 10_000] {
            let dir = tempfile::tempdir().unwrap();
            let service = CollectionService::new(dir.path()).unwrap();
            let folder = service.create("Fixture", "benchmark").unwrap();
            service
                .update(|file| {
                    file.folders[0].entries = (0..size)
                        .map(|id| CollectionEntry {
                            id: id.to_string(),
                            beatmap_id: Some(id),
                            beatmapset_id: Some(id / 4),
                            checksum: None,
                            ruleset: Some("osu".into()),
                            difficulty_name: "Hard".into(),
                            title: format!("Song {id}"),
                            artist: "Artist".into(),
                            creator: "Mapper".into(),
                            resolved: true,
                        })
                        .collect();
                    Ok(())
                })
                .unwrap();
            let summaries = service.summaries(vec![]).unwrap();
            let page = service
                .entry_page(&folder.id, 0, 12, summaries.folders[0].revision)
                .unwrap();
            let full_bytes = serde_json::to_vec(&service.snapshot(vec![]).unwrap())
                .unwrap()
                .len();
            let paged_bytes = serde_json::to_vec(&summaries).unwrap().len()
                + serde_json::to_vec(&page).unwrap().len();
            let mut before = Vec::new();
            let mut after = Vec::new();
            for iteration in 0..35 {
                for optimized in if iteration % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = std::time::Instant::now();
                    if optimized {
                        let summaries = service.summaries(vec![]).unwrap();
                        let page = service
                            .entry_page(&folder.id, 0, 12, summaries.folders[0].revision)
                            .unwrap();
                        std::hint::black_box(serde_json::to_vec(&(summaries, page)).unwrap());
                    } else {
                        std::hint::black_box(
                            serde_json::to_vec(&service.snapshot(vec![]).unwrap()).unwrap(),
                        );
                    }
                    if iteration >= 5 {
                        if optimized {
                            after.push(start.elapsed().as_micros());
                        } else {
                            before.push(start.elapsed().as_micros());
                        }
                    }
                }
            }
            before.sort_unstable();
            after.sort_unstable();
            println!(
                "PERF {}",
                serde_json::json!({"scenario": "collections", "size": size, "samples": 30, "full_bytes": full_bytes, "paged_bytes": paged_bytes,
                "median_us": after[15], "p95_us": after[28], "reference_median_us": before[15], "reference_p95_us": before[28]})
            );
        }
    }

    #[test]
    fn concurrent_folder_edits_survive_reload() {
        let dir = tempfile::tempdir().unwrap();
        let service = CollectionService::new(dir.path()).unwrap();
        let first = service.create("first", "").unwrap();
        let second = service.create("second", "").unwrap();
        std::thread::scope(|scope| {
            for folder in [&first, &second] {
                let service = &service;
                scope.spawn(move || {
                    for version in 0..8 {
                        service
                            .rename(&folder.id, &format!("{}-{version}", folder.id))
                            .unwrap();
                    }
                });
            }
        });
        let restored = CollectionService::new(dir.path()).unwrap();
        for folder in [&first, &second] {
            assert_eq!(
                restored.folder(&folder.id).unwrap().name,
                format!("{}-7", folder.id)
            );
        }
    }

    #[test]
    fn failed_edit_does_not_publish_and_lazy_load_precedes_first_write() {
        let dir = tempfile::tempdir().unwrap();
        let original = CollectionService::new(dir.path()).unwrap();
        let folder = original.create("saved", "").unwrap();
        let service = CollectionService::new(dir.path()).unwrap();
        // First edit loads and migrates before writing, preserving the existing folder.
        service.rename(&folder.id, "loaded").unwrap();
        let shard = std::fs::read_dir(dir.path().join("collections-data"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::create_dir(shard.with_extension("json.tmp")).unwrap();
        assert!(service.rename(&folder.id, "must not publish").is_err());
        assert_eq!(service.folder(&folder.id).unwrap().name, "loaded");
        assert_eq!(
            CollectionService::new(dir.path())
                .unwrap()
                .folder(&folder.id)
                .unwrap()
                .name,
            "loaded"
        );
    }

    #[test]
    fn rejects_stale_pages_without_invalidating_unrelated_folders() {
        let dir = tempfile::tempdir().unwrap();
        let service = CollectionService::new(dir.path()).unwrap();
        let a = service.create("A", "").unwrap();
        let b = service.create("B", "").unwrap();
        let before = service.summaries(vec![]).unwrap();
        let revision_a = before
            .folders
            .iter()
            .find(|folder| folder.id == a.id)
            .unwrap()
            .revision;
        let revision_b = before
            .folders
            .iter()
            .find(|folder| folder.id == b.id)
            .unwrap()
            .revision;
        service.rename(&a.id, "renamed").unwrap();
        assert_eq!(
            service
                .entry_page(&a.id, 0, 12, revision_a)
                .unwrap_err()
                .code,
            "COLLECTION_REVISION_CHANGED"
        );
        assert!(
            service
                .entry_page(&b.id, 100, 12, revision_b)
                .unwrap()
                .items
                .is_empty()
        );
        let reopened = CollectionService::new(dir.path()).unwrap();
        assert_eq!(reopened.folder(&a.id).unwrap().name, "renamed");
    }
}

#[tauri::command]
pub async fn refresh_collection_summaries(
    app: AppHandle,
    client: crate::features::local_analysis::LocalClient,
) -> CommandResult<CollectionSummaries> {
    super::stable::refresh_impl(client, &app.state::<AppState>()).await?;
    list_collection_summaries(app).await
}
