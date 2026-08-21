use std::collections::{BTreeMap, HashMap, HashSet};

use osu_difficulty_runtime::{
    DifficultyWeights, ManiaBeatmapMetadata, ManiaFeatureRecord, ManiaQueryOptions,
    ManiaQueryResult as RuntimeManiaQueryResult, ManiaQueryTarget as RuntimeManiaQueryTarget,
    QueryFilters, QueryOptions, QueryResponse as RuntimeQueryResponse,
    QueryTarget as RuntimeQueryTarget, RuntimeError, RuntimeErrorKind, WeightingMode,
};

use crate::{
    app::models::Ruleset,
    error::{CommandError, CommandResult},
    similarity::models::{
        ManiaSimilarityBeatmap, ManiaSimilarityRecommendationGroup,
        ManiaSimilarityRecommendationResult, ManiaSimilarityResult, ManiaSimilarityTarget,
        SimilarityBeatmap, SimilarityQueryRequest, SimilarityQueryResponse,
        SimilarityRecommendationKind, SimilarityRecommendationRequest,
        SimilarityRecommendationResponse, SimilarityRecommendationResult, SimilarityResult,
        SimilaritySeedDynamicProfile, SimilarityTarget,
    },
};

pub fn options_from_request(request: &SimilarityQueryRequest) -> CommandResult<QueryOptions> {
    let SimilarityQueryRequest::Osu {
        weighting,
        filters,
        result_limit,
        ..
    } = request
    else {
        return Err(CommandError::new(
            "SIMILARITY_RULESET_MISMATCH",
            "osu!mania 查询不能使用 standard 权重或筛选器",
        ));
    };
    options_from_parts(*weighting, filters, *result_limit)
}

pub fn mania_options_from_request(
    request: &SimilarityQueryRequest,
) -> CommandResult<ManiaQueryOptions> {
    let SimilarityQueryRequest::Mania { result_limit, .. } = request else {
        return Err(CommandError::new(
            "SIMILARITY_RULESET_MISMATCH",
            "osu!standard 查询不能使用 Mania 查询选项",
        ));
    };
    validate_result_limit(*result_limit)?;
    Ok(ManiaQueryOptions {
        result_limit: *result_limit,
        include_same_set: false,
    })
}

pub fn options_from_recommendation_request(
    request: &SimilarityRecommendationRequest,
) -> CommandResult<QueryOptions> {
    let SimilarityRecommendationRequest::Osu {
        weighting,
        filters,
        result_limit,
        excluded_beatmap_ids,
        ..
    } = request
    else {
        return Err(CommandError::new(
            "SIMILARITY_RULESET_MISMATCH",
            "osu!mania 推荐不能使用 standard 权重或筛选器",
        ));
    };
    let mut options = options_from_parts(*weighting, filters, *result_limit)?;
    options.result_limit = expanded_candidate_limit(*result_limit, excluded_beatmap_ids.len());
    Ok(options)
}

pub fn mania_options_from_recommendation_request(
    request: &SimilarityRecommendationRequest,
) -> CommandResult<ManiaQueryOptions> {
    let SimilarityRecommendationRequest::Mania {
        result_limit,
        excluded_beatmap_ids,
        ..
    } = request
    else {
        return Err(CommandError::new(
            "SIMILARITY_RULESET_MISMATCH",
            "osu!standard 推荐不能使用 Mania 查询选项",
        ));
    };
    validate_result_limit(*result_limit)?;
    Ok(ManiaQueryOptions {
        result_limit: expanded_candidate_limit(*result_limit, excluded_beatmap_ids.len()),
        include_same_set: false,
    })
}

fn expanded_candidate_limit(result_limit: usize, excluded_count: usize) -> usize {
    result_limit
        .saturating_add(excluded_count)
        .saturating_mul(3)
        .clamp(50, 150)
}

fn validate_result_limit(result_limit: usize) -> CommandResult<()> {
    if !(5..=50).contains(&result_limit) {
        return Err(CommandError::new(
            "INVALID_RESULT_LIMIT",
            "结果数量必须在 5 到 50 之间",
        ));
    }
    Ok(())
}

