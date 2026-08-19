use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use osu_difficulty_runtime::{Dataset, RuntimeError, RuntimeErrorKind};

use crate::similarity::models::{SimilarityIndexState, SimilarityIndexStatus};

struct CachedDataset {
    directory: PathBuf,
    dataset: Arc<Dataset>,
}

#[derive(Default)]
pub struct SimilarityRuntime {
    cached: Mutex<Option<CachedDataset>>,
}

impl SimilarityRuntime {
    pub fn clear(&self) {
        if let Ok(mut cached) = self.cached.lock() {
            *cached = None;
        }
    }

    pub fn inspect(&self, directory: Option<&str>) -> SimilarityIndexStatus {
        // 检查仅描述索引可用性，不加载完整数据集，避免设置页触发昂贵初始化。
        let Some(directory) = directory.map(str::trim).filter(|value| !value.is_empty()) else {
            return SimilarityIndexStatus::unconfigured();
        };
        let path = Path::new(directory);
        if !path.is_dir() {
            self.clear();
            return SimilarityIndexStatus {
                state: SimilarityIndexState::Missing,
                directory: Some(directory.into()),
                message: "已配置的本地索引目录不可用，请重新选择。".into(),
                record_count: None,
                analyzer_version: None,
                normalization_version: None,
                algorithm_id: None,
                data_cutoff_at: None,
                supports_dynamic_weighting: false,
            };
        }
        match self.dataset(directory) {
            Ok(dataset) => {
                let info = dataset.info();
                SimilarityIndexStatus {
                    state: SimilarityIndexState::Ready,
                    directory: Some(directory.into()),
                    message: "本地索引已就绪。".into(),
                    record_count: Some(info.record_count),
                    analyzer_version: Some(info.analyzer_version),
                    normalization_version: Some(info.normalization_version),
                    algorithm_id: Some(info.algorithm_id.clone()),
                    data_cutoff_at: info.data_cutoff_at,
                    supports_dynamic_weighting: info.supports_dynamic_weighting,
                }
            }
            Err(error) => status_from_error(directory, &error),
        }
    }

    pub fn dataset(&self, directory: &str) -> Result<Arc<Dataset>, RuntimeError> {
        // 以规范化目录为键缓存运行时数据集；同一索引的并发查询共享只读实例。
        let path = PathBuf::from(directory);
        if let Ok(cached) = self.cached.lock()
            && let Some(cached) = cached.as_ref()
            && cached.directory == path
        {
            return Ok(cached.dataset.clone());
        }
        let dataset = Arc::new(Dataset::open(&path)?);
        if let Ok(mut cached) = self.cached.lock() {
            *cached = Some(CachedDataset {
                directory: path,
                dataset: dataset.clone(),
            });
        }
        Ok(dataset)
    }
}

fn status_from_error(directory: &str, error: &RuntimeError) -> SimilarityIndexStatus {
    let (state, message) = status_copy_for_error(error.kind());
    SimilarityIndexStatus {
        state,
        directory: Some(directory.into()),
        message: message.into(),
        record_count: None,
        analyzer_version: None,
        normalization_version: None,
        algorithm_id: None,
        data_cutoff_at: None,
        supports_dynamic_weighting: false,
    }
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
    fn unconfigured_is_a_normal_status() {
        let status = SimilarityRuntime::default().inspect(None);
        assert_eq!(status.state, SimilarityIndexState::Unconfigured);
    }

    #[test]
    fn missing_directory_does_not_expose_runtime_details() {
        let status = SimilarityRuntime::default().inspect(Some("Z:/definitely-not-an-opp-index"));
        assert_eq!(status.state, SimilarityIndexState::Missing);
        assert!(status.message.contains("不可用"));
    }

    #[test]
    fn incomplete_directory_is_invalid() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let status = SimilarityRuntime::default().inspect(directory.path().to_str());
        assert_eq!(status.state, SimilarityIndexState::Invalid);
    }

    #[test]
    fn incompatible_runtime_errors_map_to_the_incompatible_state() {
        let (state, _) = status_copy_for_error(RuntimeErrorKind::Incompatible);
        assert_eq!(state, SimilarityIndexState::Incompatible);
    }

    #[test]
    #[ignore = "requires explicit OPP_SIMILARITY_INDEX"]
    fn queries_an_opt_in_similarity_index() {
        let directory = std::env::var("OPP_SIMILARITY_INDEX")
            .expect("OPP_SIMILARITY_INDEX must point to a compatible index");
        let runtime = SimilarityRuntime::default();
        let dataset = runtime.dataset(&directory).expect("open configured index");
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
}
