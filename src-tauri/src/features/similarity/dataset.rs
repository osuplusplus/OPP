//! 相似谱面数据集的加载与缓存运行时。
//! 以目录路径为键缓存只读数据集实例；目录变更或清空缓存时通过更换
//! “加载槽位”做代际隔离，在途的旧加载无法把过期数据写回注册表。

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use osu_difficulty_runtime::{Dataset, ManiaDataset, RuntimeError, RuntimeErrorKind};

use crate::{
    domain::Ruleset,
    features::similarity::models::{SimilarityIndexState, SimilarityIndexStatus},
};

/// 一次加载的预留：目标目录加共享的“已加载数据集”槽位。
struct CachedDataset<T> {
    directory: PathBuf,
    dataset: Arc<Mutex<Option<Arc<T>>>>,
}

/// 分模式持有的数据集缓存，随 `AppState` 存活整个应用生命周期。
#[derive(Default)]
pub struct SimilarityRuntime {
    standard: Mutex<Option<CachedDataset<Dataset>>>,
    mania: Mutex<Option<CachedDataset<ManiaDataset>>>,
}

impl SimilarityRuntime {
    /// 预留一个“视图”运行时：与注册表共享当前加载槽位，但自身不持有
    /// 注册表，之后的清空操作不会影响它已拿到的槽位。
    /// Reserve a generation before async waits or worker admission. This view
    /// shares its load slot, but cannot change the application's registry.
    pub fn prepared(&self, ruleset: Ruleset, directory: Option<&str>) -> Self {
        let view = Self::default();
        if let Some(directory) = directory {
            match ruleset {
                Ruleset::Osu => {
                    *view.standard.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(reserve_dataset(&self.standard, directory));
                }
                Ruleset::Mania => {
                    *view.mania.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(reserve_dataset(&self.mania, directory));
                }
                Ruleset::Taiko | Ruleset::Fruits => {}
            }
        }
        view
    }

    /// 清空指定模式的缓存槽位；在途加载仍持有旧槽位，只是无法再发布回来。
    pub fn clear(&self, ruleset: Ruleset) {
        match ruleset {
            Ruleset::Osu => clear_cache(&self.standard),
            Ruleset::Mania => clear_cache(&self.mania),
            Ruleset::Taiko | Ruleset::Fruits => {}
        }
    }

    /// 体检索引目录：区分未配置、目录缺失、数据集无效、不兼容与就绪五种结果。
    pub fn inspect(&self, ruleset: Ruleset, directory: Option<&str>) -> SimilarityIndexStatus {
        if matches!(ruleset, Ruleset::Taiko | Ruleset::Fruits) {
            return SimilarityIndexStatus::unsupported(ruleset);
        }
        // 检查仅描述索引可用性，不在设置页之外持久化任何数据。
        let Some(directory) = directory.map(str::trim).filter(|value| !value.is_empty()) else {
            return SimilarityIndexStatus::unconfigured(ruleset);
        };
        let path = Path::new(directory);
        if !path.is_dir() {
            self.clear(ruleset);
            return unavailable_status(
                ruleset,
                directory,
                SimilarityIndexState::Missing,
                "已配置的本地索引目录不可用，请重新选择。",
            );
        }
        match ruleset {
            Ruleset::Osu => match self.standard_dataset(directory) {
                Ok(dataset) => {
                    let info = dataset.info();
                    SimilarityIndexStatus {
                        ruleset,
                        state: SimilarityIndexState::Ready,
                        directory: Some(directory.into()),
                        message: "本地索引已就绪。".into(),
                        record_count: Some(info.record_count),
                        records_by_key_count: None,
                        analyzer_version: Some(info.analyzer_version),
                        normalization_version: Some(info.normalization_version),
                        algorithm_id: Some(info.algorithm_id.clone()),
                        data_cutoff_at: info.data_cutoff_at,
                        supports_dynamic_weighting: false,
                    }
                }
                Err(error) => status_from_error(ruleset, directory, &error),
            },
            Ruleset::Mania => match self.mania_dataset(directory) {
                Ok(dataset) => {
                    let info = dataset.info();
                    SimilarityIndexStatus {
                        ruleset,
                        state: SimilarityIndexState::Ready,
                        directory: Some(directory.into()),
                        message: "osu!mania 本地索引已就绪。".into(),
                        record_count: Some(info.record_count),
                        records_by_key_count: Some(info.records_by_key_count.clone()),
                        analyzer_version: Some(info.analyzer_version),
                        normalization_version: Some(info.normalization_version),
                        algorithm_id: Some(info.algorithm_id.clone()),
                        data_cutoff_at: info.data_cutoff_at,
                        supports_dynamic_weighting: false,
                    }
                }
                Err(error) => status_from_error(ruleset, directory, &error),
            },
            Ruleset::Taiko | Ruleset::Fruits => unreachable!("handled above"),
        }
    }

