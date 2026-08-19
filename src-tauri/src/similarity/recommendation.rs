use std::collections::HashSet;

use crate::app::models::Score;

pub const MAX_RECOMMENDATION_SEEDS: usize = 50;

pub fn requested_seed_limit(value: Option<usize>) -> usize {
    value
        .unwrap_or(MAX_RECOMMENDATION_SEEDS)
        .clamp(1, MAX_RECOMMENDATION_SEEDS)
}

pub fn seed_ids(scores: &[Score], limit: usize) -> Vec<u64> {
    let mut seen = HashSet::new();
    scores
        .iter()
        .filter_map(|score| score.beatmap.as_ref()?.get("id")?.as_u64())
        .filter(|beatmap_id| seen.insert(*beatmap_id))
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn score(beatmap_id: Option<u64>) -> Score {
        serde_json::from_value(json!({
            "user_id": 1,
            "rank": "A",
            "statistics": {},
            "beatmap": beatmap_id.map(|id| json!({ "id": id }))
        }))
        .expect("score fixture")
    }

    #[test]
    fn seeds_are_ordered_deduplicated_and_limited() {
        let scores = vec![score(Some(8)), score(None), score(Some(8)), score(Some(9))];
        assert_eq!(seed_ids(&scores, 2), vec![8, 9]);
    }

    #[test]
    fn requested_limit_stays_within_the_supported_range() {
        assert_eq!(requested_seed_limit(None), MAX_RECOMMENDATION_SEEDS);
        assert_eq!(requested_seed_limit(Some(0)), 1);
        assert_eq!(requested_seed_limit(Some(5)), 5);
        assert_eq!(
            requested_seed_limit(Some(usize::MAX)),
            MAX_RECOMMENDATION_SEEDS
        );
    }
}
