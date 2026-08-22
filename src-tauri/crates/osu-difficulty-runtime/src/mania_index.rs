//! Read-only Mania v1 bucket-index compatibility.
//!
//! Layout, bucket expansion, and distance weights are pinned to
//! osu-difficulty-lab commit 1fa21fa6a5144992df58efe7ce9d96019981fad3.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ManiaDistanceComponents, ManiaFeatureRecord, ManiaModeFamily, ManiaQueryOptions,
    MANIA_ANALYZER_VERSION, MANIA_NORMALIZATION_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ManiaBucketEntry {
    pub(crate) beatmap_id: u64,
    pub(crate) beatmapset_id: u64,
    pub(crate) mode_family: ManiaModeFamily,
    pub(crate) normalized_offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManiaBucket {
    key_count: u8,
    difficulty_band: u8,
    entries: Vec<ManiaBucketEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ManiaBucketIndex {
    normalization_version: u32,
    analyzer_version: u32,
    buckets: Vec<ManiaBucket>,
}

impl ManiaBucketIndex {
    pub(crate) fn read(root: &Path) -> Result<Self, String> {
        let path = root.join("indexes").join("mania-v1.buckets");
        let bytes = fs::read(&path).map_err(|_| "mania bucket index is missing".to_owned())?;
        let checksum_path = root.join("indexes").join("mania-v1.buckets.sha256");
        let saved = fs::read_to_string(checksum_path)
            .map_err(|_| "mania bucket index checksum is missing".to_owned())?;
        if saved.trim() != hex::encode(Sha256::digest(&bytes)) {
            return Err("mania bucket index checksum mismatch".into());
        }
        let index: Self = bincode::deserialize(&bytes)
            .map_err(|error| format!("invalid mania bucket index: {error}"))?;
        let encoded_size = bincode::serialized_size(&index)
            .map_err(|error| format!("invalid mania bucket index layout: {error}"))?
            as usize;
        if encoded_size != bytes.len() {
            return Err("mania bucket index has trailing or truncated data".into());
        }
        index.validate_shape()?;
        Ok(index)
    }

    fn validate_shape(&self) -> Result<(), String> {
        if self.normalization_version != MANIA_NORMALIZATION_VERSION
            || self.analyzer_version != MANIA_ANALYZER_VERSION
        {
            return Err("mania bucket index version does not match Analyzer v1".into());
        }
        if self.buckets.is_empty() {
            return Err("mania bucket index is empty".into());
        }
        let mut bucket_keys = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for bucket in &self.buckets {
            if !matches!(bucket.key_count, 4 | 6 | 7) || bucket.difficulty_band > 9 {
                return Err("mania bucket index contains an invalid bucket".into());
            }
            if !bucket_keys.insert((bucket.key_count, bucket.difficulty_band)) {
                return Err("mania bucket index contains duplicate buckets".into());
            }
            if bucket.entries.is_empty() {
                return Err("mania bucket index contains an empty bucket".into());
            }
            for entry in &bucket.entries {
                if entry.beatmap_id == 0 || !ids.insert(entry.beatmap_id) {
                    return Err("mania bucket index contains duplicate or zero beatmap IDs".into());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn coverage(&self) -> BTreeMap<u64, (u8, u8, u64, ManiaModeFamily, u64)> {
        let mut coverage = BTreeMap::new();
        for bucket in &self.buckets {
            for entry in &bucket.entries {
                coverage.insert(
                    entry.beatmap_id,
                    (
                        bucket.key_count,
                        bucket.difficulty_band,
                        entry.beatmapset_id,
                        entry.mode_family,
                        entry.normalized_offset,
                    ),
                );
            }
        }
        coverage
    }

    pub(crate) fn candidates<'a>(
        &'a self,
        target: &ManiaFeatureRecord,
        query: &ManiaQueryOptions,
    ) -> Vec<&'a ManiaBucketEntry> {
        let target_total = 256_usize.max(query.result_limit.saturating_mul(4));
        let target_family = 32_usize.max(query.result_limit.saturating_mul(2));
        let mut candidate_entries = BTreeMap::<u64, &ManiaBucketEntry>::new();
        let center = target.difficulty_band as i16;
        for radius in 0_i16..=9 {
            let mut bands = vec![center - radius];
            if radius > 0 {
                bands.push(center + radius);
            }
            for band in bands.into_iter().filter(|band| (0..=9).contains(band)) {
                if let Some(bucket) = self.buckets.iter().find(|bucket| {
                    bucket.key_count == target.key_count && bucket.difficulty_band == band as u8
                }) {
                    candidate_entries
                        .extend(bucket.entries.iter().map(|entry| (entry.beatmap_id, entry)));
                }
            }
            let mut total = 0_usize;
            let mut same_family = 0_usize;
            for candidate in candidate_entries.values() {
                if excluded_entry(target, candidate, query.include_same_set) {
                    continue;
                }
                total += 1;
                if candidate.mode_family == target.mode_family {
                    same_family += 1;
                }
            }
            if total >= target_total && same_family >= target_family {
                break;
            }
        }
        candidate_entries
            .into_values()
            .filter(|candidate| !excluded_entry(target, candidate, query.include_same_set))
            .collect()
    }
}

fn excluded_entry(
    target: &ManiaFeatureRecord,
    candidate: &ManiaBucketEntry,
    include_same_set: bool,
) -> bool {
    candidate.beatmap_id == target.beatmap_id
        || (!include_same_set
            && target.beatmapset_id != 0
            && candidate.beatmapset_id == target.beatmapset_id)
}

pub(crate) fn distance_components(
    target: ManiaFeatureRecord,
    candidate: ManiaFeatureRecord,
) -> ManiaDistanceComponents {
    ManiaDistanceComponents {
        skill: hellinger(&skill_shape(target), &skill_shape(candidate)),
        pattern: hellinger(
            &target.style.pattern_array(),
            &candidate.style.pattern_array(),
        ),
        structure: rms_distance(
            &target.style.structure_array(),
            &candidate.style.structure_array(),
        ),
        difficulty: (target.difficulty_percentile - candidate.difficulty_percentile).abs(),
        context: 0.5 * log_ratio_distance(target.base.bpm, candidate.base.bpm, 4.0)
            + 0.5
                * log_ratio_distance(
                    target.base.active_length_seconds,
                    candidate.base.active_length_seconds,
                    10.0,
                ),
    }
}

/// Convert cohort percentiles into an intra-map skill profile. Absolute intensity
/// is already represented by `difficulty_percentile`; centering here prevents a
/// high-end map whose eight axes all reached percentile 1.0 from becoming an
/// indistinguishable full vector.
fn skill_shape(record: ManiaFeatureRecord) -> [f32; 8] {
    let values = record.difficulty.as_array();
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values.map(|value| ((value - mean) * 4.0).exp())
}

pub(crate) fn classification_tier(target: ManiaFeatureRecord, candidate: ManiaFeatureRecord) -> u8 {
    match (
        target.mode_family == candidate.mode_family,
        target.dominant_pattern == candidate.dominant_pattern,
    ) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

pub(crate) fn final_distance(components: ManiaDistanceComponents) -> f32 {
    // The intra-map skill profile is the shape users see in the radar. Keep it
    // dominant, while pattern and structure provide the next level of coupling.
    0.50 * components.skill
        + 0.25 * components.pattern
        + 0.17 * components.structure
        + 0.05 * components.difficulty
        + 0.03 * components.context
}

fn hellinger<const N: usize>(left: &[f32; N], right: &[f32; N]) -> f32 {
    const EPSILON: f32 = 1e-6;
    let left_sum = left.iter().sum::<f32>() + EPSILON * N as f32;
    let right_sum = right.iter().sum::<f32>() + EPSILON * N as f32;
    (left
        .iter()
        .zip(right)
        .map(|(left, right)| {
            let left = ((*left + EPSILON) / left_sum).sqrt();
            let right = ((*right + EPSILON) / right_sum).sqrt();
            (left - right).powi(2)
        })
        .sum::<f32>()
        / 2.0)
        .sqrt()
        .clamp(0.0, 1.0)
}

fn rms_distance<const N: usize>(left: &[f32; N], right: &[f32; N]) -> f32 {
    (left
        .iter()
        .zip(right)
        .map(|(left, right)| (left - right).powi(2))
        .sum::<f32>()
        / N as f32)
        .sqrt()
        .clamp(0.0, 1.0)
}

fn log_ratio_distance(left: f32, right: f32, maximum_ratio: f32) -> f32 {
    (((left.max(0.0) + 1.0) / (right.max(0.0) + 1.0)).ln().abs() / maximum_ratio.ln())
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ManiaPattern;

    fn classified(family: ManiaModeFamily, pattern: ManiaPattern) -> ManiaFeatureRecord {
        ManiaFeatureRecord {
            mode_family: family,
            dominant_pattern: pattern,
            ..ManiaFeatureRecord::default()
        }
    }

    #[test]
    fn classification_tiers_precede_distance_ranking() {
        let target = classified(ManiaModeFamily::Rc, ManiaPattern::Stream);
        assert_eq!(classification_tier(target, target), 0);
        assert_eq!(
            classification_tier(target, classified(ManiaModeFamily::Rc, ManiaPattern::Jacks)),
            1
        );
        assert_eq!(
            classification_tier(
                target,
                classified(ManiaModeFamily::Ln, ManiaPattern::Stream)
            ),
            2
        );
        assert_eq!(
            classification_tier(target, classified(ManiaModeFamily::Ln, ManiaPattern::Jacks)),
            3
        );
    }

    #[test]
    fn saturated_skill_vectors_remain_finite() {
        let record = ManiaFeatureRecord {
            difficulty: crate::ManiaDifficultyVector::from_array([1.0; 8]),
            ..ManiaFeatureRecord::default()
        };
        assert!(skill_shape(record).into_iter().all(f32::is_finite));
        assert_eq!(distance_components(record, record).skill, 0.0);
    }
}
