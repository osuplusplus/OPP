use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::infrastructure::lazy_mutex::LazyMutex;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    CollectionCandidate, CollectionEntry, CollectionFolder, CollectionSnapshot, CollectionSource,
    CollectionSourceStatus,
};
use crate::error::{CommandError, CommandResult};

const MAX_PRESENCE_CACHE_ENTRIES: usize = 50_000;
const MAX_PRESENCE_CACHE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct CollectionFile {
    #[serde(skip)]
    pub(super) revisions: HashMap<String, u64>,
    #[serde(skip)]
    pub(super) revision: u64,
    #[serde(default)]
    pub(super) sharded: bool,
    #[serde(default)]
    pub(super) folder_order: Vec<String>,
    #[serde(default)]
    pub(super) folders: Vec<CollectionFolder>,
    #[serde(default)]
    pub(super) stable_fingerprint: Option<String>,
    #[serde(default)]
    pub(super) stable_version: Option<i32>,
    #[serde(default)]
    pub(super) refreshed_at: Option<String>,
    #[serde(default)]
    pub(super) local_presence_cache: HashMap<String, LocalPresenceCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LocalPresenceCacheEntry {
    pub(super) present: bool,
    pub(super) scan_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CollectionCacheFile {
    #[serde(default)]
    local_presence_cache: HashMap<String, LocalPresenceCacheEntry>,
}

#[derive(Default)]
struct CollectionPersistedBytes {
    initialized: bool,
    collections: Vec<u8>,
    cache: Vec<u8>,
    folders: HashMap<String, Vec<u8>>,
}

pub(super) fn cached_local_presence(
    cache: &HashMap<String, LocalPresenceCacheEntry>,
    checksum: &str,
    scan_at: &Option<String>,
) -> Option<bool> {
    cache
        .get(checksum)
        .filter(|cached| &cached.scan_at == scan_at)
        .map(|cached| cached.present)
}

pub struct CollectionService {
    pub(crate) notebooks: super::notebook::NotebookStore,
    path: PathBuf,
    cache_path: PathBuf,
    folders_path: PathBuf,
    pub(super) value: LazyMutex<CollectionFile>,
    persist: Mutex<CollectionPersistedBytes>,
}

impl CollectionService {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let root = app_data_dir.to_path_buf();
        Ok(Self {
            notebooks: super::notebook::NotebookStore::new(app_data_dir),
            path: root.join("collections.json"),
            cache_path: root.join("collections-cache.json"),
            folders_path: root.join("collections-data"),
            value: LazyMutex::new(move || Self::load_file(&root)),
            persist: Mutex::new(CollectionPersistedBytes::default()),
        })
    }

    fn load_file(app_data_dir: &Path) -> CommandResult<CollectionFile> {
        // 收藏夹按文件夹拆分持久化，单个文件损坏不会使整个集合不可恢复。
        fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("collections.json");
        let collection_bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let mut value: CollectionFile = serde_json::from_slice(&collection_bytes)
            .ok()
            .unwrap_or_default();
        let cache_path = app_data_dir.join("collections-cache.json");
        let cache_bytes = fs::read(&cache_path).unwrap_or_default();
        if let Ok(cache) = serde_json::from_slice::<CollectionCacheFile>(&cache_bytes) {
            value.local_presence_cache = cache.local_presence_cache;
        }
        let folders_path = app_data_dir.join("collections-data");
        fs::create_dir_all(&folders_path)?;
        let mut sharded_folders = HashMap::new();
        for entry in fs::read_dir(&folders_path)?.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let bytes = fs::read(&path)?;
            let Ok(folder) = serde_json::from_slice::<CollectionFolder>(&bytes) else {
                continue;
            };
            sharded_folders.insert(folder.id.clone(), folder);
        }
        if value.sharded {
            value.folders = value
                .folder_order
                .iter()
                .filter_map(|id| sharded_folders.remove(id))
                .collect();
            let mut remaining = sharded_folders.into_values().collect::<Vec<_>>();
            remaining.sort_by(|left, right| left.id.cmp(&right.id));
            value.folders.extend(remaining);
        }
        prune_presence_cache(&mut value.local_presence_cache);
        Ok(value)
    }

    pub(super) fn update<R>(
        &self,
        action: impl FnOnce(&mut CollectionFile) -> CommandResult<R>,
    ) -> CommandResult<R> {
        // 在持久化锁内读改写，确保并发编辑不会丢失另一个调用刚写入的条目。
        let mut persisted = self
            .persist
            .lock()
            .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹持久化状态不可用"))?;
        let mut file = self.value.lock()?.clone();
        let result = action(&mut file)?;
        prune_presence_cache(&mut file.local_presence_cache);
        let cache = CollectionCacheFile {
            local_presence_cache: file.local_presence_cache.clone(),
        };
        let durable = CollectionFile {
            sharded: true,
            folder_order: file
                .folders
                .iter()
                .map(|folder| folder.id.clone())
                .collect(),
            stable_fingerprint: file.stable_fingerprint.clone(),
            stable_version: file.stable_version,
            refreshed_at: file.refreshed_at.clone(),
            ..Default::default()
        };
        let collection_bytes = serde_json::to_vec_pretty(&durable)?;
        let cache_bytes = serde_json::to_vec(&cache)?;
        if persisted.cache != cache_bytes {
            let temporary = self.cache_path.with_extension("json.tmp");
            fs::write(&temporary, &cache_bytes)?;
            atomic_replace(&temporary, &self.cache_path)?;
            persisted.cache = cache_bytes;
        }
        let mut current_folder_ids = HashSet::new();
        for folder in &file.folders {
            current_folder_ids.insert(folder.id.clone());
            let bytes = serde_json::to_vec(&folder)?;
            if persisted.folders.get(&folder.id) == Some(&bytes) {
                continue;
            }
            let target = self
                .folders_path
                .join(format!("{}.json", folder_storage_key(&folder.id)));
            let temporary = target.with_extension("json.tmp");
            fs::write(&temporary, &bytes)?;
            atomic_replace(&temporary, &target)?;
            persisted.folders.insert(folder.id.clone(), bytes);
            file.revision += 1;
            file.revisions.insert(folder.id.clone(), file.revision);
        }
        let removed = persisted
            .folders
            .keys()
            .filter(|id| !current_folder_ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in removed {
            let target = self
                .folders_path
                .join(format!("{}.json", folder_storage_key(&id)));
            if target.exists() {
                fs::remove_file(target)?;
            }
            persisted.folders.remove(&id);
        }
        if persisted.collections != collection_bytes {
            let temporary = self.path.with_extension("json.tmp");
            fs::write(&temporary, &collection_bytes)?;
            atomic_replace(&temporary, &self.path)?;
            persisted.collections = collection_bytes;
        }
        file.sharded = true;
        file.folder_order = durable.folder_order;
        file.revisions
            .retain(|id, _| current_folder_ids.contains(id));
        *self.value.lock()? = file;
        persisted.initialized = true;
        Ok(result)
    }

    /// Common edits copy and serialize only the selected folder, after initial migration.
    fn edit_folder<R>(
        &self,
        folder_id: &str,
        action: impl FnOnce(&mut CollectionFolder) -> CommandResult<R>,
    ) -> CommandResult<R> {
        let mut persisted = self
            .persist
            .lock()
            .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹持久化状态不可用"))?;
        if !persisted.initialized {
            drop(persisted);
            return self.update(|file| action(folder_mut(file, folder_id)?));
        }
        let mut folder = self.folder(folder_id)?;
        let result = action(&mut folder)?;
        let bytes = serde_json::to_vec(&folder)?;
        if persisted.folders.get(folder_id) != Some(&bytes) {
            let target = self
                .folders_path
                .join(format!("{}.json", folder_storage_key(folder_id)));
            let temporary = target.with_extension("json.tmp");
            fs::write(&temporary, &bytes)?;
            atomic_replace(&temporary, &target)?;
            let mut file = self.value.lock()?;
            *folder_mut(&mut file, folder_id)? = folder;
            file.revision += 1;
            let revision = file.revision;
            file.revisions.insert(folder_id.to_owned(), revision);
            persisted.folders.insert(folder_id.to_owned(), bytes);
        }
        Ok(result)
    }

    pub(super) fn snapshot(
        &self,
        statuses: Vec<CollectionSourceStatus>,
    ) -> CommandResult<CollectionSnapshot> {
        let file = self
            .value
            .lock()
            .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
        Ok(CollectionSnapshot {
            folders: file.folders.clone(),
            sources: statuses,
        })
    }

    pub(crate) fn create(&self, name: &str, creator: &str) -> CommandResult<CollectionFolder> {
        let name = validate_name(name)?;
        self.update(|file| {
            let now = Utc::now().to_rfc3339();
            let folder = CollectionFolder {
                id: Uuid::new_v4().to_string(),
                name,
                creator: creator.trim().to_string(),
                created_at: now.clone(),
                updated_at: now,
                source: CollectionSource::Opp,
                read_only: false,
                pending_write: true,
                stable_sync: true,
                pool: None,
                entries: Vec::new(),
                external_id: None,
                external_fingerprint: None,
                last_read_at: None,
                backup_path: None,
                backup_fingerprint: None,
                backup_confirmed_at: None,
            };
            file.folders.push(folder.clone());
            Ok(folder)
        })
    }

    pub(super) fn rename(&self, folder_id: &str, name: &str) -> CommandResult<()> {
        let name = validate_name(name)?;
        self.edit_folder(folder_id, |folder| {
            ensure_writable(folder)?;
            folder.name = name;
            touch(folder);
            Ok(())
        })
    }

    pub(crate) fn delete(&self, folder_id: &str) -> CommandResult<()> {
        self.update(|file| {
            let index = file
                .folders
                .iter()
                .position(|folder| folder.id == folder_id)
                .ok_or_else(|| CommandError::new("COLLECTION_NOT_FOUND", "未找到收藏夹"))?;
            ensure_writable(&file.folders[index])?;
            file.folders.remove(index);
            Ok(())
        })?;
        self.notebooks.delete(folder_id)
    }

    pub(crate) fn add_entries(
        &self,
        folder_id: &str,
        candidates: Vec<CollectionCandidate>,
    ) -> CommandResult<()> {
        self.edit_folder(folder_id, |folder| {
            ensure_writable(folder)?;
            for candidate in candidates {
                let entry = candidate_to_entry(candidate);
                if !folder
                    .entries
                    .iter()
                    .any(|current| same_entry(current, &entry))
                {
                    folder.entries.push(entry);
                }
            }
            touch(folder);
            Ok(())
        })
    }

    pub(crate) fn pool_snapshot(
        &self,
        folder_id: &str,
    ) -> CommandResult<Option<super::notebook::PoolSnapshot>> {
        let folder = self.folder(folder_id)?;
        match folder.pool {
            Some(pool) => Ok(Some(pool)),
            None => Ok(self.notebooks.get(folder_id)?.pool),
        }
    }

    /// Update display metadata independently of game membership and personal records.
    pub(crate) fn save_pool_metadata(
        &self,
        folder_id: &str,
        maps: &HashMap<i32, serde_json::Value>,
    ) -> CommandResult<usize> {
        let span = crate::infrastructure::logging::global()
            .map(|logger| logger.operation("collections", "save_pool_metadata"));
        let legacy = self.pool_snapshot(folder_id)?;
        let result = self.edit_folder(folder_id, |folder| {
            if folder.pool.is_none() {
                folder.pool = legacy;
            }
            let Some(pool) = &mut folder.pool else {
                return Ok(0);
            };
            let mut updated = HashSet::new();
            for entry in &mut folder.entries {
                let Some(id) = entry.beatmap_id else {
                    continue;
                };
                let Some(map) = maps.get(&id) else {
                    continue;
                };
                // Recheck current membership after the asynchronous fetch; never restore removed maps.
                for slot in pool
                    .slots
                    .iter_mut()
                    .filter(|s| s.beatmap_id == id && s.metadata.is_none())
                {
                    slot.metadata = Some(map.clone());
                    updated.insert(id);
                }
                if !updated.contains(&id) {
                    continue;
                }
                let set = &map["beatmapset"];
                entry.beatmapset_id = map["beatmapset_id"]
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok());
                for (field, value) in [
                    (&mut entry.title, &set["title"]),
                    (&mut entry.artist, &set["artist"]),
                    (&mut entry.creator, &set["creator"]),
                    (&mut entry.difficulty_name, &map["version"]),
                ] {
                    if let Some(text) = value.as_str() {
                        *field = text.to_owned();
                    }
                }
            }
            // Display metadata is OPP-only: preserve hashes, records, membership and pending-write state.
            Ok(updated.len())
        });
        crate::infrastructure::logging::finish_span(span, result)
    }

    /// Replace one linked OPP folder in a single collection update, keyed by source rather than name.
    pub(crate) fn replace_tournament_pool(
        &self,
        source_id: &str,
        name: &str,
        candidates: Vec<CollectionCandidate>,
        pool: Option<super::notebook::PoolSnapshot>,
    ) -> CommandResult<CollectionFolder> {
        let span = crate::infrastructure::logging::global()
            .map(|logger| logger.operation("collections", "replace_tournament_pool"));
        let result = (|| {
            let name = validate_name(name)?;
            if candidates.is_empty()
                || candidates
                    .iter()
                    .any(|entry| entry.beatmap_id.is_none_or(|id| id <= 0))
            {
                return Err(CommandError::new(
                    "EMPTY_OR_INVALID_TOURNAMENT_POOL",
                    "空图池或无效谱面不会覆盖收藏夹",
                ));
            }
            let mut ids = HashSet::new();
            let entries: Vec<_> = candidates
                .into_iter()
                .filter(|entry| ids.insert(entry.beatmap_id))
                .map(candidate_to_entry)
                .collect();
            self.update(|file| {
                if let Some(folder) = file.folders.iter_mut().find(|folder| {
                    folder.source == CollectionSource::Opp
                        && folder.external_id.as_deref() == Some(source_id)
                }) {
                    ensure_writable(folder)?;
                    let mut entries = entries;
                    for entry in &mut entries {
                        if let Some(old) = folder.entries.iter().find(|old| same_entry(old, entry))
                        {
                            entry.id.clone_from(&old.id);
                            // An unresolved refresh must not discard previously resolved local metadata.
                            if entry.beatmapset_id.is_none() {
                                let id = entry.id.clone();
                                *entry = old.clone();
                                entry.id = id;
                            } else if entry.checksum == old.checksum {
                                entry.resolved = old.resolved;
                            }
                        }
                    }
                    let mut pool = pool;
                    if let (Some(next), Some(previous)) = (&mut pool, &folder.pool) {
                        for slot in &mut next.slots {
                            if slot.metadata.is_none() {
                                slot.metadata = previous
                                    .slots
                                    .iter()
                                    .find(|old| old.beatmap_id == slot.beatmap_id)
                                    .and_then(|old| old.metadata.clone());
                            }
                        }
                    }
                    folder.entries = entries;
                    folder.pool = pool;
                    touch(folder);
                    return Ok(folder.clone());
                }
                let now = Utc::now().to_rfc3339();
                let folder = CollectionFolder {
                    id: Uuid::new_v4().to_string(),
                    name,
                    creator: pool
                        .as_ref()
                        .and_then(|p| p.info.tournament.clone())
                        .unwrap_or_else(|| "OPP URI".into()),
                    created_at: now.clone(),
                    updated_at: now,
                    source: CollectionSource::Opp,
                    read_only: false,
                    pending_write: false,
                    stable_sync: false,
                    pool,
                    entries,
                    external_id: Some(source_id.into()),
                    external_fingerprint: None,
                    last_read_at: None,
                    backup_path: None,
                    backup_fingerprint: None,
                    backup_confirmed_at: None,
                };
                file.folders.push(folder.clone());
                Ok(folder)
            })
        })();
        if let Some(span) = &span {
            span.fs_op("sync_tournament_folder", &self.folders_path, &result);
        }
        crate::infrastructure::logging::finish_span(span, result)
    }

    pub(super) fn remove_entry(&self, folder_id: &str, entry_id: &str) -> CommandResult<()> {
        self.edit_folder(folder_id, |folder| {
            ensure_writable(folder)?;
            folder.entries.retain(|entry| entry.id != entry_id);
            touch(folder);
            Ok(())
        })
    }

    pub(crate) fn linked_pool(&self, source_id: &str) -> CommandResult<Option<CollectionFolder>> {
        Ok(self
            .value
            .lock()?
            .folders
            .iter()
            .find(|f| {
                f.source == CollectionSource::Opp && f.external_id.as_deref() == Some(source_id)
            })
            .cloned())
    }

    pub(crate) fn enable_stable_sync(&self, folder_id: &str) -> CommandResult<()> {
        self.edit_folder(folder_id, |folder| {
            ensure_writable(folder)?;
            folder.stable_sync = true;
            touch(folder);
            Ok(())
        })
    }

    pub(crate) fn folder(&self, folder_id: &str) -> CommandResult<CollectionFolder> {
        self.value
            .lock()
            .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
            .cloned()
            .ok_or_else(|| CommandError::new("COLLECTION_NOT_FOUND", "未找到收藏夹"))
    }
}