fn options_from_parts(
    weighting: WeightingMode,
    filters: &QueryFilters,
    result_limit: usize,
) -> CommandResult<QueryOptions> {
    validate_result_limit(result_limit)?;
    validate_range(filters.min_star, filters.max_star, 0.0, 20.0, "star rating")?;
    validate_range(filters.min_ar, filters.max_ar, 0.0, 11.0, "AR")?;
    validate_range(filters.min_cs, filters.max_cs, 0.0, 10.0, "CS")?;
    validate_range(filters.min_od, filters.max_od, 0.0, 11.0, "OD")?;
    validate_range(filters.min_bpm, filters.max_bpm, 0.0, 1000.0, "BPM")?;
    validate_range(
        filters.min_length_seconds,
        filters.max_length_seconds,
        0.0,
        7200.0,
        "length",
    )?;
    validate_range(
        filters.min_object_density,
        filters.max_object_density,
        0.0,
        100.0,
        "object density",
    )?;
    validate_range(
        filters.min_circle_ratio,
        filters.max_circle_ratio,
        0.0,
        1.0,
        "circle ratio",
    )?;
    validate_range(
        filters.min_slider_ratio,
        filters.max_slider_ratio,
        0.0,
        1.0,
        "slider ratio",
    )?;
    match weighting {
        WeightingMode::Manual {
            difficulty_weights,
            parameter_weight,
        } => validate_weights(difficulty_weights, parameter_weight)?,
        WeightingMode::Dynamic {
            lower_sections,
            upper_sections,
        } => {
            if lower_sections > 20 || upper_sections > 20 {
                return Err(CommandError::new(
                    "INVALID_DYNAMIC_SECTION_RANGE",
                    "动态星数范围必须在 0 到 20 段之间",
                ));
            }
        }
    }
    Ok(QueryOptions {
        weighting,
        filters: filters.clone(),
        result_limit,
    })
}

fn validate_weights(difficulty: DifficultyWeights, parameter_weight: f32) -> CommandResult<()> {
    let weights = [
        difficulty.aim,
        difficulty.speed,
        difficulty.reading,
        difficulty.slider,
        difficulty.overlap,
    ];
    if weights
        .iter()
        .any(|weight| !weight.is_finite() || !(0.0..=2.0).contains(weight))
        || !parameter_weight.is_finite()
        || !(0.0..=2.0).contains(&parameter_weight)
        || (weights.iter().all(|weight| *weight == 0.0) && parameter_weight == 0.0)
    {
        return Err(CommandError::new(
            "INVALID_SIMILARITY_WEIGHTS",
            "权重必须在 0 到 2 之间，且不能全部为 0",
        ));
    }
    Ok(())
}

fn validate_range(
    minimum: Option<f32>,
    maximum: Option<f32>,
    lower: f32,
    upper: f32,
    label: &str,
) -> CommandResult<()> {
    if minimum.is_some_and(|value| !value.is_finite() || value < lower || value > upper)
        || maximum.is_some_and(|value| !value.is_finite() || value < lower || value > upper)
        || minimum.zip(maximum).is_some_and(|(min, max)| min > max)
    {
        return Err(CommandError::new(
            "INVALID_SIMILARITY_FILTER",
            format!("{label} 范围无效"),
        ));
    }
    Ok(())
}

pub fn response_from_runtime(
    target: RuntimeQueryTarget,
    response: RuntimeQueryResponse,
    source: &str,
) -> SimilarityQueryResponse {
    SimilarityQueryResponse::Osu {
        target: SimilarityTarget {
            analyzer_version: target.record.analyzer_version,
            normalization_version: target.record.normalization_version,
            source: source.into(),
            beatmap: beatmap_from_parts(target.metadata, target.record),
        },
        results: response
            .results
            .into_iter()
            .map(|result| SimilarityResult {
                beatmap: beatmap_from_parts(result.metadata, result.record),
                final_distance: result.final_distance,
                difficulty_distance: result.difficulty_distance,
                base_distance: result.base_distance,
            })
            .collect(),
        dynamic_profile: response.weight_profile,
    }
}

