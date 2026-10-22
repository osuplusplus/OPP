//! 从玩家成绩挑选推荐“种子”：standard 取谱面 ID，Mania 额外解析成绩 Mod。

use std::collections::HashSet;

use osu_difficulty_runtime::ManiaGameMod;

use crate::domain::Score;

/// 推荐参考成绩（种子）的数量上限。
pub const MAX_RECOMMENDATION_SEEDS: usize = 50;

/// 附加 Mod 不改变键位排列，也不改变倍率，可以和标准倍率一起使用。
const NEUTRAL_MANIA_MODS: [&str; 7] = ["NF", "SD", "PF", "HD", "FI", "FL", "CL"];

/// 把用户给定的种子上限 clamp 到 [1, `MAX_RECOMMENDATION_SEEDS`]。
pub fn requested_seed_limit(value: Option<usize>) -> usize {
    value
        .unwrap_or(MAX_RECOMMENDATION_SEEDS)
        .clamp(1, MAX_RECOMMENDATION_SEEDS)
}

/// 按成绩顺序取过去重后的谱面 ID，最多 limit 个。
pub fn seed_ids(scores: &[Score], limit: usize) -> Vec<u64> {
    let mut seen = HashSet::new();
    scores
        .iter()
        .filter_map(|score| score.beatmap.as_ref()?.get("id")?.as_u64())
        .filter(|beatmap_id| seen.insert(*beatmap_id))
        .take(limit)
        .collect()
}

/// Mania 种子：谱面 ID 加该成绩对应的 NM / DT / HT 变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManiaSeed {
    pub beatmap_id: u64,
    pub game_mod: ManiaGameMod,
}

/// Mania 种子挑选结果，附两类被跳过成绩的计数（按谱面 ID 去重统计）。
#[derive(Debug, Default)]
pub struct ManiaSeedSelection {
    pub seeds: Vec<ManiaSeed>,
    /// 使用了无法对应到 NM / DT / HT 标准倍率的 Mod，不能作为参考成绩。
    pub unusable_mod_scores: usize,
    /// 成绩本身可用，但不属于本次请求选择的 Mod 池。
    pub outside_mod_pool_scores: usize,
}

/// 按成绩顺序挑选命中 Mod 池的 Mania 种子；同一 (谱面, Mod) 只取一个，
/// 并统计不可用与池外的成绩数，供上层给出更精确的错误提示。
pub fn mania_seed_ids(
    scores: &[Score],
    limit: usize,
    allowed_mods: &[ManiaGameMod],
) -> ManiaSeedSelection {
    let mut accepted = HashSet::new();
    let mut unusable_ids = HashSet::new();
    let mut outside_pool_ids = HashSet::new();
    let mut selection = ManiaSeedSelection {
        seeds: Vec::with_capacity(limit),
        ..ManiaSeedSelection::default()
    };
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
            if unusable_ids.insert(beatmap_id) {
                selection.unusable_mod_scores += 1;
            }
            continue;
        };
        if !allowed_mods.contains(&game_mod) {
            if outside_pool_ids.insert(beatmap_id) {
                selection.outside_mod_pool_scores += 1;
            }
            continue;
        }
        if !accepted.insert((beatmap_id, game_mod)) {
            continue;
        }
        selection.seeds.push(ManiaSeed {
            beatmap_id,
            game_mod,
        });
        if selection.seeds.len() == limit {
            break;
        }
    }
    selection
}