fn folder_storage_key(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}

fn prune_presence_cache(cache: &mut HashMap<String, LocalPresenceCacheEntry>) {
    // 本地谱面存在性可由下次扫描重建，采用时间顺序淘汰并限制内存占用。
    let mut entries = cache
        .iter()
        .map(|(key, value)| {
            let bytes = serde_json::to_vec(&(key, value)).map_or(0, |bytes| bytes.len());
            (key.clone(), bytes)
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut total_bytes = entries.iter().map(|(_, bytes)| *bytes).sum::<usize>();
    let mut total_entries = entries.len();
    for (key, bytes) in entries {
        if total_entries <= MAX_PRESENCE_CACHE_ENTRIES && total_bytes <= MAX_PRESENCE_CACHE_BYTES {
            break;
        }
        cache.remove(&key);
        total_entries = total_entries.saturating_sub(1);
        total_bytes = total_bytes.saturating_sub(bytes);
    }
}

fn validate_name(name: &str) -> CommandResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(CommandError::new(
            "INVALID_COLLECTION_NAME",
            "收藏夹名称需为 1 到 120 个字符",
        ));
    }
    Ok(name.to_string())
}

fn folder_mut<'a>(
    file: &'a mut CollectionFile,
    folder_id: &str,
) -> CommandResult<&'a mut CollectionFolder> {
    file.folders
        .iter_mut()
        .find(|folder| folder.id == folder_id)
        .ok_or_else(|| CommandError::new("COLLECTION_NOT_FOUND", "未找到收藏夹"))
}

