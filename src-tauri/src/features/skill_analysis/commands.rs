use std::{collections::HashSet, fs};

use chrono::{Duration, Utc};
use serde_json::Value;
use tauri::State;

use crate::{
    domain::{CacheRecord, OwnProfile, Ruleset, Score, ScoreCategory},
    error::{CommandError, CommandResult},
    features::{
        account::{get_own_profile, get_scores},
        similarity::source::fetch_online_osu,
    },
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

use super::{
    algorithm::{self, ALGORITHM_ID, ALGORITHM_SOURCE, ALGORITHM_VERSION},
    models::{
        AlgorithmInfo, PlayerSummary, SkillAnalysisRequest, SkillAnalysisResult, SkillContribution,
        SkillCoverage, SkillVector,
    },
};

const CACHE_SECONDS: i64 = 600;
// Invalidate results that skipped Classic scores before CL support was added.
const CACHE_REVISION: &str = "2";
const MAX_LIMIT: usize = 200;

#[tauri::command]
pub async fn analyze_player_skills(
    request: SkillAnalysisRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillAnalysisResult> {
    let span = global().map(|logger| logger.operation("skill_analysis", "analyze_player_skills"));
    let can_fallback =
        request.force_refresh && request.ruleset == "osu" && request.score_limit == MAX_LIMIT;
    let fallback = if can_fallback {
        let key = format!(
            "skill-analysis:{}:{}:{}:{CACHE_REVISION}",
            request.client, request.ruleset, ALGORITHM_VERSION
        );
        let store = state.store.clone();
        crate::infrastructure::tasks::blocking_io("read_skill_fallback", move || {
            store.read_cached(|saved| saved.cache.get(&key).cloned())
        })
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
    } else {
        None
    };
    let result = analyze_player_skills_inner(request, state, span.as_ref()).await;
    let result = match result {
        Err(error) if can_fallback && fallback.is_some() => {
            if let Some(span) = span.as_ref() {
                span.warn(
                    "刷新失败，返回过期技能分析缓存",
                    Some(serde_json::json!({ "code": error.code })),
                );
            }
            fallback
                .and_then(|record| serde_json::from_value::<SkillAnalysisResult>(record.value).ok())
                .map(|mut cached| {
                    cached.stale = true;
                    cached
                })
                .ok_or(error)
        }
        other => other,
    };
    finish_span(span, result)
}

async fn analyze_player_skills_inner(
    request: SkillAnalysisRequest,
    state: State<'_, AppState>,
    span: Option<&crate::infrastructure::logging::LogSpan>,
) -> CommandResult<SkillAnalysisResult> {
    if request.ruleset != "osu" {
        return Err(CommandError::new(
            "SKILL_RULESET_UNSUPPORTED",
            "技能分析首版仅支持 osu! standard",
        ));
    }
    if request.score_limit != MAX_LIMIT {
        return Err(CommandError::new(
            "INVALID_SKILL_SCORE_LIMIT",
            "技能分析首版固定分析最佳成绩前 200",
        ));
    }

    let cache_key = format!(
        "skill-analysis:{}:{}:{}:{CACHE_REVISION}",
        request.client, request.ruleset, ALGORITHM_VERSION
    );
    let store = state.store.clone();
    let key = cache_key.clone();
    let cached = crate::infrastructure::tasks::blocking_io("read_skill_cache", move || {
        store.read_cached(|saved| saved.cache.get(&key).cloned())
    })
    .await??;
    if !request.force_refresh
        && let Some(record) = cached.as_ref()
        && Utc::now() - record.fetched_at < Duration::seconds(CACHE_SECONDS)
    {
        if let Some(span) = span {
            span.info(
                "命中技能分析缓存",
                Some(serde_json::json!({ "cache_seconds": CACHE_SECONDS })),
            );
        }
        let mut result: SkillAnalysisResult = serde_json::from_value(record.value.clone())?;
        result.stale = false;
        return Ok(result);
    }

    let profile = get_own_profile(Ruleset::Osu, request.force_refresh, state.clone())
        .await?
        .data;
    let mut scores = Vec::with_capacity(MAX_LIMIT);
    for offset in [0_u32, 100_u32] {
        if let Some(span) = span {
            span.info(
                "读取官方最佳成绩分页",
                Some(serde_json::json!({ "offset": offset, "limit": 100 })),
            );
        }
        let page = get_scores(
            Ruleset::Osu,
            ScoreCategory::Best,
            offset,
            100,
            request.force_refresh,
            state.clone(),
        )
        .await?;
        scores.extend(page.data);
    }
    scores.truncate(MAX_LIMIT);

    let result = analyze_scores(&state, &profile, &scores, &request, span).await?;
    let value = serde_json::to_value(&result)?;
    let store = state.store.clone();
    crate::infrastructure::tasks::blocking_io("write_skill_cache", move || {
        store.insert_cache(
            cache_key,
            CacheRecord {
                value,
                fetched_at: Utc::now(),
            },
            None,
        )
    })
    .await??;
    Ok(result)
}

async fn analyze_scores(
    state: &AppState,
    profile: &OwnProfile,
    scores: &[Score],
    request: &SkillAnalysisRequest,
    span: Option<&crate::infrastructure::logging::LogSpan>,
) -> CommandResult<SkillAnalysisResult> {
    let mut contributions = Vec::with_capacity(scores.len());
    let mut seen = HashSet::new();
    let mut coverage = SkillCoverage {
        requested_scores: scores.len(),
        analyzed_scores: 0,
        local_scores: 0,
        online_scores: 0,
        skipped_scores: 0,
        skipped_reasons: Vec::new(),
    };

    for (index, score) in scores.iter().enumerate() {
        let Some(beatmap_id) = score
            .beatmap
            .as_ref()
            .and_then(|map| map.get("id"))
            .and_then(Value::as_u64)
        else {
            coverage.skipped_scores += 1;
            coverage
                .skipped_reasons
                .push(format!("成绩 {} 缺少 Beatmap ID", index + 1));
            continue;
        };
        if !seen.insert(beatmap_id) {
            coverage.skipped_scores += 1;
            coverage
                .skipped_reasons
                .push(format!("成绩 {} 重复 Beatmap ID {beatmap_id}", index + 1));
            continue;
        }
        let mods = score_mods(score);
        let mut source = "local";
        let mut resource_id = None;
        let bytes = match state
            .local_analysis
            .beatmap_path_by_id(request.client, beatmap_id as i32)
        {
            Ok(Some((path, id))) => {
                resource_id = Some(id);
                if let Some(span) = span {
                    span.info(
                        "使用本地谱面",
                        Some(serde_json::json!({ "beatmap_id": beatmap_id })),
                    );
                }
                match fs::read(path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        coverage.skipped_scores += 1;
                        coverage
                            .skipped_reasons
                            .push(format!("谱面 {beatmap_id} 本地读取失败：{error}"));
                        continue;
                    }
                }
            }
            Ok(None) if request.include_online => {
                source = "online";
                if let Some(span) = span {
                    span.info(
                        "本地未命中，获取在线谱面",
                        Some(serde_json::json!({ "beatmap_id": beatmap_id })),
                    );
                }
                match fetch_online_osu(&state.providers, beatmap_id).await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        coverage.skipped_scores += 1;
                        coverage.skipped_reasons.push(error.message);
                        continue;
                    }
                }
            }
            Ok(None) => {
                coverage.skipped_scores += 1;
                coverage
                    .skipped_reasons
                    .push(format!("谱面 {beatmap_id} 不在本地"));
                continue;
            }
            Err(_) if request.include_online => {
                source = "online";
                if let Some(span) = span {
                    span.info(
                        "本地索引不可用，获取在线谱面",
                        Some(serde_json::json!({ "beatmap_id": beatmap_id })),
                    );
                }
                match fetch_online_osu(&state.providers, beatmap_id).await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        coverage.skipped_scores += 1;
                        coverage.skipped_reasons.push(error.message);
                        continue;
                    }
                }
            }
            Err(error) => {
                coverage.skipped_scores += 1;
                coverage.skipped_reasons.push(error.message);
                continue;
            }
        };

        let calculated = crate::infrastructure::tasks::background("skill_analysis", {
            let bytes = bytes.clone();
            let mods = mods.clone();
            move || algorithm::calculate(&bytes, &mods)
        })
        .await
        .map_err(|error| {
            CommandError::new(
                "SKILL_CALCULATION_FAILED",
                format!("技能计算线程失败：{error}"),
            )
        })?;
        let (skills, map) = match calculated {
            Ok(value) => value,
            Err(error) => {
                if let Some(span) = span {
                    span.warn(
                        "谱面解析或技能计算失败",
                        Some(serde_json::json!({ "beatmap_id": beatmap_id, "error": error })),
                    );
                }
                coverage.skipped_scores += 1;
                coverage
                    .skipped_reasons
                    .push(format!("谱面 {beatmap_id}：{error}"));
                continue;
            }
        };
        let misses = score
            .statistics
            .get("count_miss")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let max_combo = score.max_combo.or_else(|| {
            let pp_map = rosu_pp::Beatmap::from_bytes(&bytes).ok()?;
            Some(rosu_pp::Difficulty::new().calculate(&pp_map).max_combo() as u64)
        });
        let factor = algorithm::score_factor(misses, score.max_combo, max_combo);
        let weight = score
            .weight
            .as_ref()
            .and_then(|value| value.get("percentage"))
            .and_then(Value::as_f64)
            .unwrap_or_else(|| 0.95_f64.powi(index as i32))
            .max(0.0001);
        let mut weighted = skills.clone();
        weighted.scale_score_factor(factor);
        let meta = score.beatmap.as_ref();
        let contribution = SkillContribution {
            beatmap_id,
            title: meta
                .and_then(|v| v.get("beatmapset").and_then(|s| s.get("title")))
                .and_then(Value::as_str)
                .unwrap_or(&map.title)
                .to_string(),
            artist: meta
                .and_then(|v| v.get("beatmapset").and_then(|s| s.get("artist")))
                .and_then(Value::as_str)
                .unwrap_or(&map.artist)
                .to_string(),
            version: meta
                .and_then(|v| v.get("version"))
                .and_then(Value::as_str)
                .unwrap_or(&map.version)
                .to_string(),
            creator: meta
                .and_then(|v| v.get("beatmapset").and_then(|s| s.get("creator")))
                .and_then(Value::as_str)
                .unwrap_or(&map.creator)
                .to_string(),
            mods,
            pp: score.pp,
            accuracy: score.accuracy,
            combo: score.max_combo,
            max_combo,
            misses,
            weight,
            source: source.into(),
            resource_id,
            skills,
            weighted_skills: weighted,
            error: None,
        };
        if source == "local" {
            coverage.local_scores += 1;
        } else {
            coverage.online_scores += 1;
        }
        coverage.analyzed_scores += 1;
        contributions.push(contribution);
    }

    let mut skills = SkillVector::default();
    let mut total_weight = 0.0;
    for contribution in &contributions {
        skills.add_weighted(&contribution.weighted_skills, contribution.weight);
        total_weight += contribution.weight;
    }
    if total_weight > 0.0 {
        skills.scale(1.0 / total_weight);
    }
    let fetched_at = Utc::now().to_rfc3339();
    if let Some(span) = span {
        span.info(
            "技能分析计算完成",
            Some(serde_json::json!({
                "requested_scores": coverage.requested_scores,
                "analyzed_scores": coverage.analyzed_scores,
                "local_scores": coverage.local_scores,
                "online_scores": coverage.online_scores,
                "skipped_scores": coverage.skipped_scores,
            })),
        );
    }
    Ok(SkillAnalysisResult {
        player: PlayerSummary {
            id: profile.id,
            username: profile.username.clone(),
            country_code: profile.country_code.clone(),
            avatar_url: profile.avatar_url.clone(),
            avatar_data_url: profile.avatar_data_url.clone(),
        },
        ruleset: "osu".into(),
        algorithm: AlgorithmInfo {
            id: ALGORITHM_ID.into(),
            version: ALGORITHM_VERSION.into(),
            source: ALGORITHM_SOURCE.into(),
        },
        skills,
        contributions,
        coverage,
        fetched_at,
        stale: false,
    })
}

fn score_mods(score: &Score) -> Vec<String> {
    score
        .mods
        .iter()
        .filter_map(|value| match value {
            Value::String(value) => Some(value.to_ascii_uppercase()),
            Value::Object(value) => value
                .get("acronym")
                .and_then(Value::as_str)
                .map(|value| value.to_ascii_uppercase()),
            _ => None,
        })
        .collect()
}
