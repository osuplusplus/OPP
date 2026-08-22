use std::{collections::HashSet, sync::Arc};

use osu_difficulty_runtime::{Dataset, ManiaDataset};
use tauri::State;

use crate::{
    account::ensure_access_token,
    app::{
        models::{AppSettings, Ruleset},
        state::AppState,
    },
    error::{CommandError, CommandResult},
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
        recommendation::{ManiaSeed, mania_seed_ids, requested_seed_limit, seed_ids},
        source::{fetch_online_osu, parse_beatmap_id, read_local_osu},
    },
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

    tauri::async_runtime::spawn_blocking(move || {
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
    let (indexed_id, bytes, source_beatmap_id, source_label) = resolve_mania_source(
        request.source(),
        state,
        &dataset,
        target_mod,
    )
    .await?;

    tauri::async_runtime::spawn_blocking(move || {
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
    let (standard_seed_ids, mania_seeds, initially_skipped_seed_count) =
        if ruleset == Ruleset::Mania {
            let (seeds, skipped) = mania_seed_ids(&scores, requested_seed_limit);
            (Vec::new(), seeds, skipped)
        } else {
            (seed_ids(&scores, requested_seed_limit), Vec::new(), 0)
        };
    if standard_seed_ids.is_empty() && mania_seeds.is_empty() {
        return Err(CommandError::new(
            "NO_RECOMMENDATION_SEEDS",
            match (ruleset, kind) {
                (Ruleset::Mania, SimilarityRecommendationKind::Recent) => {
                    "没有可用于推荐的 NM / DT / HT Mania 最近通过成绩"
                }
                (Ruleset::Mania, SimilarityRecommendationKind::Best) => {
                    "没有可用于推荐的 NM / DT / HT Mania BP 成绩"
                }
                (_, SimilarityRecommendationKind::Recent) => "没有可用于推荐的最近通过成绩",
                (_, SimilarityRecommendationKind::Best) => "没有可用于推荐的 BP 成绩",
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
    tauri::async_runtime::spawn_blocking(move || {
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
    tauri::async_runtime::spawn_blocking(move || {
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
    tauri::async_runtime::spawn_blocking(move || read_local_osu(&path))
        .await
        .map_err(|_| CommandError::new("BEATMAP_READ_FAILED", "谱面文件读取任务意外停止"))?
}

async fn load_standard_dataset(
    runtime: Arc<crate::similarity::dataset::SimilarityRuntime>,
    directory: String,
) -> CommandResult<Arc<Dataset>> {
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .standard_dataset(&directory)
            .map_err(map_runtime_error)
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "相似谱面运行时意外停止"))?
}

async fn load_mania_dataset(
    runtime: Arc<crate::similarity::dataset::SimilarityRuntime>,
    directory: String,
) -> CommandResult<Arc<ManiaDataset>> {
    tauri::async_runtime::spawn_blocking(move || {
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

async fn inspect(
    runtime: Arc<crate::similarity::dataset::SimilarityRuntime>,
    ruleset: Ruleset,
    directory: Option<String>,
) -> CommandResult<SimilarityIndexStatus> {
    tauri::async_runtime::spawn_blocking(move || runtime.inspect(ruleset, directory.as_deref()))
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