pub fn mania_response_from_runtime(
    target: RuntimeManiaQueryTarget,
    results: Vec<RuntimeManiaQueryResult>,
    source: &str,
) -> SimilarityQueryResponse {
    SimilarityQueryResponse::Mania {
        target: ManiaSimilarityTarget {
            analyzer_version: target.record.analyzer_version,
            normalization_version: target.record.normalization_version,
            source: source.into(),
            beatmap: mania_beatmap_from_parts(target.metadata, target.record),
        },
        results: results
            .into_iter()
            .map(|result| ManiaSimilarityResult {
                beatmap: mania_beatmap_from_parts(result.metadata, result.record),
                final_distance: result.final_distance,
                distance_components: result.components,
            })
            .collect(),
    }
}

pub fn recommendation_response_from_runtime(
    kind: SimilarityRecommendationKind,
    batches: Vec<(RuntimeQueryTarget, RuntimeQueryResponse)>,
    skipped_seed_count: usize,
    result_limit: usize,
    excluded_beatmap_ids: &HashSet<u64>,
) -> SimilarityRecommendationResponse {
    let seed_count = batches.len();
    let seed_ids = batches
        .iter()
        .map(|(target, _)| target.record.beatmap_id)
        .collect::<HashSet<_>>();
    let seed_sets = batches
        .iter()
        .map(|(target, _)| target.record.beatmapset_id)
        .filter(|beatmapset_id| *beatmapset_id != 0)
        .collect::<HashSet<_>>();
    let mut nearest_by_set = HashMap::<(bool, u64), SimilarityRecommendationResult>::new();
    let mut dynamic_profiles = Vec::new();

    for (target, response) in batches {
        if let Some(profile) = response.weight_profile {
            dynamic_profiles.push(SimilaritySeedDynamicProfile {
                seed_beatmap_id: target.record.beatmap_id,
                profile,
            });
        }
        let recommended_by = beatmap_from_parts(target.metadata, target.record);
        for result in response.results {
            if excluded_beatmap_ids.contains(&result.record.beatmap_id)
                || seed_ids.contains(&result.record.beatmap_id)
                || (result.record.beatmapset_id != 0
                    && seed_sets.contains(&result.record.beatmapset_id))
            {
                continue;
            }
            let key = recommendation_key(result.record.beatmapset_id, result.record.beatmap_id);
            let recommendation = SimilarityRecommendationResult {
                result: SimilarityResult {
                    beatmap: beatmap_from_parts(result.metadata, result.record),
                    final_distance: result.final_distance,
                    difficulty_distance: result.difficulty_distance,
                    base_distance: result.base_distance,
                },
                recommended_by: recommended_by.clone(),
            };
            if should_replace_standard(nearest_by_set.get(&key), &recommendation) {
                nearest_by_set.insert(key, recommendation);
            }
        }
    }

    let mut results = nearest_by_set.into_values().collect::<Vec<_>>();
    results.sort_by(compare_standard_recommendations);
    results.truncate(result_limit);

    SimilarityRecommendationResponse::Osu {
        kind,
        seed_count,
        skipped_seed_count,
        results,
        dynamic_profiles,
    }
}

