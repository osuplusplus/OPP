use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{CacheRecord, PersistedState},
    error::{CommandError, CommandResult},
};

const MAX_CACHE_ENTRIES: usize = 500;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_REFRESH_ENTRIES: usize = 1_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedCache {
    #[serde(default)]
    cache: BTreeMap<String, CacheRecord>,
    #[serde(default)]
    last_manual_refresh: BTreeMap<String, DateTime<Utc>>,
}

#[derive(Default)]
struct PersistedBytes {
    state: Vec<u8>,
    cache: Vec<u8>,
}

pub struct StateStore {
    path: PathBuf,
    cache_path: PathBuf,
    value: Mutex<PersistedState>,
    persist: Mutex<PersistedBytes>,
}

impl StateStore {
    /// 从应用数据目录恢复状态和可再生缓存，并在载入后立刻执行容量淘汰。
    ///
    /// 状态文件损坏时回退到默认值，缓存文件损坏时只丢弃缓存，避免一个损坏的
    /// 优化数据文件阻止应用启动。
    pub fn load(app_data_dir: &Path) -> CommandResult<Self> {
        fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("state.json");
        let mut value = if path.exists() {
            let source = fs::read_to_string(&path)?;
            serde_json::from_str(&source).unwrap_or_default()
        } else {
            PersistedState::default()
        };
        let cache_path = app_data_dir.join("cache.json");
        if let Ok(bytes) = fs::read(&cache_path)
            && let Ok(cache) = serde_json::from_slice::<PersistedCache>(&bytes)
        {
            value.cache = cache.cache;
            value.last_manual_refresh = cache.last_manual_refresh;
        }
        prune_cache(&mut value);

        Ok(Self {
            path,
            cache_path,
            value: Mutex::new(value),
            persist: Mutex::new(PersistedBytes::default()),
        })
    }

    /// 返回一致的内存快照，不会触发磁盘写入。
    pub fn snapshot(&self) -> CommandResult<PersistedState> {
        self.value
            .lock()
            .map(|state| state.clone())
            .map_err(|_| CommandError::new("STATE_ERROR", "本地状态锁已损坏"))
    }

    /// 在同一持久化临界区中修改状态、执行缓存淘汰并按需落盘。
    ///
    /// 先取得持久化锁再取得状态锁，使并发更新不能因交错写入而覆盖彼此。
    pub fn update<R>(&self, operation: impl FnOnce(&mut PersistedState) -> R) -> CommandResult<R> {
        let mut persisted = self
            .persist
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "本地状态持久化锁已损坏"))?;
        let (result, state) = {
            let mut state = self
                .value
                .lock()
                .map_err(|_| CommandError::new("STATE_ERROR", "本地状态锁已损坏"))?;
            let result = operation(&mut state);
            prune_cache(&mut state);
            (result, state.clone())
        };
        self.persist(&state, &mut persisted)?;
        Ok(result)
    }

    /// 将主状态与高频缓存分别序列化，仅在字节实际变更时执行原子替换写入。
    fn persist(&self, state: &PersistedState, previous: &mut PersistedBytes) -> CommandResult<()> {
        let cache = PersistedCache {
            cache: state.cache.clone(),
            last_manual_refresh: state.last_manual_refresh.clone(),
        };
        let mut durable = state.clone();
        durable.cache.clear();
        durable.last_manual_refresh.clear();
        let state_bytes = serde_json::to_vec_pretty(&durable)?;
        let cache_bytes = serde_json::to_vec(&cache)?;
        if previous.state != state_bytes {
            atomic_write(&self.path, &state_bytes)?;
            previous.state = state_bytes;
        }
        if previous.cache != cache_bytes {
            atomic_write(&self.cache_path, &cache_bytes)?;
            previous.cache = cache_bytes;
        }
        Ok(())
    }
}

fn prune_cache(state: &mut PersistedState) {
    // 缓存按抓取时间淘汰，同时受条目数和序列化后体积两项上限约束。
    let mut oldest = state
        .cache
        .iter()
        .map(|(key, record)| {
            let bytes = serde_json::to_vec(&(key, record)).map_or(0, |bytes| bytes.len());
            (key.clone(), record.fetched_at, bytes)
        })
        .collect::<Vec<_>>();
    oldest.sort_by_key(|(_, fetched_at, _)| *fetched_at);
    let mut total_bytes = oldest.iter().map(|(_, _, bytes)| *bytes).sum::<usize>();
    let mut total_entries = oldest.len();
    for (key, _, bytes) in oldest {
        if total_entries <= MAX_CACHE_ENTRIES && total_bytes <= MAX_CACHE_BYTES {
            break;
        }
        state.cache.remove(&key);
        total_entries = total_entries.saturating_sub(1);
        total_bytes = total_bytes.saturating_sub(bytes);
    }
    if state.last_manual_refresh.len() > MAX_REFRESH_ENTRIES {
        let mut oldest = state
            .last_manual_refresh
            .iter()
            .map(|(key, value)| (key.clone(), *value))
            .collect::<Vec<_>>();
        oldest.sort_by_key(|(_, value)| *value);
        for (key, _) in oldest
            .into_iter()
            .take(state.last_manual_refresh.len() - MAX_REFRESH_ENTRIES)
        {
            state.last_manual_refresh.remove(&key);
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> CommandResult<()> {
    // 保留上一份完整文件直到新文件改名成功，避免崩溃留下半写入的 JSON。
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    let backup = path.with_extension("json.bak");
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(path, &backup)?;
    }
    match fs::rename(&temporary, path) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            if backup.exists() {
                let _ = fs::rename(backup, path);
            }
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CacheRecord;
    use serde_json::json;

    #[test]
    fn state_round_trips() {
        let directory = tempfile::tempdir().expect("temp directory");
        let store = StateStore::load(directory.path()).expect("create store");
        store
            .update(|state| state.client_id = Some("42".into()))
            .expect("save state");

        let reloaded = StateStore::load(directory.path()).expect("reload store");
        assert_eq!(
            reloaded.snapshot().expect("snapshot").client_id.as_deref(),
            Some("42")
        );
    }

    #[test]
    fn cache_is_bounded_and_persisted_separately() {
        let directory = tempfile::tempdir().expect("temp directory");
        let store = StateStore::load(directory.path()).expect("create store");
        store
            .update(|state| {
                for index in 0..(MAX_CACHE_ENTRIES + 25) {
                    state.cache.insert(
                        format!("score-{index}"),
                        CacheRecord {
                            value: json!({"index": index}),
                            fetched_at: Utc::now() + chrono::Duration::seconds(index as i64),
                        },
                    );
                }
            })
            .expect("persist cache");
        assert_eq!(
            store.snapshot().expect("snapshot").cache.len(),
            MAX_CACHE_ENTRIES
        );
        let durable = fs::read_to_string(directory.path().join("state.json")).expect("state file");
        assert!(!durable.contains("score-"));
        assert!(directory.path().join("cache.json").is_file());
    }
}
