use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{AppSettings, CacheRecord, PersistedState},
    error::{CommandError, CommandResult},
};

/// 缓存条目数上限：超出后按抓取时间从最旧开始淘汰。
const MAX_CACHE_ENTRIES: usize = 500;
/// 缓存序列化后的总体积上限（64 MiB），防止 cache.json 无限膨胀。
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
/// 手动刷新时间记录的条目上限。
const MAX_REFRESH_ENTRIES: usize = 1_000;

/// cache.json 侧车文件的磁盘格式：只保存可再生的响应缓存与手动刷新时间。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedCache {
    /// 缓存键到记录的映射。
    #[serde(default)]
    cache: BTreeMap<String, CacheRecord>,
    /// 各缓存键最近一次手动刷新的时间。
    #[serde(default)]
    last_manual_refresh: BTreeMap<String, DateTime<Utc>>,
}

/// 持久化层的脏检查状态：缓存最近一次成功写盘的字节，内容未变化时跳过 I/O。
#[derive(Default)]
struct PersistedBytes {
    /// cache.json 是否已完成首次共享加载。
    cache_loaded: bool,
    /// 旧版 state.json 内联缓存是否尚未迁移到侧车文件。
    legacy_inline_cache: bool,
    /// 各缓存条目序列化后的大小，供增量淘汰复用，避免重复序列化。
    cache_sizes: BTreeMap<String, usize>,
    /// 最近一次写入 state.json 的字节快照。
    state: Vec<u8>,
    /// 最近一次写入 cache.json 的字节快照。
    cache: Vec<u8>,
}

/// 本地状态仓库：主状态（state.json）与可再生缓存（cache.json）分文件存储，
/// 写盘前做字节级脏检查，全部写入走“临时文件 + 原子改名”。
pub struct StateStore {
    /// 主状态文件路径。
    path: PathBuf,
    /// 缓存侧车文件路径。
    cache_path: PathBuf,
    /// 内存中的完整状态（含尚未加载或落盘的缓存）。
    value: Mutex<PersistedState>,
    /// 持久化脏检查与迁移标记。
    persist: Mutex<PersistedBytes>,
}

impl StateStore {
    /// 启动时只恢复必要状态；可再生缓存首次使用时共享加载并执行容量淘汰。
    ///
    /// 状态文件损坏时回退到默认值，缓存文件损坏时只丢弃缓存，避免一个损坏的
    /// 优化数据文件阻止应用启动。
    pub fn load(app_data_dir: &Path) -> CommandResult<Self> {
        fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("state.json");
        let value = if path.exists() {
            let source = fs::read_to_string(&path)?;
            serde_json::from_str(&source).unwrap_or_default()
        } else {
            PersistedState::default()
        };
        let cache_path = app_data_dir.join("cache.json");

        let legacy_inline_cache = !value.cache.is_empty() || !value.last_manual_refresh.is_empty();
        Ok(Self {
            path,
            cache_path,
            value: Mutex::new(value),
            persist: Mutex::new(PersistedBytes {
                legacy_inline_cache,
                ..Default::default()
            }),
        })
    }