pub fn mania_recommendation_response_from_runtime(
    kind: SimilarityRecommendationKind,
    batches: Vec<(RuntimeManiaQueryTarget, Vec<RuntimeManiaQueryResult>)>,
    skipped_seed_count: usize,
    result_limit: usize,
    excluded_beatmap_ids: &HashSet<u64>,
) -> SimilarityRecommendationResponse {
    let seed_count = batches.len();
    let seed_ids = batches
        .iter()
        .map(|(target, _)| target.record.beatmap_id)
        .collect::<HashSet<_>>();
    let seed_sets = batches
        .iter()
        .map(|(target, _)| target.record.beatmapset_id)
        .filter(|beatmapset_id| *beatmapset_id != 0)
        .collect::<HashSet<_>>();
    let mut seeds_by_key = BTreeMap::<u8, usize>::from([(4, 0), (6, 0), (7, 0)]);
    let mut nearest_by_key =
        BTreeMap::<u8, HashMap<(bool, u64), ManiaSimilarityRecommendationResult>>::new();

    for (target, results) in batches {
        let key_count = target.record.key_count;
        *seeds_by_key.entry(key_count).or_default() += 1;
        let recommended_by = mania_beatmap_from_parts(target.metadata, target.record);
        let nearest_by_set = nearest_by_key.entry(key_count).or_default();
        for result in results {
            if result.record.key_count != key_count
                || excluded_beatmap_ids.contains(&result.record.beatmap_id)
                || seed_ids.contains(&result.record.beatmap_id)
                || (result.record.beatmapset_id != 0
                    && seed_sets.contains(&result.record.beatmapset_id))
            {
                continue;
            }
            let key = recommendation_key(result.record.beatmapset_id, result.record.beatmap_id);
            let recommendation = ManiaSimilarityRecommendationResult {
                result: ManiaSimilarityResult {
                    beatmap: mania_beatmap_from_parts(result.metadata, result.record),
                    final_distance: result.final_distance,
                    distance_components: result.components,
                },
                recommended_by: recommended_by.clone(),
            };
            if should_replace_mania(nearest_by_set.get(&key), &recommendation) {
                nearest_by_set.insert(key, recommendation);
            }
        }
    }

    let groups = [4, 6, 7]
        .into_iter()
        .map(|key_count| {
            let mut results = nearest_by_key
                .remove(&key_count)
                .unwrap_or_default()
                .into_values()
                .collect::<Vec<_>>();
            results.sort_by(compare_mania_recommendations);
            results.truncate(result_limit);
            ManiaSimilarityRecommendationGroup {
                key_count,
                seed_count: seeds_by_key.get(&key_count).copied().unwrap_or_default(),
                results,
            }
        })
        .collect();

    SimilarityRecommendationResponse::Mania {
        kind,
        seed_count,
        skipped_seed_count,
        groups,
    }
}

fn recommendation_key(beatmapset_id: u64, beatmap_id: u64) -> (bool, u64) {
    if beatmapset_id == 0 {
        (false, beatmap_id)
    } else {
        (true, beatmapset_id)
    }
}

fn should_replace_standard(
    current: Option<&SimilarityRecommendationResult>,
    candidate: &SimilarityRecommendationResult,
) -> bool {
    current.is_none_or(|current| compare_standard_recommendations(candidate, current).is_lt())
}

fn compare_standard_recommendations(
    left: &SimilarityRecommendationResult,
    right: &SimilarityRecommendationResult,
) -> std::cmp::Ordering {
    left.result
        .final_distance
        .total_cmp(&right.result.final_distance)
        .then_with(|| {
            left.result
                .beatmap
                .beatmap_id
                .cmp(&right.result.beatmap.beatmap_id)
        })
}

fn should_replace_mania(
    current: Option<&ManiaSimilarityRecommendationResult>,
    candidate: &ManiaSimilarityRecommendationResult,
) -> bool {
    current.is_none_or(|current| compare_mania_recommendations(candidate, current).is_lt())
}

fn compare_mania_recommendations(
    left: &ManiaSimilarityRecommendationResult,
    right: &ManiaSimilarityRecommendationResult,
) -> std::cmp::Ordering {
    left.result
        .final_distance
        .total_cmp(&right.result.final_distance)
        .then_with(|| {
            left.result
                .beatmap
                .beatmap_id
                .cmp(&right.result.beatmap.beatmap_id)
        })
}

fn beatmap_from_parts(
    metadata: osu_difficulty_runtime::BeatmapMetadata,
    record: osu_difficulty_runtime::BeatmapFeatureRecord,
) -> SimilarityBeatmap {
    SimilarityBeatmap {
        ruleset: Ruleset::Osu,
        beatmap_id: metadata.beatmap_id,
        beatmapset_id: metadata.beatmapset_id,
        artist: metadata.artist,
        title: metadata.title,
        version: metadata.version,
        creator: metadata.creator,
        online_url: metadata.online_url,
        star_rating: metadata.star_rating,
        difficulty: record.difficulty,
        base: record.base,
    }
}