    /// 打开（或复用缓存的）standard 数据集；并发调用只会真正打开一次。
    pub fn standard_dataset(&self, directory: &str) -> Result<Arc<Dataset>, RuntimeError> {
        cached_dataset(&self.standard, directory, |path| Dataset::open(path))
    }

    /// 打开（或复用缓存的）Mania 数据集；并发调用只会真正打开一次。
    pub fn mania_dataset(&self, directory: &str) -> Result<Arc<ManiaDataset>, RuntimeError> {
        cached_dataset(&self.mania, directory, |path| ManiaDataset::open(path))
    }
}

fn clear_cache<T>(cache: &Mutex<Option<CachedDataset<T>>>) {
    if let Ok(mut cached) = cache.lock() {
        *cached = None;
    }
}

/// 先预留槽位再加锁检查的去重加载：并发到达时只有一个调用执行 `open`。
fn cached_dataset<T>(
    cache: &Mutex<Option<CachedDataset<T>>>,
    directory: &str,
    open: impl FnOnce(&Path) -> Result<T, RuntimeError>,
) -> Result<Arc<T>, RuntimeError> {
    let reservation = reserve_dataset(cache, directory);
    let mut loaded = reservation
        .dataset
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(dataset) = loaded.as_ref() {
        return Ok(dataset.clone());
    }
    let dataset = Arc::new(open(&reservation.directory)?);
    *loaded = Some(dataset.clone());
    Ok(dataset)
}

/// 以目录为键预留加载槽位；注册表已指向其他目录或被清空时启用新槽位。
fn reserve_dataset<T>(
    cache: &Mutex<Option<CachedDataset<T>>>,
    directory: &str,
) -> CachedDataset<T> {
    // Standard 与 Mania 各自以目录为键缓存只读实例，永不共享格式或状态。
    let path = PathBuf::from(directory);
    // Each directory/configuration generation owns a load slot. Clearing the
    // registry detaches in-flight work; it can no longer republish stale data.
    let slot = {
        let mut cached = cache.lock().unwrap_or_else(|error| error.into_inner());
        if cached
            .as_ref()
            .is_none_or(|current| current.directory != path)
        {
            *cached = Some(CachedDataset {
                directory: path.clone(),
                dataset: Arc::new(Mutex::new(None)),
            });
        }
        cached.as_ref().expect("slot initialized").dataset.clone()
    };
    CachedDataset {
        directory: path,
        dataset: slot,
    }
}

fn unavailable_status(
    ruleset: Ruleset,
    directory: &str,
    state: SimilarityIndexState,
    message: &str,
) -> SimilarityIndexStatus {
    SimilarityIndexStatus {
        ruleset,
        state,
        directory: Some(directory.into()),
        message: message.into(),
        record_count: None,
        records_by_key_count: None,
        analyzer_version: None,
        normalization_version: None,
        algorithm_id: None,
        data_cutoff_at: None,
        supports_dynamic_weighting: false,
    }
}

/// 把数据集打开错误折叠为 `Incompatible` 或 `Invalid` 状态（不含路径细节）。
fn status_from_error(
    ruleset: Ruleset,
    directory: &str,
    error: &RuntimeError,
) -> SimilarityIndexStatus {
    let (state, message) = status_copy_for_error(error.kind());
    unavailable_status(ruleset, directory, state, message)
}

