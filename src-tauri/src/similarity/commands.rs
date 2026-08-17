use std::sync::Arc;

use tauri::State;

use crate::{
    account::ensure_access_token,
    error::{CommandError, CommandResult},
    models::Ruleset,
    similarity::{
        models::{
            SimilarityIndexStatus, SimilarityQueryRequest, SimilarityQueryResponse,
            SimilarityRecommendationKind, SimilarityRecommendationRequest,
            SimilarityRecommendationResponse, SimilaritySource,
        },
        query::{
            map_runtime_error, options_from_recommendation_request, options_from_request,
            recommendation_response_from_runtime, response_from_runtime,
        },
        recommendation::{requested_seed_limit, seed_ids},
        source::{fetch_online_osu, parse_beatmap_id, read_local_osu},
    },
    state::AppState,
};

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_similarity_index_status(
    state: State<'_, AppState>,
) -> CommandResult<SimilarityIndexStatus> {
    let directory = configured_directory(&state)?;
    state.similarity.clear();
    inspect(state.similarity.clone(), directory).await
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：配置服务的索引或连接。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn configure_similarity_index(
    directory: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityIndexStatus> {
    let directory = directory
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    state
        .store
        .update(|persisted| persisted.settings.similarity_index_directory = directory.clone())?;
    state.similarity.clear();
    inspect(state.similarity.clone(), directory).await
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：按条件查询本地索引。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn query_similar_beatmaps(
    request: SimilarityQueryRequest,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityQueryResponse> {
    let directory = configured_directory(&state)?.ok_or_else(|| {
        CommandError::new(
            "SIMILARITY_INDEX_NOT_CONFIGURED",
            "请先选择本地相似谱面索引目录",
        )
    })?;
    let runtime = state.similarity.clone();
    let dataset_directory = directory.clone();
    let dataset = tauri::async_runtime::spawn_blocking(move || {
        runtime
            .dataset(&dataset_directory)
            .map_err(map_runtime_error)
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "相似谱面运行时意外停止"))??;
    let options = options_from_request(&request)?;

    let (indexed_id, bytes, source_label) = match &request.source {
        SimilaritySource::BeatmapId { value } => {
            let beatmap_id = parse_beatmap_id(value)?;
            if dataset.contains(beatmap_id) {
                (Some(beatmap_id), None, "index")
            } else {
                let bytes = fetch_online_osu(&state.providers, beatmap_id).await?;
                (None, Some(bytes), "online")
            }
        }
        SimilaritySource::LocalFile { path } => {
            let path = path.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || read_local_osu(&path))
                .await
                .map_err(|_| {
                    CommandError::new("BEATMAP_READ_FAILED", "谱面文件读取任务意外停止")
                })??;
            (None, Some(bytes), "local_file")
        }
    };

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

#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn recommend_similar_beatmaps(
    request: SimilarityRecommendationRequest,
    state: State<'_, AppState>,
) -> CommandResult<SimilarityRecommendationResponse> {
    let directory = configured_directory(&state)?.ok_or_else(|| {
        CommandError::new(
            "SIMILARITY_INDEX_NOT_CONFIGURED",
            "请先选择本地相似谱面索引目录",
        )
    })?;
    let runtime = state.similarity.clone();
    let dataset_directory = directory.clone();
    let dataset = tauri::async_runtime::spawn_blocking(move || {
        runtime
            .dataset(&dataset_directory)
            .map_err(map_runtime_error)
    })
    .await
    .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "相似谱面运行时意外停止"))??;
    let options = options_from_recommendation_request(&request)?;

    let access_token = ensure_access_token(&state).await?;
    let profile = state
        .api
        .get_own_profile(&access_token, Ruleset::Osu)
        .await?;
    let scores = match request.kind {
        SimilarityRecommendationKind::Recent => {
            state
                .api
                .get_recent_scores(&access_token, profile.id, Ruleset::Osu)
                .await?
        }
        SimilarityRecommendationKind::Best => {
            state
                .api
                .get_best_scores(&access_token, profile.id, Ruleset::Osu)
                .await?
        }
    };
    let seed_ids = seed_ids(&scores, requested_seed_limit(request.seed_limit));
    if seed_ids.is_empty() {
        return Err(CommandError::new(
            "NO_RECOMMENDATION_SEEDS",
            match request.kind {
                SimilarityRecommendationKind::Recent => "没有可用于推荐的最近通过成绩",
                SimilarityRecommendationKind::Best => "没有可用于推荐的 BP 成绩",
            },
        ));
    }

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
        return Err(CommandError::new(
            "NO_USABLE_RECOMMENDATION_SEEDS",
            "成绩中的谱面均无法从本地索引或在线谱面源读取",
        ));
    }

    let kind = request.kind;
    let final_result_limit = request.result_limit;
    let excluded_beatmap_ids = request.excluded_beatmap_ids.into_iter().collect();
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

async fn inspect(
    runtime: Arc<crate::similarity::dataset::SimilarityRuntime>,
    directory: Option<String>,
) -> CommandResult<SimilarityIndexStatus> {
    tauri::async_runtime::spawn_blocking(move || runtime.inspect(directory.as_deref()))
        .await
        .map_err(|_| CommandError::new("SIMILARITY_RUNTIME_ERROR", "本地索引校验任务意外停止"))
}

fn configured_directory(state: &AppState) -> CommandResult<Option<String>> {
    Ok(state.store.snapshot()?.settings.similarity_index_directory)
}