fn mania_beatmap_from_parts(
    metadata: ManiaBeatmapMetadata,
    record: ManiaFeatureRecord,
) -> ManiaSimilarityBeatmap {
    ManiaSimilarityBeatmap {
        ruleset: Ruleset::Mania,
        beatmap_id: metadata.beatmap_id,
        beatmapset_id: metadata.beatmapset_id,
        artist: metadata.artist,
        title: metadata.title,
        version: metadata.version,
        creator: metadata.creator,
        online_url: metadata.online_url,
        key_count: record.key_count,
        family: record.mode_family,
        pattern: record.dominant_pattern,
        difficulty: record.difficulty,
        style: record.style,
        base: record.base,
        difficulty_percentile: record.difficulty_percentile,
        difficulty_band: record.difficulty_band,
    }
}

pub fn map_runtime_error(error: RuntimeError) -> CommandError {
    // 不把索引的文件系统路径或运行时内部细节暴露给前端。
    match error.kind() {
        RuntimeErrorKind::Invalid => {
            CommandError::new("SIMILARITY_INDEX_INVALID", "本地相似谱面索引损坏或无法读取")
        }
        RuntimeErrorKind::Incompatible => CommandError::new(
            "SIMILARITY_INDEX_INCOMPATIBLE",
            "本地相似谱面索引版本与当前 OPP 不兼容",
        ),
        RuntimeErrorKind::UnknownBeatmap => {
            CommandError::new("BEATMAP_NOT_INDEXED", "目标谱面不在本地索引中")
        }
        RuntimeErrorKind::Analysis => map_analysis_error(error.to_string()),
    }
}

fn map_analysis_error(detail: String) -> CommandError {
    if detail.contains("only osu!standard") || detail.contains("only osu!mania") {
        CommandError::new(
            "BEATMAP_RULESET_MISMATCH",
            "目标谱面的模式与当前相似谱面模式不一致",
        )
    } else if detail.contains("only 4K, 6K, and 7K") {
        CommandError::new(
            "UNSUPPORTED_MANIA_KEY_COUNT",
            "osu!mania 相似谱面首版仅支持 4K、6K 和 7K",
        )
    } else {
        CommandError::new("BEATMAP_ANALYSIS_FAILED", detail)
    }
}

#[cfg(test)]
mod tests {
    use osu_difficulty_runtime::{
        BeatmapFeatureRecord, BeatmapMetadata, DifficultyVector, DifficultyWeights,
        DynamicWeightProfile, ManiaDistanceComponents, ManiaModeFamily, ManiaPattern,
        ParameterVector, QueryResponse as RuntimeQueryResponse, QueryResult as RuntimeQueryResult,
    };

    use super::*;
    use crate::similarity::models::SimilaritySource;

    fn request() -> SimilarityQueryRequest {
        SimilarityQueryRequest::Osu {
            source: SimilaritySource::BeatmapId { value: "1".into() },
            weighting: WeightingMode::Manual {
                difficulty_weights: DifficultyWeights::default(),
                parameter_weight: 1.0,
            },
            filters: QueryFilters::default(),
            result_limit: 20,
        }
    }

    #[test]
    fn request_wire_shape_is_discriminated_by_ruleset() {
        let mania: SimilarityQueryRequest = serde_json::from_value(serde_json::json!({
            "ruleset": "mania",
            "source": { "kind": "beatmap_id", "value": "42" },
            "result_limit": 20
        }))
        .expect("mania request");
        assert_eq!(mania.ruleset(), Ruleset::Mania);
        assert!(matches!(
            mania,
            SimilarityQueryRequest::Mania {
                result_limit: 20,
                ..
            }
        ));
        assert!(
            serde_json::from_value::<SimilarityQueryRequest>(serde_json::json!({
                "ruleset": "mania",
                "source": { "kind": "beatmap_id", "value": "42" },
                "weighting": { "kind": "dynamic", "lower_sections": 4, "upper_sections": 4 },
                "result_limit": 20
            }))
            .is_err(),
            "Mania must not accept standard-only weighting fields"
        );
    }