/// 从成绩 Mod 列表推断标准 NM / DT / HT 变体：中性 Mod 直接忽略，
/// NC / DC 分别折算为 DT / HT；遇到其他 Mod、两种倍率混用或自定义倍率
/// 时返回 `None`——这些成绩无法对应到数据集的预计算特征。
fn supported_mania_mod(score: &Score) -> Option<ManiaGameMod> {
    let mut game_mod = ManiaGameMod::Nm;
    for value in &score.mods {
        let acronym = value
            .as_str()
            .or_else(|| value.get("acronym").and_then(serde_json::Value::as_str))?;
        if acronym.eq_ignore_ascii_case("NM") || acronym.eq_ignore_ascii_case("NO_MOD") {
            continue;
        }
        if NEUTRAL_MANIA_MODS
            .iter()
            .any(|neutral| acronym.eq_ignore_ascii_case(neutral))
        {
            continue;
        }
        let next = if acronym.eq_ignore_ascii_case("DT") || acronym.eq_ignore_ascii_case("NC") {
            ManiaGameMod::Dt
        } else if acronym.eq_ignore_ascii_case("HT") || acronym.eq_ignore_ascii_case("DC") {
            ManiaGameMod::Ht
        } else {
            return None;
        };
        // 自定义倍率不是标准 1.5x / 0.75x，不能套用对应 Mod 的特征。
        if let Some(rate) = value
            .get("settings")
            .and_then(|settings| settings.get("speed_change"))
        {
            let expected = next.clock_rate();
            if rate
                .as_f64()
                .is_none_or(|value| (value - expected).abs() > 0.000_001)
            {
                return None;
            }
        }
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

    // 种子保持成绩顺序、去重并截断到上限。
    #[test]
    fn seeds_are_ordered_deduplicated_and_limited() {
        let scores = vec![score(Some(8)), score(None), score(Some(8)), score(Some(9))];
        assert_eq!(seed_ids(&scores, 2), vec![8, 9]);
    }

    // 种子上限 clamp 的边界行为。
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

    // NM / DT / HT 可作种子，其他 Mod 跳过并计数。
    #[test]
    fn mania_seeds_accept_nm_dt_ht_and_skip_other_mods() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(2), json!([])),
            score_with_mods(Some(3), json!(["NM"])),
            score_with_mods(Some(4), json!([{ "acronym": "NO_MOD" }])),
            score_with_mods(Some(5), json!([{ "acronym": "K4" }])),
        ];
        let selection = mania_seed_ids(&scores, 50, &ManiaGameMod::ALL);
        assert_eq!(
            selection.seeds,
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
        assert_eq!(selection.unusable_mod_scores, 1);
    }

    // limit 只统计可用种子，不可用成绩不占名额。
    #[test]
    fn mania_seed_limit_counts_usable_no_mod_scores() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "HT" }])),
            score(Some(2)),
            score(Some(3)),
        ];
        let selection = mania_seed_ids(&scores, 1, &ManiaGameMod::ALL);
        assert_eq!(
            selection.seeds,
            vec![ManiaSeed {
                beatmap_id: 1,
                game_mod: ManiaGameMod::Ht
            }]
        );
        assert_eq!(selection.unusable_mod_scores, 0);
    }

    // 同一张图的不同 Mod 成绩是不同种子。
    #[test]
    fn different_supported_mods_of_the_same_map_are_distinct_seeds() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(1), json!([])),
        ];
        let selection = mania_seed_ids(&scores, 50, &ManiaGameMod::ALL);
        assert_eq!(selection.seeds.len(), 2);
        assert_eq!(selection.seeds[0].game_mod, ManiaGameMod::Dt);
        assert_eq!(selection.seeds[1].game_mod, ManiaGameMod::Nm);
        assert_eq!(selection.unusable_mod_scores, 0);
    }

    // 先按 Mod 池过滤再截断，保证种子都属于请求的池。
    #[test]
    fn reference_seeds_are_filtered_by_the_requested_mod_pool_before_the_limit() {
        let scores = vec![
            score_with_mods(Some(1), json!([{ "acronym": "DT" }])),
            score_with_mods(Some(2), json!([{ "acronym": "DT" }])),
            score(Some(3)),
            score(Some(4)),
        ];
        let no_mod = mania_seed_ids(&scores, 2, &[ManiaGameMod::Nm]);
        assert_eq!(
            no_mod.seeds,
            vec![
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
        assert_eq!(no_mod.outside_mod_pool_scores, 2);
        assert_eq!(no_mod.unusable_mod_scores, 0);

        let double_time = mania_seed_ids(&scores, 2, &[ManiaGameMod::Dt]);
        assert_eq!(
            double_time.seeds,
            vec![
                ManiaSeed {
                    beatmap_id: 1,
                    game_mod: ManiaGameMod::Dt
                },
                ManiaSeed {
                    beatmap_id: 2,
                    game_mod: ManiaGameMod::Dt
                },
            ]
        );

        let all_double_time = mania_seed_ids(&scores, 50, &[ManiaGameMod::Dt]);
        assert_eq!(all_double_time.seeds.len(), 2);
        assert_eq!(all_double_time.outside_mod_pool_scores, 2);
        assert_eq!(all_double_time.unusable_mod_scores, 0);

        let mixed = mania_seed_ids(&scores, 50, &ManiaGameMod::ALL);
        assert_eq!(mixed.seeds.len(), 4);
        assert_eq!(mixed.outside_mod_pool_scores, 0);
    }

    // 中性 Mod 与 NC / DC 别名不影响成绩可用性。
    #[test]
    fn neutral_mods_and_nightcore_aliases_stay_usable() {
        let scores = vec![
            score_with_mods(Some(1), json!(["HD", "FI", "NF"])),
            score_with_mods(Some(2), json!([{ "acronym": "NC" }])),
            score_with_mods(Some(3), json!([{ "acronym": "DC" }])),
            score_with_mods(Some(4), json!(["SD", "PF", "FL", "CL", "HD"])),
        ];
        let selection = mania_seed_ids(&scores, 50, &ManiaGameMod::ALL);
        assert_eq!(
            selection.seeds,
            vec![
                ManiaSeed {
                    beatmap_id: 1,
                    game_mod: ManiaGameMod::Nm
                },
                ManiaSeed {
                    beatmap_id: 2,
                    game_mod: ManiaGameMod::Dt
                },
                ManiaSeed {
                    beatmap_id: 3,
                    game_mod: ManiaGameMod::Ht
                },
                ManiaSeed {
                    beatmap_id: 4,
                    game_mod: ManiaGameMod::Nm
                },
            ]
        );
        assert_eq!(selection.unusable_mod_scores, 0);
    }

    // 自定义倍率不能冒充标准 DT / HT。
    #[test]
    fn custom_clock_rates_never_stand_in_for_dt_or_ht() {
        let scores = vec![
            score_with_mods(
                Some(1),
                json!([{ "acronym": "DT", "settings": { "speed_change": 1.2 } }]),
            ),
            score_with_mods(
                Some(2),
                json!([{ "acronym": "NC", "settings": { "speed_change": 1.5 } }]),
            ),
            score_with_mods(
                Some(3),
                json!([{ "acronym": "HT", "settings": { "speed_change": 0.9 } }]),
            ),
            score_with_mods(
                Some(4),
                json!([{ "acronym": "DC", "settings": { "speed_change": 0.75 } }]),
            ),
        ];
        let selection = mania_seed_ids(&scores, 50, &ManiaGameMod::ALL);
        assert_eq!(
            selection.seeds,
            vec![
                ManiaSeed {
                    beatmap_id: 2,
                    game_mod: ManiaGameMod::Dt
                },
                ManiaSeed {
                    beatmap_id: 4,
                    game_mod: ManiaGameMod::Ht
                },
            ]
        );
        assert_eq!(selection.unusable_mod_scores, 2);
    }
}
