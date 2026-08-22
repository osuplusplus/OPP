//! Read-only osu!mania v1 normalizer compatibility.
//!
//! The wire layout and transform are pinned to osu-difficulty-lab commit
//! 1fa21fa6a5144992df58efe7ce9d96019981fad3.  Deliberately no fit/save API is
//! exposed: runtime datasets are immutable inputs.

use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    MANIA_ANALYZER_VERSION, MANIA_DIFFICULTY_DIMENSIONS, MANIA_NORMALIZATION_VERSION,
    ManiaDifficultyVector, ManiaFeatureRecord, ManiaRawFeatureRecord,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyNormalizer {
    key_count: u8,
    axes: [Vec<f32>; MANIA_DIFFICULTY_DIMENSIONS],
    overall: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManiaNormalizer {
    pub version: u32,
    pub analyzer_version: u32,
    keys: Vec<KeyNormalizer>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ManiaNormalizeError {
    #[error("mania normalizer is missing")]
    Missing,
    #[error("mania normalizer is invalid: {0}")]
    Invalid(String),
    #[error("mania normalizer is incompatible: {0}")]
    Incompatible(String),
}

impl ManiaNormalizer {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ManiaNormalizeError> {
        let path = root.as_ref().join("normalizers").join("mania-v1.bin");
        let bytes = fs::read(path).map_err(|_| ManiaNormalizeError::Missing)?;
        let normalizer: Self = bincode::deserialize(&bytes)
            .map_err(|error| ManiaNormalizeError::Invalid(error.to_string()))?;
        let encoded_size = bincode::serialized_size(&normalizer)
            .map_err(|error| ManiaNormalizeError::Invalid(error.to_string()))?
            as usize;
        if encoded_size != bytes.len() {
            return Err(ManiaNormalizeError::Invalid(
                "normalizer has trailing or truncated data".into(),
            ));
        }
        normalizer.validate()?;
        Ok(normalizer)
    }

    pub fn transform(
        &self,
        raw: &ManiaRawFeatureRecord,
    ) -> Result<ManiaFeatureRecord, ManiaNormalizeError> {
        if self.analyzer_version != MANIA_ANALYZER_VERSION
            || raw.analyzer_version != self.analyzer_version
        {
            return Err(ManiaNormalizeError::Incompatible(
                "raw feature version does not match the normalizer".into(),
            ));
        }
        if raw
            .difficulty
            .as_array()
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
            || raw
                .style
                .as_array()
                .into_iter()
                .any(|value| !(0.0..=1.0).contains(&value))
        {
            return Err(ManiaNormalizeError::Invalid(
                "raw feature is outside its declared range".into(),
            ));
        }
        let key = self
            .keys
            .iter()
            .find(|normalizer| normalizer.key_count == raw.key_count)
            .ok_or_else(|| {
                ManiaNormalizeError::Incompatible(format!(
                    "normalizer has no {}K cohort",
                    raw.key_count
                ))
            })?;
        let mut normalized = [0.0_f32; MANIA_DIFFICULTY_DIMENSIONS];
        for (index, value) in raw.difficulty.as_array().into_iter().enumerate() {
            normalized[index] = midrank(&key.axes[index], value);
        }
        let percentile = midrank(&key.overall, overall_intensity(normalized));
        Ok(ManiaFeatureRecord {
            beatmap_id: raw.beatmap_id,
            beatmapset_id: raw.beatmapset_id,
            difficulty: ManiaDifficultyVector::from_array(normalized),
            style: raw.style,
            base: raw.base,
            difficulty_percentile: percentile,
            difficulty_band: ((percentile * 10.0).floor() as u8).min(9),
            key_count: raw.key_count,
            mode_family: raw.mode_family,
            dominant_pattern: raw.dominant_pattern,
            analyzer_version: raw.analyzer_version,
            normalization_version: self.version,
        })
    }

    pub(crate) fn cohort_sizes(&self) -> BTreeMap<u8, usize> {
        self.keys
            .iter()
            .map(|key| (key.key_count, key.overall.len()))
            .collect()
    }

    fn validate(&self) -> Result<(), ManiaNormalizeError> {
        if self.version != MANIA_NORMALIZATION_VERSION
            || self.analyzer_version != MANIA_ANALYZER_VERSION
        {
            return Err(ManiaNormalizeError::Incompatible(
                "version does not match Mania Analyzer v1".into(),
            ));
        }
        let cohorts = self.cohort_sizes();
        if self.keys.len() != 3
            || cohorts.len() != self.keys.len()
            || ![4, 6, 7].into_iter().all(|key| cohorts.contains_key(&key))
        {
            return Err(ManiaNormalizeError::Invalid(
                "expected exactly one 4K, 6K, and 7K cohort".into(),
            ));
        }
        for key in &self.keys {
            if key.overall.is_empty()
                || key
                    .axes
                    .iter()
                    .any(|axis| axis.len() != key.overall.len() || axis.is_empty())
                || key
                    .axes
                    .iter()
                    .flatten()
                    .any(|value| !value.is_finite() || *value < 0.0)
                || key
                    .overall
                    .iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                || key.axes.iter().any(|axis| !is_sorted(axis))
                || !is_sorted(&key.overall)
            {
                return Err(ManiaNormalizeError::Invalid(format!(
                    "{}K cohort is malformed",
                    key.key_count
                )));
            }
        }
        Ok(())
    }
}

pub fn overall_intensity(values: [f32; MANIA_DIFFICULTY_DIMENSIONS]) -> f32 {
    let maximum = values.into_iter().fold(0.0_f32, f32::max);
    let rms = (values.into_iter().map(|value| value * value).sum::<f32>()
        / MANIA_DIFFICULTY_DIMENSIONS as f32)
        .sqrt();
    let mut sorted = values;
    sorted.sort_by(|left, right| right.total_cmp(left));
    let top_three = (sorted[0] + sorted[1] + sorted[2]) / 3.0;
    (0.50 * maximum + 0.30 * rms + 0.20 * top_three).clamp(0.0, 1.0)
}

fn midrank(sorted: &[f32], value: f32) -> f32 {
    if sorted.len() <= 1 {
        return 0.5;
    }
    let lower = sorted.partition_point(|candidate| candidate.total_cmp(&value).is_lt());
    let upper = sorted.partition_point(|candidate| !candidate.total_cmp(&value).is_gt());
    let midpoint = if upper > lower {
        (lower + upper - 1) as f32 / 2.0
    } else {
        lower as f32
    };
    (midpoint / (sorted.len() - 1) as f32).clamp(0.0, 1.0)
}

fn is_sorted(values: &[f32]) -> bool {
    values
        .windows(2)
        .all(|pair| pair[0].total_cmp(&pair[1]).is_le())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn duplicate_values_receive_the_same_midrank() {
        assert_eq!(midrank(&[1.0, 1.0, 2.0], 1.0), 0.25);
    }

    #[test]
    fn overall_intensity_stays_bounded() {
        assert_eq!(overall_intensity([0.0; 8]), 0.0);
        assert_eq!(overall_intensity([1.0; 8]), 1.0);
    }

    #[test]
    fn load_rejects_duplicate_key_count_cohorts() {
        let root = tempdir().expect("temporary directory");
        fs::create_dir_all(root.path().join("normalizers")).expect("normalizer directory");
        let cohort = |key_count| KeyNormalizer {
            key_count,
            axes: std::array::from_fn(|_| vec![0.0]),
            overall: vec![0.5],
        };
        let normalizer = ManiaNormalizer {
            version: MANIA_NORMALIZATION_VERSION,
            analyzer_version: MANIA_ANALYZER_VERSION,
            keys: vec![cohort(4), cohort(4), cohort(6), cohort(7)],
        };
        let bytes = bincode::serialize(&normalizer).expect("serialize malformed normalizer");
        fs::write(root.path().join("normalizers/mania-v1.bin"), bytes)
            .expect("write malformed normalizer");

        let error = ManiaNormalizer::load(root.path()).expect_err("duplicate cohort must fail");
        assert!(matches!(error, ManiaNormalizeError::Invalid(_)));
    }
}