    #[test]
    fn mania_recommendation_wire_shape_rejects_standard_only_fields() {
        let valid = serde_json::json!({
            "ruleset": "mania",
            "kind": "recent",
            "result_limit": 20,
            "seed_limit": 10,
            "excluded_beatmap_ids": [1, 2]
        });
        assert!(serde_json::from_value::<SimilarityRecommendationRequest>(valid.clone()).is_ok());
        let mut invalid = valid;
        invalid["filters"] = serde_json::json!({});
        assert!(
            serde_json::from_value::<SimilarityRecommendationRequest>(invalid).is_err(),
            "Mania recommendation must not accept standard-only filters"
        );
    }

    #[test]
    fn rejects_inverted_filters() {
        let mut request = request();
        let SimilarityQueryRequest::Osu { filters, .. } = &mut request else {
            unreachable!()
        };
        filters.min_ar = Some(10.0);
        filters.max_ar = Some(8.0);
        assert!(options_from_request(&request).is_err());
    }

    #[test]
    fn rejects_zero_difficulty_weights() {
        let mut request = request();
        let SimilarityQueryRequest::Osu { weighting, .. } = &mut request else {
            unreachable!()
        };
        *weighting = WeightingMode::Manual {
            difficulty_weights: DifficultyWeights {
                aim: 0.0,
                speed: 0.0,
                reading: 0.0,
                slider: 0.0,
                overlap: 0.0,
            },
            parameter_weight: 0.0,
        };
        assert!(options_from_request(&request).is_err());
    }

    #[test]
    fn accepts_parameter_only_manual_weighting() {
        let mut request = request();
        let SimilarityQueryRequest::Osu { weighting, .. } = &mut request else {
            unreachable!()
        };
        *weighting = WeightingMode::Manual {
            difficulty_weights: DifficultyWeights::from_array([0.0; 5]),
            parameter_weight: 1.0,
        };
        assert!(options_from_request(&request).is_ok());
    }

    #[test]
    fn rejects_inverted_star_filter() {
        let mut request = request();
        let SimilarityQueryRequest::Osu { filters, .. } = &mut request else {
            unreachable!()
        };
        filters.min_star = Some(6.5);
        filters.max_star = Some(5.7);
        assert!(options_from_request(&request).is_err());
    }

    #[test]
    fn mania_options_have_no_standard_weighting_or_filters() {
        let request = SimilarityQueryRequest::Mania {
            source: SimilaritySource::BeatmapId { value: "1".into() },
            result_limit: 20,
        };
        let options = mania_options_from_request(&request).expect("mania options");
        assert_eq!(options.result_limit, 20);
        assert!(!options.include_same_set);
        assert!(options_from_request(&request).is_err());
    }

    #[test]
    fn analysis_errors_distinguish_ruleset_and_key_count_mismatches() {
        assert_eq!(
            map_analysis_error("only osu!mania mode is supported (found mode 0)".into()).code,
            "BEATMAP_RULESET_MISMATCH"
        );
        assert_eq!(
            map_analysis_error("only 4K, 6K, and 7K are supported (found 5K)".into()).code,
            "UNSUPPORTED_MANIA_KEY_COUNT"
        );
    }

    #[test]
    fn recommendation_uses_a_larger_internal_candidate_pool_per_ruleset() {
        let standard = SimilarityRecommendationRequest::Osu {
            kind: SimilarityRecommendationKind::Recent,
            weighting: WeightingMode::default(),
            filters: QueryFilters::default(),
            result_limit: 20,
            seed_limit: None,
            excluded_beatmap_ids: Vec::new(),
        };
        assert_eq!(
            options_from_recommendation_request(&standard)
                .expect("recommendation options")
                .result_limit,
            60
        );
        let mania = SimilarityRecommendationRequest::Mania {
            kind: SimilarityRecommendationKind::Recent,
            result_limit: 20,
            seed_limit: None,
            excluded_beatmap_ids: Vec::new(),
        };
        assert_eq!(
            mania_options_from_recommendation_request(&mania)
                .expect("mania recommendation options")
                .result_limit,
            60
        );
    }

