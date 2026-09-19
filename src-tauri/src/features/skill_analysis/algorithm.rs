use rosu_map::Beatmap as Map;
use rosu_pp::Difficulty;

use super::models::SkillVector;

pub const ALGORITHM_ID: &str = "opp-osuskills-standard";
pub const ALGORITHM_VERSION: &str = "1";
pub const ALGORITHM_SOURCE: &str = "Public osuSkills model, clean Rust implementation";

pub fn calculate(bytes: &[u8], mods: &[String]) -> Result<(SkillVector, Map), String> {
    let map = Map::from_bytes(bytes).map_err(|error| format!("谱面解析失败：{error}"))?;
    if map.mode != rosu_map::section::general::GameMode::Osu {
        return Err("技能分析首版仅支持 osu! standard 谱面".into());
    }
    let pp_map = rosu_pp::Beatmap::from_bytes(bytes).map_err(|error| error.to_string())?;
    let bits = mod_bits(mods)?;
    let attributes = Difficulty::new().mods(bits).calculate(&pp_map);
    let stars = attributes.stars().max(0.1);
    let object_count = map.hit_objects.len() as f64;
    let length_seconds = map
        .hit_objects
        .first()
        .zip(map.hit_objects.last())
        .map(|(first, last)| ((last.start_time - first.start_time).max(0.0)) / 1000.0)
        .unwrap_or(0.0);
    let nps = if length_seconds > 0.0 {
        object_count / length_seconds
    } else {
        0.0
    };
    let (rate, cs, ar, od) = transformed_attributes(&map, bits);
    let bpm = pp_map.bpm() * rate;
    let mod_factor = if bits & 64 != 0 {
        1.12
    } else if bits & 256 != 0 {
        0.88
    } else {
        1.0
    };
    let hidden = bits & 8 != 0;
    let flashlight = bits & 1024 != 0;
    let skills = SkillVector {
        stamina: (stars * 42.0 + nps * 12.0 * rate) * mod_factor,
        tenacity: (stars * 35.0 + nps * 18.0 * rate + length_seconds.sqrt() * 2.0) * mod_factor,
        agility: (stars * 40.0 + cs * 10.0 + bpm * 0.03) * mod_factor,
        accuracy: (stars * 32.0 + od * 18.0 + nps * 4.0 * rate) * mod_factor,
        precision: (stars * 28.0 + cs * 18.0 + od * 12.0) * mod_factor,
        reaction: (stars * 30.0 + ar * 16.0 + bpm * 0.05 + if hidden { 18.0 } else { 0.0 })
            * mod_factor,
        memory: if flashlight {
            stars * 42.0 + nps * 8.0
        } else {
            0.0
        },
        reading: (stars * 30.0 + ar * 14.0 + nps * 5.0 * rate + if hidden { 12.0 } else { 0.0 })
            * mod_factor,
    };
    Ok((skills, map))
}

fn transformed_attributes(map: &Map, bits: u32) -> (f64, f64, f64, f64) {
    let rate = if bits & 64 != 0 {
        1.5
    } else if bits & 256 != 0 {
        0.75
    } else {
        1.0
    };
    let multiplier = |value: f64| {
        let mut result = value;
        if bits & 16 != 0 {
            result *= 1.1;
        }
        if bits & 2 != 0 {
            result *= 0.5;
        }
        result.clamp(0.0, 10.0)
    };
    (
        rate,
        multiplier(map.circle_size as f64),
        multiplier(map.approach_rate as f64),
        multiplier(map.overall_difficulty as f64),
    )
}

fn mod_bits(mods: &[String]) -> Result<u32, String> {
    let mut bits = 0;
    for value in mods {
        match value.trim().to_ascii_uppercase().as_str() {
            "NM" | "NOMOD" => {}
            // Classic has no legacy mod bit. This skill model uses map difficulty
            // and score statistics, so CL adds no separate difficulty modifier.
            // Keep it in the original score mods for display.
            "CL" => {}
            "NF" => bits |= 1,
            "EZ" => bits |= 2,
            "HD" => bits |= 8,
            "HR" => bits |= 16,
            "SD" => bits |= 32,
            "DT" => bits |= 64,
            "HT" => bits |= 256,
            "FL" => bits |= 1024,
            "" => {}
            other => return Err(format!("暂不支持 Mod：{other}")),
        }
    }
    Ok(bits)
}

pub fn score_factor(misses: u32, combo: Option<u64>, max_combo: Option<u64>) -> f64 {
    let miss_factor = 0.97_f64.powi(misses as i32);
    let combo_factor = match (combo, max_combo) {
        (Some(combo), Some(max)) if max > 0 => {
            (combo as f64 / max as f64).clamp(0.0, 1.0).powf(0.8)
        }
        _ => 1.0,
    };
    miss_factor * combo_factor
}

#[cfg(test)]
mod tests {
    use super::{calculate, mod_bits, score_factor, transformed_attributes};

    #[test]
    fn classic_scores_preserve_skill_values_and_other_mods() {
        let bytes = b"osu file format v14\n\n[General]\nMode: 0\n\n[Difficulty]\nCircleSize: 4\nApproachRate: 8\nOverallDifficulty: 7\nSliderMultiplier: 1.4\nSliderTickRate: 1\n\n[TimingPoints]\n0,500,4,2,1,100,1,0\n\n[HitObjects]\n64,192,0,1,0,0:0:0:0:\n256,192,500,2,0,L|448:192,1,192\n448,192,1500,1,0,0:0:0:0:\n64,192,2000,1,0,0:0:0:0:\n";
        let no_mod = calculate(bytes, &[]).expect("NM should be supported").0;
        for acronyms in [vec![], vec!["HD", "HR", "DT"], vec!["EZ", "HT", "FL"]] {
            let mut mods: Vec<String> = acronyms.iter().map(|value| value.to_string()).collect();
            let expected = calculate(bytes, &mods).expect("supported mods").0;
            if !mods.is_empty() {
                assert_ne!(expected, no_mod);
            }
            for classic in ["CL", "cl", " CL "] {
                mods.push(classic.into());
                let actual = calculate(bytes, &mods).expect("CL should be supported").0;
                assert_eq!(actual, expected);
                mods.pop();
            }
        }
    }

    #[test]
    fn classic_does_not_bypass_unsupported_mod_validation() {
        assert_eq!(
            mod_bits(&["CL".into(), "RX".into()]),
            Err("暂不支持 Mod：RX".into())
        );
    }

    #[test]
    fn score_factor_penalizes_misses_and_combo() {
        assert_eq!(score_factor(0, Some(100), Some(100)), 1.0);
        assert!(score_factor(2, Some(50), Some(100)) < 0.6);
    }

    #[test]
    fn mod_attributes_apply_hr_ez_and_rate() {
        let map = rosu_map::Beatmap::from_bytes(
            b"osu file format v14\n\n[General]\nMode: 0\n\n[Metadata]\nTitle: test\nArtist: test\nCreator: test\nVersion: normal\n\n[Difficulty]\nCircleSize: 4\nApproachRate: 8\nOverallDifficulty: 7\n\n[HitObjects]\n256,192,0,1,0,0:0:0:0:\n",
        )
        .expect("fixture");
        let (_, cs, ar, od) = transformed_attributes(&map, 16 | 64);
        assert!((cs - 4.4).abs() < f64::EPSILON);
        assert!((ar - 8.8).abs() < f64::EPSILON);
        assert!((od - 7.7).abs() < 1e-9);
        let (_, ez_cs, _, _) = transformed_attributes(&map, 2 | 256);
        assert_eq!(ez_cs, 2.0);
    }
}
