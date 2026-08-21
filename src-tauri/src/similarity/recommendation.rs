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

/// Mania v1 is a NoMod, 1.0x snapshot. Modded plays must not silently become
/// seeds for a chart analysis that cannot represent their rate/key/random changes.
pub fn mania_seed_ids(scores: &[Score], limit: usize) -> (Vec<u64>, usize) {
    let mut accepted = HashSet::new();
    let mut skipped_modded_ids = HashSet::new();
    let mut ids = Vec::with_capacity(limit);
    let mut skipped_modded = 0;
    for score in scores {
        let Some(beatmap_id) = score
            .beatmap
            .as_ref()
            .and_then(|beatmap| beatmap.get("id"))
            .and_then(serde_json::Value::as_u64)
        else {
            continue;
        };
        if accepted.contains(&beatmap_id) {
            continue;
        }
        if !is_no_mod_score(score) {
            if skipped_modded_ids.insert(beatmap_id) {
                skipped_modded += 1;
            }
            continue;
        }
        accepted.insert(beatmap_id);
        ids.push(beatmap_id);
        if ids.len() == limit {
            break;
        }
    }
    (ids, skipped_modded)
}

fn is_no_mod_score(score: &Score) -> bool {
    score.mods.is_empty()
        || score.mods.iter().all(|value| {
            let acronym = value
                .as_str()
                .or_else(|| value.get("acronym").and_then(serde_json::Value::as_str));
            acronym.is_some_and(|value| {
                value.eq_ignore_ascii_case("NM") || value.eq_ignore_ascii_case("NO_MOD")
            })
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn score_with_mods(beatmap_id: Option<u64>, mods: serde_json::Value) -> Score {
        serde_json::from_value(json!({
            "user_id": 1,
            "rank": "A",
            "statistics": {},
            "beatmap": beatmap_id.map(|id| json!({ "id": id })),
            "mods": mods
        }))
        .expect("score fixture")
    }

    fn score(beatmap_id: Option<u64>) -> Score {
        score_with_mods(beatmap_id, json!([]))
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

    #[test]
    fn mania_seeds_accept_only_no_mod_and_count_modded_scores_as_skipped() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(2), json!([])),
            score_with_mods(Some(3), json!(["NM"])),
            score_with_mods(Some(4), json!([{ "acronym": "NO_MOD" }])),
            score_with_mods(Some(5), json!([{ "acronym": "K4" }])),
        ];
        let (ids, skipped) = mania_seed_ids(&scores, 50);
        assert_eq!(ids, vec![2, 3, 4]);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn mania_seed_limit_counts_usable_no_mod_scores() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "HT" }])),
            score(Some(2)),
            score(Some(3)),
        ];
        let (ids, skipped) = mania_seed_ids(&scores, 1);
        assert_eq!(ids, vec![2]);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn a_modded_score_does_not_hide_a_later_no_mod_score_for_the_same_map() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(1), json!([])),
        ];
        let (ids, skipped) = mania_seed_ids(&scores, 50);
        assert_eq!(ids, vec![1]);
        assert_eq!(skipped, 1);
    }
}