    fn dynamic_profile() -> DynamicWeightProfile {
        DynamicWeightProfile {
            target_star_rating: 6.1,
            candidate_min_section: 57,
            candidate_max_section: 65,
            stats_min_section: 57,
            stats_max_section: 65,
            sample_count: 200,
            mean: DifficultyVector::default(),
            stddev: DifficultyVector::default(),
            delta: DifficultyVector::default(),
            z_score: DifficultyVector::default(),
            weights: DifficultyWeights::default(),
            parameter_mean: ParameterVector::default(),
            parameter_stddev: ParameterVector::default(),
            parameter_delta: ParameterVector::default(),
            parameter_z_score: ParameterVector::default(),
            parameter_group_z_score: 0.0,
            parameter_weight: 1.0,
            fallback_reason: None,
        }
    }

    fn target(id: u64, set: u64) -> RuntimeQueryTarget {
        RuntimeQueryTarget {
            metadata: metadata(id, set),
            record: record(id, set),
        }
    }

    fn result(id: u64, set: u64, distance: f32) -> RuntimeQueryResult {
        RuntimeQueryResult {
            metadata: metadata(id, set),
            record: record(id, set),
            final_distance: distance,
            difficulty_distance: distance,
            base_distance: 0.0,
        }
    }

    fn metadata(id: u64, set: u64) -> BeatmapMetadata {
        BeatmapMetadata {
            beatmap_id: id,
            beatmapset_id: set,
            checksum: format!("checksum-{id}"),
            artist: format!("Artist {id}"),
            title: format!("Title {id}"),
            version: "Insane".into(),
            creator: "Mapper".into(),
            online_url: format!("https://osu.ppy.sh/beatmaps/{id}"),
            star_rating: Some(6.1),
        }
    }

    fn record(id: u64, set: u64) -> BeatmapFeatureRecord {
        BeatmapFeatureRecord {
            beatmap_id: id,
            beatmapset_id: set,
            ..BeatmapFeatureRecord::default()
        }
    }

