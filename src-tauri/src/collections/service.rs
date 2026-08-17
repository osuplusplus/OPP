use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

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
    path: PathBuf,
    cache_path: PathBuf,
    folders_path: PathBuf,
    pub(super) value: Mutex<CollectionFile>,
    persist: Mutex<CollectionPersistedBytes>,
}

impl CollectionService {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        // 收藏夹按文件夹拆分持久化，单个文件损坏不会使整个集合不可恢复。
        fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("collections.json");
        let collection_bytes = fs::read(&path).unwrap_or_default();
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
        let mut persisted_folders = HashMap::new();
        let mut sharded_folders = HashMap::new();
        for entry in fs::read_dir(&folders_path)?.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(folder) = serde_json::from_slice::<CollectionFolder>(&bytes) else {
                continue;
            };
            persisted_folders.insert(folder.id.clone(), bytes);
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
        Ok(Self {
            path,
            cache_path,
            folders_path,
            value: Mutex::new(value),
            persist: Mutex::new(CollectionPersistedBytes {
                collections: collection_bytes,
                cache: cache_bytes,
                folders: persisted_folders,
            }),
        })
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
        let (result, file) = {
            let mut file = self
                .value
                .lock()
                .map_err(|_| CommandError::new("COLLECTION_STATE_ERROR", "收藏夹状态不可用"))?;
            let result = action(&mut file)?;
            prune_presence_cache(&mut file.local_presence_cache);
            (result, file.clone())
        };
        let cache = CollectionCacheFile {
            local_presence_cache: file.local_presence_cache.clone(),
        };
        let folders = file.folders.clone();
        let mut durable = file;
        durable.sharded = true;
        durable.folder_order = folders.iter().map(|folder| folder.id.clone()).collect();
        durable.folders.clear();
        durable.local_presence_cache.clear();
        let collection_bytes = serde_json::to_vec_pretty(&durable)?;
        let cache_bytes = serde_json::to_vec(&cache)?;
        if persisted.cache != cache_bytes {
            let temporary = self.cache_path.with_extension("json.tmp");
            fs::write(&temporary, &cache_bytes)?;
            atomic_replace(&temporary, &self.cache_path)?;
            persisted.cache = cache_bytes;
        }
        let mut current_folder_ids = HashSet::new();
        for folder in folders {
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
            persisted.folders.insert(folder.id, bytes);
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

    pub(super) fn create(&self, name: &str, creator: &str) -> CommandResult<CollectionFolder> {
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
                entries: Vec::new(),
            };
            file.folders.push(folder.clone());
            Ok(folder)
        })
    }

    pub(super) fn rename(&self, folder_id: &str, name: &str) -> CommandResult<()> {
        let name = validate_name(name)?;
        self.update(|file| {
            let folder = folder_mut(file, folder_id)?;
            ensure_writable(folder)?;
            folder.name = name;
            touch(folder);
            Ok(())
        })
    }

    pub(super) fn delete(&self, folder_id: &str) -> CommandResult<()> {
        self.update(|file| {
            let index = file
                .folders
                .iter()
                .position(|folder| folder.id == folder_id)
                .ok_or_else(|| CommandError::new("COLLECTION_NOT_FOUND", "未找到收藏夹"))?;
            ensure_writable(&file.folders[index])?;
            file.folders.remove(index);
            Ok(())
        })
    }

    pub(super) fn add_entries(
        &self,
        folder_id: &str,
        candidates: Vec<CollectionCandidate>,
    ) -> CommandResult<()> {
        self.update(|file| {
            let folder = folder_mut(file, folder_id)?;
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

    pub(super) fn remove_entry(&self, folder_id: &str, entry_id: &str) -> CommandResult<()> {
        self.update(|file| {
            let folder = folder_mut(file, folder_id)?;
            ensure_writable(folder)?;
            folder.entries.retain(|entry| entry.id != entry_id);
            touch(folder);
            Ok(())
        })
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
    if folder.source != CollectionSource::Lazer {
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

fn atomic_replace(temporary: &Path, target: &Path) -> std::io::Result<()> {
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