fn ensure_writable(folder: &CollectionFolder) -> CommandResult<()> {
    if folder.read_only {
        Err(CommandError::new(
            "COLLECTION_READ_ONLY",
            "该收藏夹来自只读的 osu!lazer 数据",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn touch(folder: &mut CollectionFolder) {
    folder.updated_at = Utc::now().to_rfc3339();
    if folder.participates_in_stable() {
        folder.pending_write = true;
    }
}

pub(super) fn candidate_to_entry(candidate: CollectionCandidate) -> CollectionEntry {
    // Online API responses may contain an official checksum even though the
    // beatmap is not installed locally. Only local candidates are resolved.
    let resolved = candidate.local_client.is_some() && candidate.checksum.is_some();
    CollectionEntry {
        id: Uuid::new_v4().to_string(),
        beatmap_id: candidate.beatmap_id,
        beatmapset_id: candidate.beatmapset_id,
        checksum: candidate.checksum.map(|value| value.to_ascii_lowercase()),
        ruleset: candidate.ruleset,
        difficulty_name: candidate.difficulty_name,
        title: candidate.title,
        artist: candidate.artist,
        creator: candidate.creator,
        // Online entries do not have a local checksum until osu! imports them.
        resolved,
    }
}

fn same_entry(left: &CollectionEntry, right: &CollectionEntry) -> bool {
    match (left.beatmap_id, right.beatmap_id) {
        (Some(a), Some(b)) => a == b,
        _ => left
            .checksum
            .as_deref()
            .zip(right.checksum.as_deref())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b)),
    }
}

pub(super) fn atomic_replace(temporary: &Path, target: &Path) -> std::io::Result<()> {
    // 先用临时文件完整落盘，再替换目标，防止意外退出破坏收藏夹分片。
    if target.exists() {
        let backup = target.with_extension("bak");
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(target, &backup)?;
        match fs::rename(temporary, target) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(backup, target);
                Err(error)
            }
        }
    } else {
        fs::rename(temporary, target)
    }
}
