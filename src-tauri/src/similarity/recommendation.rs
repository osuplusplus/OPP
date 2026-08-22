use std::collections::HashSet;

use osu_difficulty_runtime::ManiaGameMod;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManiaSeed {
    pub beatmap_id: u64,
    pub game_mod: ManiaGameMod,
}

pub fn mania_seed_ids(scores: &[Score], limit: usize) -> (Vec<ManiaSeed>, usize) {
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
        let Some(game_mod) = supported_mania_mod(score) else {
            if skipped_modded_ids.insert(beatmap_id) {
                skipped_modded += 1;
            }
            continue;
        };
        if !accepted.insert((beatmap_id, game_mod)) {
            continue;
        }
        ids.push(ManiaSeed {
            beatmap_id,
            game_mod,
        });
        if ids.len() == limit {
            break;
        }
    }
    (ids, skipped_modded)
}

fn supported_mania_mod(score: &Score) -> Option<ManiaGameMod> {
    let mut game_mod = ManiaGameMod::Nm;
    for value in &score.mods {
        let acronym = value
            .as_str()
            .or_else(|| value.get("acronym").and_then(serde_json::Value::as_str))?;
        if acronym.eq_ignore_ascii_case("NM") || acronym.eq_ignore_ascii_case("NO_MOD") {
            continue;
        }
        let next = if acronym.eq_ignore_ascii_case("DT") || acronym.eq_ignore_ascii_case("NC") {
            ManiaGameMod::Dt
        } else if acronym.eq_ignore_ascii_case("HT") {
            ManiaGameMod::Ht
        } else {
            return None;
        };
        if game_mod != ManiaGameMod::Nm && game_mod != next {
            return None;
        }
        game_mod = next;
    }
    Some(game_mod)
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
    fn mania_seeds_accept_nm_dt_ht_and_skip_other_mods() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(2), json!([])),
            score_with_mods(Some(3), json!(["NM"])),
            score_with_mods(Some(4), json!([{ "acronym": "NO_MOD" }])),
            score_with_mods(Some(5), json!([{ "acronym": "K4" }])),
        ];
        let (ids, skipped) = mania_seed_ids(&scores, 50);
        assert_eq!(
            ids,
            vec![
                ManiaSeed {
                    beatmap_id: 1,
                    game_mod: ManiaGameMod::Dt
                },
                ManiaSeed {
                    beatmap_id: 2,
                    game_mod: ManiaGameMod::Nm
                },
                ManiaSeed {
                    beatmap_id: 3,
                    game_mod: ManiaGameMod::Nm
                },
                ManiaSeed {
                    beatmap_id: 4,
                    game_mod: ManiaGameMod::Nm
                },
            ]
        );
        assert_eq!(skipped, 1);
    }

    #[test]
    fn mania_seed_limit_counts_usable_no_mod_scores() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "HT" }])),
            score(Some(2)),
            score(Some(3)),
        ];
        let (ids, skipped) = mania_seed_ids(&scores, 1);
        assert_eq!(
            ids,
            vec![ManiaSeed {
                beatmap_id: 1,
                game_mod: ManiaGameMod::Ht
            }]
        );
        assert_eq!(skipped, 0);
    }

    #[test]
    fn different_supported_mods_of_the_same_map_are_distinct_seeds() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(1), json!([])),
        ];
        let (ids, skipped) = mania_seed_ids(&scores, 50);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].game_mod, ManiaGameMod::Dt);
        assert_eq!(ids[1].game_mod, ManiaGameMod::Nm);
        assert_eq!(skipped, 0);
    }
}
