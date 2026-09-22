//! OPP-only metadata, kept separate from game collection membership.
use super::CollectionEntry;
use crate::{
    error::{CommandError, CommandResult},
    features::local_scores::LocalScore,
    infrastructure::logging::{finish_span, global},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CollectionTag {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PersonalRecord {
    pub revision: u64,
    pub tags: Vec<CollectionTag>,
    pub note: String,
    pub slot_override: Option<String>,
    pub scores: Vec<LocalScore>,
    pub representative: Option<LocalScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSlot {
    pub beatmap_id: i32,
    pub label: String,
    pub selected_by: String,
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub reference: crate::features::tournament_pools::TournamentPoolRef,
    pub slots: Vec<PoolSlot>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Notebook {
    version: u32,
    #[serde(default)]
    pub records: HashMap<String, PersonalRecord>,
    #[serde(default, skip_serializing)]
    pub pool: Option<PoolSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct StoredPool {
    version: u32,
    pool: PoolSnapshot,
}
impl Default for Notebook {
    fn default() -> Self {
        Self {
            version: 1,
            records: HashMap::new(),
            pool: None,
        }
    }
}

fn keys(entry: &CollectionEntry) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(id) = entry.beatmap_id.filter(|id| *id > 0) {
        keys.push(format!("bid:{id}"));
    }
    if let Some(hash) = entry.checksum.as_ref().filter(|h| !h.is_empty()) {
        keys.push(format!("md5:{}", hash.to_lowercase()));
    }
    keys.push(format!("entry:{}", entry.id));
    keys
}
impl Notebook {
    pub fn record(&self, entry: &CollectionEntry) -> PersonalRecord {
        keys(entry)
            .iter()
            .find_map(|key| self.records.get(key))
            .cloned()
            .unwrap_or_default()
    }
}

pub struct NotebookStore {
    root: PathBuf,
    cache: Mutex<HashMap<String, Notebook>>,
}
impl NotebookStore {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.join("collection-notes"),
            cache: Mutex::new(HashMap::new()),
        }
    }
    fn path(&self, folder: &str) -> PathBuf {
        self.root
            .join(format!("{:x}.json", Sha256::digest(folder.as_bytes())))
    }
    fn pool_path(&self, folder: &str) -> PathBuf {
        self.root
            .with_file_name("collection-pools")
            .join(format!("{:x}.json", Sha256::digest(folder.as_bytes())))
    }
    fn load(&self, folder: &str) -> CommandResult<Notebook> {
        let span = global().map(|log| log.operation("collections", "read_notebook"));
        let path = self.path(folder);
        let read = fs::read(&path);
        if let Some(log) = &span {
            log.fs_op("read", &path, &read);
        }
        let result = match read {
            Ok(bytes) => serde_json::from_slice::<Notebook>(&bytes)
                .map_err(CommandError::from)
                .and_then(|value| {
                    if value.version != 1 {
                        Err(CommandError::new(
                            "NOTEBOOK_VERSION_UNSUPPORTED",
                            "收藏记录版本不受支持，原文件已保留",
                        ))
                    } else {
                        Ok(value)
                    }
                }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Notebook::default()),
            Err(error) => Err(error.into()),
        };
        let result = result.and_then(|mut notebook| {
            let path = self.pool_path(folder);
            let read = fs::read(&path);
            if let Some(log) = &span {
                log.fs_op("read_pool", &path, &read);
            }
            match read {
                Ok(bytes) => {
                    let stored: StoredPool = serde_json::from_slice(&bytes)?;
                    if stored.version != 1 {
                        return Err(CommandError::new(
                            "POOL_VERSION_UNSUPPORTED",
                            "比赛快照版本不受支持，原文件已保留",
                        ));
                    }
                    notebook.pool = Some(stored.pool);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            Ok(notebook)
        });
        finish_span(span, result)
    }
    pub(super) fn get(&self, folder: &str) -> CommandResult<Notebook> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "数据状态不可用"))?;
        if !cache.contains_key(folder) {
            cache.insert(folder.into(), self.load(folder)?);
        }
        Ok(cache[folder].clone())
    }
    fn update(
        &self,
        folder: &str,
        action: impl FnOnce(&mut Notebook) -> CommandResult<()>,
    ) -> CommandResult<()> {
        let span = global().map(|log| log.operation("collections", "save_notebook"));
        let result = (|| {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "数据状态不可用"))?;
            let mut value = match cache.get(folder) {
                Some(value) => value.clone(),
                None => self.load(folder)?,
            };
            action(&mut value)?;
            fs::create_dir_all(&self.root)?;
            let path = self.path(folder);
            let temporary = path.with_extension("json.tmp");
            let write = fs::write(&temporary, serde_json::to_vec(&value)?)
                .map_err(CommandError::from)
                .and_then(|()| {
                    super::service::atomic_replace(&temporary, &path).map_err(CommandError::from)
                });
            if let Some(log) = &span {
                log.fs_op("atomic_write", &path, &write);
            }
            write?;
            cache.insert(folder.into(), value);
            Ok(())
        })();
        finish_span(span, result)
    }
    pub fn save(
        &self,
        folder: &str,
        entry: &CollectionEntry,
        mut record: PersonalRecord,
    ) -> CommandResult<PersonalRecord> {
        validate(&record)?;
        self.update(folder, |notebook| {
            if notebook.record(entry).revision != record.revision {
                return Err(CommandError::new(
                    "NOTEBOOK_CONFLICT",
                    "记录已在其他窗口更新，请重新载入后保存",
                ));
            }
            record.revision += 1;
            let aliases = keys(entry);
            for alias in &aliases {
                notebook.records.remove(alias);
            }
            notebook.records.insert(aliases[0].clone(), record.clone());
            Ok(())
        })?;
        Ok(record)
    }
    pub fn save_pool(&self, folder: &str, pool: PoolSnapshot) -> CommandResult<()> {
        let span = global().map(|log| log.operation("collections", "save_pool_snapshot"));
        let result = (|| {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "数据状态不可用"))?;
            let mut value = match cache.get(folder) {
                Some(value) => value.clone(),
                None => self.load(folder)?,
            };
            let path = self.pool_path(folder);
            let directory = path.parent().expect("pool path has a parent");
            let created = fs::create_dir_all(directory);
            if let Some(log) = &span {
                log.fs_op("create_directory", directory, &created);
            }
            created?;
            let temporary = path.with_extension("json.tmp");
            let stored = StoredPool { version: 1, pool };
            let write = fs::write(&temporary, serde_json::to_vec(&stored)?)
                .and_then(|()| super::service::atomic_replace(&temporary, &path));
            if let Some(log) = &span {
                log.fs_op("atomic_write", &path, &write);
            }
            write?;
            value.pool = Some(stored.pool);
            cache.insert(folder.into(), value);
            Ok(())
        })();
        finish_span(span, result)
    }
    pub fn delete(&self, folder: &str) -> CommandResult<()> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "数据状态不可用"))?;
        for path in [self.path(folder), self.pool_path(folder)] {
            let result = match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(CommandError::from(e)),
            };
            if let Some(log) = global() {
                log.operation("collections", "delete_notebook")
                    .fs_op("delete", &path, &result);
            }
            result?;
        }
        cache.remove(folder);
        Ok(())
    }
}
fn validate(record: &PersonalRecord) -> CommandResult<()> {
    let bad = || CommandError::new("INVALID_COLLECTION_RECORD", "请检查标签、笔记或成绩格式");
    if record.note.len() > 100_000
        || record.tags.len() > 32
        || record.scores.len() > 10_000
        || record.slot_override.as_ref().is_some_and(|s| s.len() > 64)
    {
        return Err(bad());
    }
    for tag in &record.tags {
        if tag.name.trim().is_empty()
            || tag.name.len() > 100
            || tag.color.len() != 7
            || !tag.color.starts_with('#')
            || !tag.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(bad());
        }
    }
    for score in record.scores.iter().chain(record.representative.iter()) {
        if score.score > 9_007_199_254_740_991
            || score
                .accuracy
                .is_some_and(|a| !a.is_finite() || !(0.0..=1.0).contains(&a))
            || score.note.len() > 10_000
            || score.mods.len() > 4096
            || score.id.is_empty()
            || score
                .played_at
                .as_ref()
                .is_some_and(|d| chrono::DateTime::parse_from_rfc3339(d).is_err())
        {
            return Err(bad());
        }
    }
    if record.scores.iter().any(|s| s.source != "manual") {
        return Err(bad());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> CollectionEntry {
        CollectionEntry {
            id: "first".into(),
            beatmap_id: Some(42),
            beatmapset_id: None,
            checksum: None,
            ruleset: Some("osu".into()),
            difficulty_name: "test".into(),
            title: "song".into(),
            artist: String::new(),
            creator: String::new(),
            resolved: false,
        }
    }
    #[test]
    fn records_survive_restart_identity_changes_and_are_folder_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(dir.path());
        let mut map = entry();
        let record = PersonalRecord {
            note: "这里容易失误".into(),
            ..Default::default()
        };
        store.save("a", &map, record.clone()).unwrap();
        assert!(store.save("a", &map, record).is_err());
        map.id = "new-id".into();
        map.checksum = Some("filled".into());
        let reopened = NotebookStore::new(dir.path());
        assert_eq!(reopened.get("a").unwrap().record(&map).note, "这里容易失误");
        assert!(reopened.get("b").unwrap().record(&map).note.is_empty());
        reopened.delete("a").unwrap();
        assert!(reopened.get("a").unwrap().record(&map).note.is_empty());
    }
    #[test]
    fn pool_sync_writes_a_separate_snapshot_without_rewriting_notes() {
        let dir = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(dir.path());
        let map = entry();
        store
            .save(
                "a",
                &map,
                PersonalRecord {
                    note: "保留练习笔记".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let before = fs::read(store.path("a")).unwrap();
        store
            .save_pool(
                "a",
                PoolSnapshot {
                    reference: crate::features::tournament_pools::TournamentPoolRef {
                        provider: "rino".into(),
                        season: "s1".into(),
                        category: "finals".into(),
                    },
                    slots: vec![PoolSlot {
                        beatmap_id: 42,
                        label: "NM2".into(),
                        selected_by: "Player".into(),
                        comment: "比赛备注".into(),
                    }],
                },
            )
            .unwrap();
        assert_eq!(fs::read(store.path("a")).unwrap(), before);
        assert!(!String::from_utf8(before).unwrap().contains("\"pool\""));
        let reopened = NotebookStore::new(dir.path());
        let notebook = reopened.get("a").unwrap();
        assert_eq!(notebook.record(&map).note, "保留练习笔记");
        assert_eq!(notebook.pool.unwrap().slots[0].label, "NM2");
        reopened.delete("a").unwrap();
        assert!(!reopened.pool_path("a").exists());
    }
    #[test]
    fn malformed_notebook_is_not_silently_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(dir.path());
        fs::create_dir_all(&store.root).unwrap();
        fs::write(store.path("a"), b"broken").unwrap();
        assert!(
            store
                .save("a", &entry(), PersonalRecord::default())
                .is_err()
        );
        assert_eq!(fs::read(store.path("a")).unwrap(), b"broken");
    }
}