fn status_copy_for_error(kind: RuntimeErrorKind) -> (SimilarityIndexState, &'static str) {
    match kind {
        RuntimeErrorKind::Incompatible => (
            SimilarityIndexState::Incompatible,
            "本地索引版本与当前 OPP 不兼容。",
        ),
        _ => (
            SimilarityIndexState::Invalid,
            "本地索引文件缺失、损坏或无法校验。",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 清空后的新配置不会被旧目录的预留加载顶替。
    #[test]
    fn queued_generation_cannot_replace_new_configuration() {
        let runtime = SimilarityRuntime::default();
        let queued = runtime.prepared(Ruleset::Osu, Some("old"));
        let old_slot = queued
            .standard
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .dataset
            .clone();
        runtime.clear(Ruleset::Osu);
        let current = runtime.prepared(Ruleset::Osu, Some("new"));
        // A worker entering after clear still uses its reserved, detached slot.
        let reservation = reserve_dataset(&queued.standard, "old");
        assert!(Arc::ptr_eq(&reservation.dataset, &old_slot));
        let registry = runtime.standard.lock().unwrap();
        let latest = current.standard.lock().unwrap();
        assert_eq!(registry.as_ref().unwrap().directory, PathBuf::from("new"));
        assert!(Arc::ptr_eq(
            &registry.as_ref().unwrap().dataset,
            &latest.as_ref().unwrap().dataset
        ));
        assert!(!Arc::ptr_eq(&registry.as_ref().unwrap().dataset, &old_slot));
    }

    // 并发加载共享槽位只打开一次；清空后旧加载与新代际互不污染。
    #[test]
    fn concurrent_loads_share_a_slot_and_clear_detaches_old_work() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = Mutex::new(None);
        let opens = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let value = cached_dataset(&cache, "fixture", |_| {
                        opens.fetch_add(1, Ordering::SeqCst);
                        Ok(42)
                    })
                    .unwrap();
                    assert_eq!(*value, 42);
                });
            }
        });
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        clear_cache(&cache);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let old = scope.spawn(|| {
                cached_dataset(&cache, "fixture", move |_| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(1)
                })
                .unwrap()
            });
            started_rx.recv().unwrap();
            clear_cache(&cache);
            assert_eq!(*cached_dataset(&cache, "fixture", |_| Ok(2)).unwrap(), 2);
            release_tx.send(()).unwrap();
            assert_eq!(*old.join().unwrap(), 1);
        });
        assert_eq!(
            *cached_dataset(&cache, "fixture", |_| panic!("must reuse new generation")).unwrap(),
            2
        );
    }

    // 未配置目录是各支持模式的正常状态。
    #[test]
    fn unconfigured_is_a_normal_per_ruleset_status() {
        let runtime = SimilarityRuntime::default();
        for ruleset in [Ruleset::Osu, Ruleset::Mania] {
            let status = runtime.inspect(ruleset, None);
            assert_eq!(status.ruleset, ruleset);
            assert_eq!(status.state, SimilarityIndexState::Unconfigured);
        }
    }

    // Taiko / Catch 无论目录如何都直接返回 unsupported。
    #[test]
    fn unsupported_rulesets_are_explicit() {
        let runtime = SimilarityRuntime::default();
        for ruleset in [Ruleset::Taiko, Ruleset::Fruits] {
            let status = runtime.inspect(ruleset, Some("Z:/ignored"));
            assert_eq!(status.ruleset, ruleset);
            assert_eq!(status.state, SimilarityIndexState::Unsupported);
        }
    }

    // 目录缺失返回 Missing，且不泄露运行时内部细节。
    #[test]
    fn missing_directory_does_not_expose_runtime_details() {
        let status = SimilarityRuntime::default()
            .inspect(Ruleset::Mania, Some("Z:/definitely-not-an-opp-index"));
        assert_eq!(status.state, SimilarityIndexState::Missing);
        assert!(status.message.contains("不可用"));
        assert!(status.records_by_key_count.is_none());
    }

    // 空目录对两种支持模式都是 Invalid（缺数据集文件）。
    #[test]
    fn incomplete_directories_are_invalid_for_each_supported_ruleset() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = SimilarityRuntime::default();
        for ruleset in [Ruleset::Osu, Ruleset::Mania] {
            let status = runtime.inspect(ruleset, directory.path().to_str());
            assert_eq!(status.state, SimilarityIndexState::Invalid);
        }
    }

    // 运行时的 Incompatible 错误映射到同名状态。
    #[test]
    fn incompatible_runtime_errors_map_to_the_incompatible_state() {
        let (state, _) = status_copy_for_error(RuntimeErrorKind::Incompatible);
        assert_eq!(state, SimilarityIndexState::Incompatible);
    }

    // 端到端查询：需环境变量指向真实 standard 索引，默认跳过。
    #[test]
    #[ignore = "requires explicit OPP_SIMILARITY_INDEX"]
    fn queries_an_opt_in_standard_similarity_index() {
        let directory = std::env::var("OPP_SIMILARITY_INDEX")
            .expect("OPP_SIMILARITY_INDEX must point to a compatible index");
        let runtime = SimilarityRuntime::default();
        let dataset = runtime
            .standard_dataset(&directory)
            .expect("open configured index");
        let target = dataset
            .analyze_target(
                b"osu file format v14\n\n[General]\nMode:0\n\n[Metadata]\nTitle:Query\nArtist:Test\nCreator:OPP\nVersion:Hard\nBeatmapID:999999999\nBeatmapSetID:999999999\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:7\nApproachRate:9\n\n[TimingPoints]\n0,500,4,2,0,100,1,0\n\n[HitObjects]\n64,64,0,1,0,0:0:0:0:\n448,320,500,1,0,0:0:0:0:\n64,64,1000,1,0,0:0:0:0:\n",
            )
            .expect("analyze a standard query map");
        let results = dataset
            .query(&target, &osu_difficulty_runtime::QueryOptions::default())
            .expect("query configured index");

        assert!(!results.is_empty());
    }

    // 打开并查询真实 Mania 索引，同时校验整个过程不改动数据集目录。
    #[test]
    #[ignore = "requires explicit OPP_MANIA_SIMILARITY_INDEX"]
    fn opens_the_opt_in_mania_similarity_index() {
        let directory = std::env::var("OPP_MANIA_SIMILARITY_INDEX")
            .expect("OPP_MANIA_SIMILARITY_INDEX must point to a compatible index");
        let before = directory_snapshot(Path::new(&directory));
        let runtime = SimilarityRuntime::default();
        let dataset = runtime
            .mania_dataset(&directory)
            .expect("open configured mania index");
        let info = dataset.info();
        assert_eq!(info.record_count, 23_551);
        assert_eq!(info.records_by_key_count.get(&4), Some(&18_550));
        assert_eq!(info.records_by_key_count.get(&6), Some(&800));
        assert_eq!(info.records_by_key_count.get(&7), Some(&4_201));
        let target = dataset
            .target_for_id(193_127)
            .expect("known 4K ranked target");
        let results = dataset
            .query(
                &target,
                &osu_difficulty_runtime::ManiaQueryOptions {
                    result_limit: 20,
                    include_same_set: false,
                    ..osu_difficulty_runtime::ManiaQueryOptions::default()
                },
            )
            .expect("query known Mania target");
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .all(|result| result.record.key_count == target.record.key_count)
        );
        assert!(results.iter().all(|result| {
            result.record.beatmap_id != target.record.beatmap_id
                && result.record.beatmapset_id != target.record.beatmapset_id
        }));
        drop(dataset);
        drop(runtime);
        assert_eq!(before, directory_snapshot(Path::new(&directory)));
    }

    fn directory_snapshot(root: &Path) -> Vec<(PathBuf, u64, Option<std::time::SystemTime>)> {
        let mut entries = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            let Ok(read_dir) = std::fs::read_dir(path) else {
                continue;
            };
            for entry in read_dir.flatten() {
                let path = entry.path();
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if metadata.is_dir() {
                    pending.push(path);
                } else if metadata.is_file() {
                    entries.push((path, metadata.len(), metadata.modified().ok()));
                }
            }
        }
        entries.sort();
        entries
    }
}
