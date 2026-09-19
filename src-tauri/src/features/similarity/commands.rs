use std::{collections::HashSet, sync::Arc};

use osu_difficulty_runtime::{Dataset, ManiaDataset, ManiaGameMod};
use tauri::State;

use crate::{
    domain::{AppSettings, Ruleset, Score},
    error::{CommandError, CommandResult},
    features::{
        account::ensure_access_token,
        similarity::{
            models::{
                SimilarityIndexStatus, SimilarityQueryRequest, SimilarityQueryResponse,
                SimilarityRecommendationKind, SimilarityRecommendationRequest,
                SimilarityRecommendationResponse, SimilaritySource,
            },
            query::{
                mania_options_from_recommendation_request, mania_options_from_request,
                mania_recommendation_response_from_runtime, mania_response_from_runtime,
                map_runtime_error, options_from_recommendation_request, options_from_request,
                recommendation_response_from_runtime, response_from_runtime,
            },
            recommendation::{
                ManiaSeed, ManiaSeedSelection, mania_seed_ids, requested_seed_limit, seed_ids,
            },
            source::{fetch_online_osu, parse_beatmap_id, read_local_osu},
        },
    },
    state::AppState,
};

#[tauri::command]
/// 返回指定模式的相似谱面索引状态。Taiko 与 Catch 会明确返回 unsupported。
pub async fn get_similarity_index_status(
    ruleset: Ruleset,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityIndexStatus> {
    if matches!(ruleset, Ruleset::Taiko | Ruleset::Fruits) {
        return Ok(SimilarityIndexStatus::unsupported(ruleset));
    }
    let directory = configured_directory(&state, ruleset)?;
    state.similarity.clear(ruleset);
    inspect(state.similarity.clone(), ruleset, directory).await
}

#[tauri::command]
/// 分模式保存相似谱面索引目录；standard 与 Mania 的配置和缓存互不覆盖。
pub async fn configure_similarity_index(
    ruleset: Ruleset,
    directory: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityIndexStatus> {
    if matches!(ruleset, Ruleset::Taiko | Ruleset::Fruits) {
        return Ok(SimilarityIndexStatus::unsupported(ruleset));
    }
    let directory = directory
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    state.store.update(|persisted| {
        set_configured_directory(&mut persisted.settings, ruleset, directory.clone())
    })??;
    state.similarity.clear(ruleset);
    inspect(state.similarity.clone(), ruleset, directory).await
}

#[tauri::command]
/// 按请求的 ruleset 路由到互相隔离的 standard 或 Mania 运行时。
pub async fn query_similar_beatmaps(
    request: SimilarityQueryRequest,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityQueryResponse> {
    let directory = required_directory(&state, request.ruleset())?;
    match request {
        request @ SimilarityQueryRequest::Osu { .. } => {
            query_standard(request, directory, &state).await
        }
        request @ SimilarityQueryRequest::Mania { .. } => {
            query_mania(request, directory, &state).await
        }
    }
}

async fn query_standard(
    request: SimilarityQueryRequest,
    directory: String,
    state: &AppState,
) -> CommandResult<SimilarityQueryResponse> {
    let options = options_from_request(&request)?;
    let dataset = load_standard_dataset(state.similarity.clone(), directory).await?;
    let (indexed_id, bytes, source_label) =
        resolve_standard_source(request.source(), state, &dataset).await?;

    crate::infrastructure::tasks::background("similarity", move || {
        let target = if let Some(beatmap_id) = indexed_id {
            dataset.target_for_id(beatmap_id)
        } else {
            dataset.analyze_target(bytes.as_deref().unwrap_or_default())
        }
        .map_err(map_runtime_error)?;
        let response = dataset
            .query_with_profile(&target, &options)
            .map_err(map_runtime_error)?;
        Ok(response_from_runtime(target, response, source_label))
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "相似谱面查询任务意外停止"))?
}

async fn query_mania(
    request: SimilarityQueryRequest,
    directory: String,
    state: &AppState,
) -> CommandResult<SimilarityQueryResponse> {
    let options = mania_options_from_request(&request)?;
    let SimilarityQueryRequest::Mania { target_mod, .. } = &request else {
        unreachable!("routed Mania request")
    };
    let target_mod = *target_mod;
    let dataset = load_mania_dataset(state.similarity.clone(), directory).await?;
    let (indexed_id, bytes, source_beatmap_id, source_label) =
        resolve_mania_source(request.source(), state, &dataset, target_mod).await?;

    crate::infrastructure::tasks::background("similarity", move || {
        let target = if let Some(beatmap_id) = indexed_id {
            dataset.target_for_id_with_mod(beatmap_id, target_mod)
        } else {
            dataset.analyze_target_with_mod(
                bytes.as_deref().unwrap_or_default(),
                source_beatmap_id,
                target_mod,
            )
        }
        .map_err(map_runtime_error)?;
        let results = dataset
            .query(&target, &options)
            .map_err(map_runtime_error)?;
        Ok(mania_response_from_runtime(target, results, source_label))
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "Mania 相似谱面查询任务意外停止"))?
}

#[tauri::command]
/// 使用请求中的 ruleset 获取成绩，并按对应运行时生成推荐。
pub async fn recommend_similar_beatmaps(
    request: SimilarityRecommendationRequest,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityRecommendationResponse> {
    let ruleset = request.ruleset();
    let directory = required_directory(&state, ruleset)?;
    let kind = request.kind();
    match &request {
        SimilarityRecommendationRequest::Osu { .. } => {
            options_from_recommendation_request(&request)?;
        }
        SimilarityRecommendationRequest::Mania { .. } => {
            mania_options_from_recommendation_request(&request)?;
        }
    }
    let access_token = ensure_access_token(&state).await?;
    let profile = state.api.get_own_profile(&access_token, ruleset).await?;
    let scores = match kind {
        SimilarityRecommendationKind::Recent => {
            state
                .api
                .get_recent_scores(&access_token, profile.id, ruleset)
                .await?
        }
        SimilarityRecommendationKind::Best => {
            state
                .api
                .get_best_scores(&access_token, profile.id, ruleset)
                .await?
        }
    };
    let requested_seed_limit = requested_seed_limit(request.seed_limit());
    // 参考成绩与候选谱面使用同一个 Mod 池，筛选必须发生在截取参考数量之前。
    let mania_mod_pool = if ruleset == Ruleset::Mania {
        Some(mania_options_from_recommendation_request(&request)?.candidate_mods)
    } else {
        None
    };
    let mania_selection = mania_mod_pool
        .as_deref()
        .map(|pool| mania_seed_ids(&scores, requested_seed_limit, pool));
    let (standard_seed_ids, mania_seeds, initially_skipped_seed_count) = match &mania_selection {
        Some(selection) => (
            Vec::new(),
            selection.seeds.clone(),
            selection.unusable_mod_scores,
        ),
        None => (seed_ids(&scores, requested_seed_limit), Vec::new(), 0),
    };
    if standard_seed_ids.is_empty() && mania_seeds.is_empty() {
        if let (Some(pool), Some(selection)) = (mania_mod_pool.as_deref(), mania_selection.as_ref())
        {
            return Err(CommandError::new(
                "NO_RECOMMENDATION_SEEDS",
                mania_seed_error_message(kind, &scores, pool, selection),
            ));
        }
        return Err(CommandError::new(
            "NO_RECOMMENDATION_SEEDS",
            match kind {
                SimilarityRecommendationKind::Recent => "没有可用于推荐的最近通过成绩",
                SimilarityRecommendationKind::Best => "没有可用于推荐的 BP 成绩",
            },
        ));
    }

    match request {
        request @ SimilarityRecommendationRequest::Osu { .. } => {
            recommend_standard(request, standard_seed_ids, directory, &state).await
        }
        request @ SimilarityRecommendationRequest::Mania { .. } => {
            recommend_mania(
                request,
                mania_seeds,
                initially_skipped_seed_count,
                directory,
                &state,
            )
            .await
        }
    }
}

async fn recommend_standard(
    request: SimilarityRecommendationRequest,
    seed_ids: Vec<u64>,
    directory: String,
    state: &AppState,
) -> CommandResult<SimilarityRecommendationResponse> {
    let dataset = load_standard_dataset(state.similarity.clone(), directory).await?;
    let options = options_from_recommendation_request(&request)?;
    let mut targets = Vec::with_capacity(seed_ids.len());
    let mut skipped_seed_count = 0;
    for beatmap_id in seed_ids {
        let target = if dataset.contains(beatmap_id) {
            dataset.target_for_id(beatmap_id).map_err(map_runtime_error)
        } else {
            match fetch_online_osu(&state.providers, beatmap_id).await {
                Ok(bytes) => dataset.analyze_target(&bytes).map_err(map_runtime_error),
                Err(_) => {
                    skipped_seed_count += 1;
                    continue;
                }
            }
        };
        match target {
            Ok(target) => targets.push(target),
            Err(_) => skipped_seed_count += 1,
        }
    }
    if targets.is_empty() {
        return Err(no_usable_seed_error(Ruleset::Osu));
    }

    let kind = request.kind();
    let final_result_limit = request.result_limit();
    let excluded_beatmap_ids = request
        .excluded_beatmap_ids()
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    crate::infrastructure::tasks::background("similarity", move || {
        let mut batches = Vec::with_capacity(targets.len());
        for target in targets {
            let response = dataset
                .query_with_profile(&target, &options)
                .map_err(map_runtime_error)?;
            batches.push((target, response));
        }
        Ok(recommendation_response_from_runtime(
            kind,
            batches,
            skipped_seed_count,
            final_result_limit,
            &excluded_beatmap_ids,
        ))
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "推荐谱面查询任务意外停止"))?
}

async fn recommend_mania(
    request: SimilarityRecommendationRequest,
    seeds: Vec<ManiaSeed>,
    initially_skipped_seed_count: usize,
    directory: String,
    state: &AppState,
) -> CommandResult<SimilarityRecommendationResponse> {
    let dataset = load_mania_dataset(state.similarity.clone(), directory).await?;
    let options = mania_options_from_recommendation_request(&request)?;
    let mut targets = Vec::with_capacity(seeds.len());
    let mut skipped_seed_count = initially_skipped_seed_count;
    for seed in seeds {
        let target = if dataset.contains_mod(seed.beatmap_id, seed.game_mod) {
            dataset
                .target_for_id_with_mod(seed.beatmap_id, seed.game_mod)
                .map_err(map_runtime_error)
        } else {
            match fetch_online_osu(&state.providers, seed.beatmap_id).await {
                // 下载文件中的旧 BeatmapID 可能错误，成绩中的 ID 才是权威来源。
                Ok(bytes) => dataset
                    .analyze_target_with_mod(&bytes, Some(seed.beatmap_id), seed.game_mod)
                    .map_err(map_runtime_error),
                Err(_) => {
                    skipped_seed_count += 1;
                    continue;
                }
            }
        };
        match target {
            Ok(target) if matches!(target.record.key_count, 4 | 6 | 7) => targets.push(target),
            Ok(_) | Err(_) => skipped_seed_count += 1,
        }
    }
    if targets.is_empty() {
        return Err(no_usable_seed_error(Ruleset::Mania));
    }

    let kind = request.kind();
    let final_result_limit = request.result_limit();
    let excluded_beatmap_ids = request
        .excluded_beatmap_ids()
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    crate::infrastructure::tasks::background("similarity", move || {
        let mut batches = Vec::with_capacity(targets.len());
        for target in targets {
            let results = dataset
                .query(&target, &options)
                .map_err(map_runtime_error)?;
            batches.push((target, results));
        }
        Ok(mania_recommendation_response_from_runtime(
            kind,
            batches,
            skipped_seed_count,
            final_result_limit,
            &excluded_beatmap_ids,
        ))
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "Mania 推荐查询任务意外停止"))?
}

async fn resolve_standard_source(
    source: &SimilaritySource,
    state: &AppState,
    dataset: &Dataset,
) -> CommandResult<(Option<u64>, Option<Vec<u8>>, &'static str)> {
    match source {
        SimilaritySource::BeatmapId { value } => {
            let beatmap_id = parse_beatmap_id(value)?;
            if dataset.contains(beatmap_id) {
                Ok((Some(beatmap_id), None, "index"))
            } else {
                let bytes = fetch_online_osu(&state.providers, beatmap_id).await?;
                Ok((None, Some(bytes), "online"))
            }
        }
        SimilaritySource::LocalFile { path } => {
            let bytes = read_local_source(path.clone()).await?;
            Ok((None, Some(bytes), "local_file"))
        }
    }
}

async fn resolve_mania_source(
    source: &SimilaritySource,
    state: &AppState,
    dataset: &ManiaDataset,
    target_mod: osu_difficulty_runtime::ManiaGameMod,
) -> CommandResult<(Option<u64>, Option<Vec<u8>>, Option<u64>, &'static str)> {
    match source {
        SimilaritySource::BeatmapId { value } => {
            let beatmap_id = parse_beatmap_id(value)?;
            if dataset.contains_mod(beatmap_id, target_mod) {
                Ok((Some(beatmap_id), None, None, "index"))
            } else {
                let bytes = fetch_online_osu(&state.providers, beatmap_id).await?;
                Ok((None, Some(bytes), Some(beatmap_id), "online"))
            }
        }
        SimilaritySource::LocalFile { path } => {
            let bytes = read_local_source(path.clone()).await?;
            Ok((None, Some(bytes), None, "local_file"))
        }
    }
}

async fn read_local_source(path: String) -> CommandResult<Vec<u8>> {
    crate::infrastructure::tasks::background("similarity", move || read_local_osu(&path))
        .await
        .map_err(|_| CommandError::new("BEATMAP_READ_FAILED", "谱面文件读取任务意外停止"))?
}

async fn load_standard_dataset(
    runtime: Arc<crate::features::similarity::dataset::SimilarityRuntime>,
    directory: String,
) -> CommandResult<Arc<Dataset>> {
    crate::infrastructure::tasks::background("similarity", move || {
        runtime
            .standard_dataset(&directory)
            .map_err(map_runtime_error)
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "相似谱面运行时意外停止"))?
}

async fn load_mania_dataset(
    runtime: Arc<crate::features::similarity::dataset::SimilarityRuntime>,
    directory: String,
) -> CommandResult<Arc<ManiaDataset>> {
    crate::infrastructure::tasks::background("similarity", move || {
        runtime.mania_dataset(&directory).map_err(map_runtime_error)
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "Mania 相似谱面运行时意外停止"))?
}

fn no_usable_seed_error(ruleset: Ruleset) -> CommandError {
    CommandError::new(
        "NO_USABLE_RECOMMENDATION_SEEDS",
        match ruleset {
            Ruleset::Mania => "成绩中的 Mania 谱面均无法读取，或不是受支持的 4K、6K、7K",
            _ => "成绩中的谱面均无法从本地索引或在线谱面源读取",
        },
    )
}

/// 区分三种没有参考成绩的原因：服务器没有返回记录、Mod 池不匹配、成绩无法作为参考。
fn mania_seed_error_message(
    kind: SimilarityRecommendationKind,
    scores: &[Score],
    mod_pool: &[ManiaGameMod],
    selection: &ManiaSeedSelection,
) -> String {
    if scores.is_empty() {
        return match kind {
            SimilarityRecommendationKind::Recent => {
                "osu! 服务器没有返回最近的 Mania 通过成绩。请确认当前账号有已上传的成绩，或改用本地 .osu 查询。"
                    .to_owned()
            }
            SimilarityRecommendationKind::Best => {
                "osu! 服务器没有返回当前账号的 Mania BP 成绩。".to_owned()
            }
        };
    }
    let pool = mod_pool
        .iter()
        .map(|game_mod| game_mod.as_str())
        .collect::<Vec<_>>()
        .join(" / ");
    if selection.outside_mod_pool_scores > 0 {
        return format!(
            "服务器返回的 {} 条 Mania 成绩里没有 {} 的参考成绩，其中 {} 张使用了其他 Mod。请切换参考 Mod，或勾选多 Mod 混池推荐。",
            scores.len(),
            pool,
            selection.outside_mod_pool_scores
        );
    }
    format!(
        "服务器返回的 {} 条 Mania 成绩里有 {} 张使用了会改变键位排列或倍率的 Mod（包含自定义倍率），无法作为参考成绩。",
        scores.len(),
        selection.unusable_mod_scores
    )
}

async fn inspect(
    runtime: Arc<crate::features::similarity::dataset::SimilarityRuntime>,
    ruleset: Ruleset,
    directory: Option<String>,
) -> CommandResult<SimilarityIndexStatus> {
    crate::infrastructure::tasks::background("similarity", move || {
        runtime.inspect(ruleset, directory.as_deref())
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "本地索引校验任务意外停止"))
}

fn required_directory(state: &AppState, ruleset: Ruleset) -> CommandResult<String> {
    configured_directory(state, ruleset)?.ok_or_else(|| {
        CommandError::new(
            "SIMILARITY_INDEX_NOT_CONFIGURED",
            match ruleset {
                Ruleset::Mania => "请先选择 osu!mania 本地相似谱面索引目录",
                _ => "请先选择本地相似谱面索引目录",
            },
        )
    })
}

fn configured_directory(state: &AppState, ruleset: Ruleset) -> CommandResult<Option<String>> {
    directory_from_settings(&state.store.snapshot()?.settings, ruleset)
}

fn directory_from_settings(
    settings: &AppSettings,
    ruleset: Ruleset,
) -> CommandResult<Option<String>> {
    match ruleset {
        Ruleset::Osu => Ok(settings.similarity_index_directory.clone()),
        Ruleset::Mania => Ok(settings.mania_similarity_index_directory.clone()),
        Ruleset::Taiko | Ruleset::Fruits => Err(unsupported_ruleset_error(ruleset)),
    }
}

fn set_configured_directory(
    settings: &mut AppSettings,
    ruleset: Ruleset,
    directory: Option<String>,
) -> CommandResult<()> {
    match ruleset {
        Ruleset::Osu => settings.similarity_index_directory = directory,
        Ruleset::Mania => settings.mania_similarity_index_directory = directory,
        Ruleset::Taiko | Ruleset::Fruits => return Err(unsupported_ruleset_error(ruleset)),
    }
    Ok(())
}

fn unsupported_ruleset_error(ruleset: Ruleset) -> CommandError {
    CommandError::new(
        "SIMILARITY_RULESET_UNSUPPORTED",
        format!("相似谱面暂不支持 {ruleset} 模式"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_keep_standard_and_mania_directories_independent() {
        let mut settings = AppSettings::default();
        set_configured_directory(&mut settings, Ruleset::Osu, Some("S:/standard".into()))
            .expect("standard directory");
        set_configured_directory(&mut settings, Ruleset::Mania, Some("M:/mania".into()))
            .expect("mania directory");

        assert_eq!(
            directory_from_settings(&settings, Ruleset::Osu).expect("standard setting"),
            Some("S:/standard".into())
        );
        assert_eq!(
            directory_from_settings(&settings, Ruleset::Mania).expect("mania setting"),
            Some("M:/mania".into())
        );
        set_configured_directory(&mut settings, Ruleset::Mania, None)
            .expect("clear mania directory");
        assert_eq!(
            settings.similarity_index_directory.as_deref(),
            Some("S:/standard")
        );
        assert!(settings.mania_similarity_index_directory.is_none());
    }

    #[test]
    fn unsupported_rulesets_cannot_acquire_a_directory() {
        let settings = AppSettings::default();
        for ruleset in [Ruleset::Taiko, Ruleset::Fruits] {
            let error = directory_from_settings(&settings, ruleset)
                .expect_err("unsupported ruleset must not have a directory");
            assert_eq!(error.code, "SIMILARITY_RULESET_UNSUPPORTED");
        }
    }

    #[test]
    fn recommendation_request_selects_the_api_ruleset() {
        let request = SimilarityRecommendationRequest::Mania {
            kind: SimilarityRecommendationKind::Recent,
            result_limit: 20,
            seed_limit: Some(5),
            excluded_beatmap_ids: vec![],
            candidate_mods: vec![osu_difficulty_runtime::ManiaGameMod::Nm],
        };
        assert_eq!(request.ruleset(), Ruleset::Mania);
        assert_eq!(request.kind(), SimilarityRecommendationKind::Recent);
        assert_eq!(request.seed_limit(), Some(5));
    }
}