    /// 首次访问缓存时共享加载 cache.json 并执行容量淘汰；之后调用直接返回。
    pub(crate) fn load_cache(&self) -> CommandResult<()> {
        let mut persisted = self
            .persist
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "缓存初始化锁已损坏"))?;
        if persisted.cache_loaded {
            return Ok(());
        }
        let bytes = match fs::read(&self.cache_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let cache = bytes
            .as_ref()
            .and_then(|bytes| serde_json::from_slice::<PersistedCache>(bytes).ok());
        let mut state = self
            .value
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "状态锁已损坏"))?;
        if let Some(cache) = cache {
            state.cache = cache.cache;
            state.last_manual_refresh = cache.last_manual_refresh;
        }
        prune_cache(&mut state);
        persisted.cache_sizes = cache_sizes(&state);
        persisted.cache_loaded = true;
        Ok(())
    }

    /// 读取可能依赖缓存的字段：先确保缓存已加载，再取一致的内存快照。
    pub(crate) fn read_cached<R>(
        &self,
        select: impl FnOnce(&PersistedState) -> R,
    ) -> CommandResult<R> {
        self.load_cache()?;
        self.read(select)
    }

    /// Settings writes neither initialize nor serialize the response cache.
    ///
    /// 设置写入既不加载也不序列化响应缓存；写盘失败时回滚内存中的设置，
    /// 保证磁盘与内存视图一致。
    pub(crate) fn update_settings<R>(
        &self,
        operation: impl FnOnce(&mut AppSettings) -> R,
    ) -> CommandResult<R> {
        let mut persisted = self
            .persist
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "持久化锁已损坏"))?;
        let (result, bytes, previous) = {
            let mut state = self
                .value
                .lock()
                .map_err(|_| CommandError::new("STATE_ERROR", "状态锁已损坏"))?;
            let previous = state.settings.clone();
            let result = operation(&mut state.settings);
            // Preserve legacy inline caches until a full write migrates them.
            let bytes = if !persisted.legacy_inline_cache {
                serde_json::to_vec_pretty(&durable_snapshot(&state))?
            } else {
                serde_json::to_vec_pretty(&*state)?
            };
            (result, bytes, previous)
        };
        if persisted.state != bytes {
            if let Err(error) = atomic_write(&self.path, &bytes) {
                self.value
                    .lock()
                    .map_err(|_| CommandError::new("STATE_ERROR", "状态锁已损坏"))?
                    .settings = previous;
                return Err(error);
            }
            persisted.state = bytes;
        }
        Ok(result)
    }

    /// 插入一条缓存记录，在临界区内完成增量容量淘汰；若仍处于旧版内联缓存
    /// 状态，则借这次写盘把缓存迁移到侧车文件。附带身份时顺带更新当前用户。
    pub(crate) fn insert_cache(
        &self,
        key: String,
        record: CacheRecord,
        identity: Option<(u64, String)>,
    ) -> CommandResult<()> {
        self.load_cache()?;
        let record_bytes = serde_json::to_vec(&(&key, &record))?.len();
        let mut persisted = self
            .persist
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "缓存持久化锁已损坏"))?;
        let (state_bytes, cache_bytes) = {
            let mut state = self
                .value
                .lock()
                .map_err(|_| CommandError::new("STATE_ERROR", "状态锁已损坏"))?;
            state.cache.insert(key.clone(), record);
            persisted.cache_sizes.insert(key, record_bytes);
            prune_sized_cache(&mut state, &mut persisted.cache_sizes);
            if let Some((id, name)) = identity {
                state.current_user_id = Some(id);
                state.username = Some(name);
            }
            (
                if persisted.legacy_inline_cache {
                    serde_json::to_vec_pretty(&*state)?
                } else {
                    serde_json::to_vec_pretty(&durable_snapshot(&state))?
                },
                serialize_cache(&state)?,
            )
        };
        // Preserve the existing metadata -> cache order. Legacy inline records
        // remain in metadata until the sidecar has been successfully persisted.
        if persisted.state != state_bytes {
            atomic_write(&self.path, &state_bytes)?;
            persisted.state = state_bytes;
        }
        if persisted.cache != cache_bytes {
            atomic_write(&self.cache_path, &cache_bytes)?;
            persisted.cache = cache_bytes;
        }
        persisted.legacy_inline_cache = false;
        Ok(())
    }

    /// 返回一致的内存快照，不会触发磁盘写入。
    pub fn snapshot(&self) -> CommandResult<PersistedState> {
        self.load_cache()?;
        self.read(Clone::clone)
    }

    /// 只复制调用方需要的字段。闭包必须短小，不执行 I/O 或其他长任务。
    pub fn read<R>(&self, select: impl FnOnce(&PersistedState) -> R) -> CommandResult<R> {
        self.value
            .lock()
            .map(|state| select(&state))
            .map_err(|_| CommandError::new("STATE_ERROR", "本地状态锁已损坏"))
    }

    /// 设置读取不应复制、整理或序列化可能达数十 MB 的响应缓存。
    pub fn settings_snapshot(&self) -> CommandResult<AppSettings> {
        self.read(|state| state.settings.clone())
    }

    /// 在同一持久化临界区中修改状态、执行缓存淘汰并按需落盘。
    ///
    /// 先取得持久化锁再取得状态锁，使并发更新不能因交错写入而覆盖彼此。
    pub fn update<R>(&self, operation: impl FnOnce(&mut PersistedState) -> R) -> CommandResult<R> {
        self.load_cache()?;
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
        persisted.legacy_inline_cache = false;
        Ok(result)
    }

    /// 将主状态与高频缓存分别序列化，仅在字节实际变更时执行原子替换写入。
    fn persist(&self, state: &PersistedState, previous: &mut PersistedBytes) -> CommandResult<()> {
        let durable = durable_snapshot(state);
        let state_bytes = if previous.legacy_inline_cache {
            serde_json::to_vec_pretty(state)?
        } else {
            serde_json::to_vec_pretty(&durable)?
        };
        let cache_bytes = serialize_cache(state)?;
        previous.cache_sizes = cache_sizes(state);
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

/// 序列化缓存侧车内容：紧凑 JSON，只含缓存本体与手动刷新时间两个字段。
fn serialize_cache(state: &PersistedState) -> CommandResult<Vec<u8>> {
    #[derive(Serialize)]
    struct CacheView<'a> {
        cache: &'a BTreeMap<String, CacheRecord>,
        last_manual_refresh: &'a BTreeMap<String, DateTime<Utc>>,
    }
    Ok(serde_json::to_vec(&CacheView {
        cache: &state.cache,
        last_manual_refresh: &state.last_manual_refresh,
    })?)
}
/// 统计每个缓存条目按侧车格式序列化后的大小，供增量淘汰复用。
fn cache_sizes(state: &PersistedState) -> BTreeMap<String, usize> {
    state
        .cache
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                serde_json::to_vec(&(key, value)).map_or(0, |bytes| bytes.len()),
            )
        })
        .collect()
}
/// 增量版容量淘汰：直接复用 `sizes` 中已统计的体积，避免每次插入都把整个
/// 缓存重新序列化一遍。
fn prune_sized_cache(state: &mut PersistedState, sizes: &mut BTreeMap<String, usize>) {
    let mut total: usize = sizes.values().sum();
    let mut oldest = state
        .cache
        .iter()
        .map(|(key, record)| (key.clone(), record.fetched_at))
        .collect::<Vec<_>>();
    oldest.sort_by_key(|(_, fetched)| *fetched);
    for (key, _) in oldest {
        if state.cache.len() <= MAX_CACHE_ENTRIES && total <= MAX_CACHE_BYTES {
            break;
        }
        state.cache.remove(&key);
        total = total.saturating_sub(sizes.remove(&key).unwrap_or(0));
    }
}