    #[test]
    fn standard_recommendation_keeps_nearest_source_and_excludes_all_seed_sets() {
        let response = recommendation_response_from_runtime(
            SimilarityRecommendationKind::Recent,
            vec![
                (
                    target(1, 10),
                    RuntimeQueryResponse {
                        results: vec![
                            result(100, 100, 0.4),
                            result(101, 101, 0.2),
                            result(2, 20, 0.1),
                        ],
                        weight_profile: Some(dynamic_profile()),
                    },
                ),
                (
                    target(2, 20),
                    RuntimeQueryResponse {
                        results: vec![result(102, 100, 0.1), result(1, 10, 0.05)],
                        weight_profile: None,
                    },
                ),
            ],
            3,
            20,
            &HashSet::new(),
        );

        let SimilarityRecommendationResponse::Osu {
            seed_count,
            skipped_seed_count,
            results,
            dynamic_profiles,
            ..
        } = response
        else {
            panic!("standard response")
        };
        assert_eq!(seed_count, 2);
        assert_eq!(skipped_seed_count, 3);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.beatmap.beatmap_id, 102);
        assert_eq!(results[0].recommended_by.beatmap_id, 2);
        assert_eq!(dynamic_profiles.len(), 1);
    }

    #[test]
    fn recommendation_excludes_history_before_truncating_results() {
        let response = recommendation_response_from_runtime(
            SimilarityRecommendationKind::Recent,
            vec![(
                target(1, 10),
                RuntimeQueryResponse {
                    results: vec![result(100, 100, 0.1), result(101, 101, 0.2)],
                    weight_profile: None,
                },
            )],
            0,
            1,
            &HashSet::from([100]),
        );

        let SimilarityRecommendationResponse::Osu { results, .. } = response else {
            panic!("standard response")
        };
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.beatmap.beatmap_id, 101);
    }

    fn mania_target(id: u64, set: u64, key_count: u8) -> RuntimeManiaQueryTarget {
        RuntimeManiaQueryTarget {
            metadata: mania_metadata(id, set, key_count),
            record: mania_record(id, set, key_count),
        }
    }

    fn mania_result(id: u64, set: u64, key_count: u8, distance: f32) -> RuntimeManiaQueryResult {
        RuntimeManiaQueryResult {
            metadata: mania_metadata(id, set, key_count),
            record: mania_record(id, set, key_count),
            final_distance: distance,
            components: ManiaDistanceComponents {
                skill: distance,
                ..ManiaDistanceComponents::default()
            },
        }
    }

    fn mania_metadata(id: u64, set: u64, key_count: u8) -> ManiaBeatmapMetadata {
        ManiaBeatmapMetadata {
            beatmap_id: id,
            beatmapset_id: set,
            checksum: format!("checksum-{id}"),
            artist: format!("Artist {id}"),
            title: format!("Title {id}"),
            version: "MX".into(),
            creator: "Mapper".into(),
            online_url: format!("https://osu.ppy.sh/beatmaps/{id}"),
            key_count,
            mode_family: ManiaModeFamily::Rc,
            dominant_pattern: ManiaPattern::Stream,
        }
    }

    fn mania_record(id: u64, set: u64, key_count: u8) -> ManiaFeatureRecord {
        ManiaFeatureRecord {
            beatmap_id: id,
            beatmapset_id: set,
            key_count,
            ..ManiaFeatureRecord::default()
        }
    }

    #[test]
    fn mania_recommendations_group_independently_and_keep_nearest_source() {
        let response = mania_recommendation_response_from_runtime(
            SimilarityRecommendationKind::Best,
            vec![
                (
                    mania_target(1, 10, 4),
                    vec![
                        mania_result(100, 100, 4, 0.4),
                        mania_result(101, 101, 4, 0.2),
                        mania_result(3, 30, 4, 0.01),
                    ],
                ),
                (mania_target(2, 20, 4), vec![mania_result(102, 100, 4, 0.1)]),
                (
                    mania_target(3, 30, 7),
                    vec![mania_result(200, 200, 7, 0.05)],
                ),
            ],
            2,
            1,
            &HashSet::new(),
        );

        let SimilarityRecommendationResponse::Mania {
            seed_count,
            skipped_seed_count,
            groups,
            ..
        } = response
        else {
            panic!("mania response")
        };
        assert_eq!(seed_count, 3);
        assert_eq!(skipped_seed_count, 2);
        assert_eq!(
            groups
                .iter()
                .map(|group| group.key_count)
                .collect::<Vec<_>>(),
            vec![4, 6, 7]
        );
        let four = &groups[0];
        assert_eq!(four.seed_count, 2);
        assert_eq!(four.results.len(), 1);
        assert_eq!(four.results[0].result.beatmap.beatmap_id, 102);
        assert_eq!(four.results[0].recommended_by.beatmap_id, 2);
        assert_eq!(groups[1].seed_count, 0);
        assert!(groups[1].results.is_empty());
        assert_eq!(groups[2].results[0].result.beatmap.beatmap_id, 200);
    }

    #[test]
    fn zero_set_ids_do_not_collapse_unrelated_recommendations() {
        let response = mania_recommendation_response_from_runtime(
            SimilarityRecommendationKind::Recent,
            vec![(
                mania_target(1, 10, 4),
                vec![mania_result(100, 0, 4, 0.1), mania_result(101, 0, 4, 0.2)],
            )],
            0,
            20,
            &HashSet::new(),
        );
        let SimilarityRecommendationResponse::Mania { groups, .. } = response else {
            panic!("mania response")
        };
        assert_eq!(groups[0].results.len(), 2);
    }
}
