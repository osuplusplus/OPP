use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use osu_difficulty_runtime::{Dataset, ManiaDataset, RuntimeError, RuntimeErrorKind};

use crate::{
    app::models::Ruleset,
    similarity::models::{SimilarityIndexState, SimilarityIndexStatus},
};

struct CachedDataset<T> {
    directory: PathBuf,
    dataset: Arc<T>,
}

#[derive(Default)]
pub struct SimilarityRuntime {
    standard: Mutex<Option<CachedDataset<Dataset>>>,
    mania: Mutex<Option<CachedDataset<ManiaDataset>>>,
}

impl SimilarityRuntime {
    pub fn clear(&self, ruleset: Ruleset) {
        match ruleset {
            Ruleset::Osu => clear_cache(&self.standard),
            Ruleset::Mania => clear_cache(&self.mania),
            Ruleset::Taiko | Ruleset::Fruits => {}
        }
    }

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

    pub fn standard_dataset(&self, directory: &str) -> Result<Arc<Dataset>, RuntimeError> {
        cached_dataset(&self.standard, directory, |path| Dataset::open(path))
    }

    pub fn mania_dataset(&self, directory: &str) -> Result<Arc<ManiaDataset>, RuntimeError> {
        cached_dataset(&self.mania, directory, |path| ManiaDataset::open(path))
    }
}

fn clear_cache<T>(cache: &Mutex<Option<CachedDataset<T>>>) {
    if let Ok(mut cached) = cache.lock() {
        *cached = None;
    }
}

fn cached_dataset<T>(
    cache: &Mutex<Option<CachedDataset<T>>>,
    directory: &str,
    open: impl FnOnce(&Path) -> Result<T, RuntimeError>,
) -> Result<Arc<T>, RuntimeError> {
    // Standard 与 Mania 各自以目录为键缓存只读实例，永不共享格式或状态。
    let path = PathBuf::from(directory);
    if let Ok(cached) = cache.lock()
        && let Some(cached) = cached.as_ref()
        && cached.directory == path
    {
        return Ok(cached.dataset.clone());
    }
    let dataset = Arc::new(open(&path)?);
    if let Ok(mut cached) = cache.lock() {
        *cached = Some(CachedDataset {
            directory: path,
            dataset: dataset.clone(),
        });
    }
    Ok(dataset)
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

    #[test]
    fn unconfigured_is_a_normal_per_ruleset_status() {
        let runtime = SimilarityRuntime::default();
        for ruleset in [Ruleset::Osu, Ruleset::Mania] {
            let status = runtime.inspect(ruleset, None);
            assert_eq!(status.ruleset, ruleset);
            assert_eq!(status.state, SimilarityIndexState::Unconfigured);
        }
    }

    #[test]
    fn unsupported_rulesets_are_explicit() {
        let runtime = SimilarityRuntime::default();
        for ruleset in [Ruleset::Taiko, Ruleset::Fruits] {
            let status = runtime.inspect(ruleset, Some("Z:/ignored"));
            assert_eq!(status.ruleset, ruleset);
            assert_eq!(status.state, SimilarityIndexState::Unsupported);
        }
    }

    #[test]
    fn missing_directory_does_not_expose_runtime_details() {
        let status = SimilarityRuntime::default()
            .inspect(Ruleset::Mania, Some("Z:/definitely-not-an-opp-index"));
        assert_eq!(status.state, SimilarityIndexState::Missing);
        assert!(status.message.contains("不可用"));
        assert!(status.records_by_key_count.is_none());
    }

    #[test]
    fn incomplete_directories_are_invalid_for_each_supported_ruleset() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = SimilarityRuntime::default();
        for ruleset in [Ruleset::Osu, Ruleset::Mania] {
            let status = runtime.inspect(ruleset, directory.path().to_str());
            assert_eq!(status.state, SimilarityIndexState::Invalid);
        }
    }

    #[test]
    fn incompatible_runtime_errors_map_to_the_incompatible_state() {
        let (state, _) = status_copy_for_error(RuntimeErrorKind::Incompatible);
        assert_eq!(state, SimilarityIndexState::Incompatible);
    }

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