/// 构造写入 state.json 的持久化视图：剥离可再生缓存，只保留身份、令牌与
/// 设置，避免几十 MB 的缓存拖慢每次主状态落盘。
fn durable_snapshot(state: &PersistedState) -> PersistedState {
    PersistedState {
        client_id: state.client_id.clone(),
        token_expires_at: state.token_expires_at,
        current_user_id: state.current_user_id,
        username: state.username.clone(),
        settings: state.settings.clone(),
        cache: BTreeMap::new(),
        last_manual_refresh: BTreeMap::new(),
    }
}

/// 全量版容量淘汰：重算每个条目的序列化体积，按抓取时间从旧到新移除，
/// 直到同时满足条目数与总体积上限；手动刷新记录单独按条目数裁剪。
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

/// 原子替换写入：先写临时文件，把旧文件改名为备份，再用临时文件顶替；
/// 顶替失败时用备份恢复原文件，磁盘上始终保留一份完整内容。
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

    // 设置写入不得触发缓存加载或改写 cache.json，且旧版内联数据须保留到迁移。
    #[test]
    fn settings_writes_leave_unloaded_cache_intact_and_preserve_legacy_inline_data() {
        let directory = tempfile::tempdir().unwrap();
        let mut state = PersistedState::default();
        state.cache.insert(
            "legacy".into(),
            CacheRecord {
                value: json!({"keep": true}),
                fetched_at: Utc::now(),
            },
        );
        fs::write(
            directory.path().join("state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let store = StateStore::load(directory.path()).unwrap();
        store
            .update_settings(|settings| settings.onboarding_version = 99)
            .unwrap();
        assert!(!store.persist.lock().unwrap().cache_loaded);
        let reopened = StateStore::load(directory.path()).unwrap();
        assert!(reopened.snapshot().unwrap().cache.contains_key("legacy"));
        reopened.update(|_| ()).unwrap();
        let bytes = fs::read(directory.path().join("cache.json")).unwrap();
        let again = StateStore::load(directory.path()).unwrap();
        again
            .update_settings(|settings| settings.onboarding_version = 100)
            .unwrap();
        assert!(!again.persist.lock().unwrap().cache_loaded);
        assert_eq!(
            fs::read(directory.path().join("cache.json")).unwrap(),
            bytes
        );
        assert!(again.snapshot().unwrap().cache.contains_key("legacy"));
    }

    // 写盘失败须回滚内存设置；增量维护的体积统计须与全量统计保持一致。
    #[test]
    fn failed_settings_write_restores_memory_and_incremental_cache_accounting_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::load(directory.path()).unwrap();
        fs::create_dir(store.path.with_extension("json.tmp")).unwrap();
        let original = store.settings_snapshot().unwrap().onboarding_version;
        assert!(
            store
                .update_settings(|settings| settings.onboarding_version = 123)
                .is_err()
        );
        assert_eq!(
            store.settings_snapshot().unwrap().onboarding_version,
            original
        );
        fs::remove_dir(store.path.with_extension("json.tmp")).unwrap();
        for index in 0..MAX_CACHE_ENTRIES + 1 {
            store
                .insert_cache(
                    index.to_string(),
                    CacheRecord {
                        value: json!({"index": index}),
                        fetched_at: Utc::now(),
                    },
                    None,
                )
                .unwrap();
        }
        assert_eq!(store.snapshot().unwrap().cache.len(), MAX_CACHE_ENTRIES);
        let sizes = store.persist.lock().unwrap().cache_sizes.clone();
        assert_eq!(sizes, cache_sizes(&store.snapshot().unwrap()));
    }

    // 主状态经落盘与重载后应保持一致。
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

    // 纯读取路径不得触发任何持久化写入或缓存淘汰。
    #[test]
    fn settings_read_does_not_persist_or_prune_response_cache() {
        let directory = tempfile::tempdir().expect("temp directory");
        let store = StateStore::load(directory.path()).expect("create store");
        {
            let mut state = store.value.lock().unwrap();
            state.settings.onboarding_version = 42;
            for index in 0..=MAX_CACHE_ENTRIES {
                state.cache.insert(
                    index.to_string(),
                    CacheRecord {
                        value: json!({ "cached": true }),
                        fetched_at: Utc::now(),
                    },
                );
            }
        }
        assert_eq!(store.settings_snapshot().unwrap().onboarding_version, 42);
        assert_eq!(
            store.value.lock().unwrap().cache.len(),
            MAX_CACHE_ENTRIES + 1
        );
        assert!(!store.path.exists());
        assert!(!store.cache_path.exists());
    }

    // 缓存受条目上限约束，且必须与主状态文件分开持久化。
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
