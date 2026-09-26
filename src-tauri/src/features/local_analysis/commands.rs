use std::{path::Path, sync::Arc};

use super::models::LocalArtwork;
use super::models::{BackgroundSize, LocalBeatmapAudioPayload};
use super::models::{
    BeatmapQuery, LocalBeatmapDetail, LocalBeatmapSetSummary, LocalBeatmapSummary, LocalClient,
    LocalIndexLoadStatus, LocalLibrarySummary, LocalScanProgress, LocalSkinAssetPayload,
    LocalSkinDetail, LocalSkinPreview, LocalSkinSummary, LocalSourceStatus, Page, SkinQuery,
};
use crate::domain::Ruleset;
use tauri::{AppHandle, Emitter, State};

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

#[tauri::command]
pub async fn query_local_beatmap_presence(
    ids: Vec<i32>,
    client: Option<LocalClient>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::models::LocalBeatmapPresence>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("query_local_beatmap_presence", move || {
        let span = crate::infrastructure::logging::global()
            .map(|log| log.operation("local_analysis.command", "beatmap_presence"));
        crate::infrastructure::logging::finish_span(span, service.beatmap_presence(ids, client))
    })
    .await?
}

#[tauri::command]
pub async fn get_local_library_storage_status(
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::models::LocalLibraryStorageStatus>> {
    let span = crate::infrastructure::logging::global()
        .map(|log| log.operation("local_analysis.command", "storage_status"));
    crate::infrastructure::logging::finish_span(span, state.local_analysis.library_storage_status())
}

#[tauri::command]
pub async fn migrate_local_library_database(
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::models::LocalLibraryStorageStatus>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("migrate_local_library_database", move || {
        let span = crate::infrastructure::logging::global()
            .map(|log| log.operation("local_analysis.command", "migrate_library"));
        crate::infrastructure::logging::finish_span(span, service.migrate_cached_indexes())
    })
    .await?
}

#[tauri::command(async)]
pub async fn get_local_sources(
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalSourceStatus>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_sources", move || {
        service.source_statuses()
    })
    .await?
}

#[tauri::command]
pub async fn get_local_artwork_sample(
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalArtwork>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("get_local_artwork_sample", move || {
        let span = crate::infrastructure::logging::global()
            .map(|log| log.operation("local_analysis", "get_local_artwork_sample"));
        crate::infrastructure::logging::finish_span(span, service.artwork_sample())
    })
    .await?
}

#[tauri::command(async)]
pub fn get_local_index_status(state: State<'_, AppState>) -> CommandResult<LocalIndexLoadStatus> {
    state.local_analysis.index_load_status()
}

#[tauri::command(async)]
pub async fn set_local_source(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSourceStatus> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("set_local_source", move || {
        service.set_source(client, Path::new(path.trim()))
    })
    .await?
}

#[tauri::command(async)]
pub async fn reset_local_source(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<LocalSourceStatus> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("reset_local_source", move || {
        service.reset_source(client)
    })
    .await?
}

#[tauri::command(async)]
pub fn get_local_summary(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<Option<LocalLibrarySummary>> {
    state.local_analysis.summary(client)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn scan_local_source(
    client: LocalClient,
    force: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<LocalLibrarySummary> {
    let service = Arc::clone(&state.local_analysis);
    let emit_event = Arc::new(move |progress: LocalScanProgress| {
        let _ = app.emit("local-scan-progress", progress);
    });
    tokio::task::spawn_blocking(move || service.scan(client, force, emit_event))
        .await
        .map_err(|error| {
            CommandError::new(
                "LOCAL_SCAN_TASK_ERROR",
                format!("本地扫描任务异常结束：{error}"),
            )
        })?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_local_scan(client: LocalClient, state: State<'_, AppState>) -> CommandResult<()> {
    state.local_analysis.cancel_scan(client)
}

#[tauri::command(async)]
pub async fn query_local_beatmaps(
    query: BeatmapQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalBeatmapSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("query_local_beatmaps", move || {
        service.query_beatmaps(query)
    })
    .await?
}

#[tauri::command(async)]
pub async fn query_local_beatmap_sets(
    query: BeatmapQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalBeatmapSetSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("query_local_beatmap_sets", move || {
        service.query_beatmap_sets(query)
    })
    .await?
}

#[tauri::command(async)]
pub async fn get_local_beatmap_detail(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapDetail> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_beatmap_detail", move || {
        service.beatmap_detail(client, &resource_id)
    })
    .await?
}

#[tauri::command(async)]
pub async fn get_local_beatmap_path(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_beatmap_path", move || {
        service.beatmap_file_path(client, &resource_id)
    })
    .await?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_beatmap_background(
    client: LocalClient,
    resource_id: String,
    size: Option<BackgroundSize>,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let service = Arc::clone(&state.local_analysis);
    let key = (
        client,
        resource_id.clone(),
        matches!(size, Some(BackgroundSize::Stage)),
        service.background_revision(client)?,
    );
    service
        .background_requests
        .run(key, || async {
            let service = Arc::clone(&service);
            crate::infrastructure::tasks::interactive("local_analysis", move || {
                let span = crate::infrastructure::logging::global()
                    .map(|log| log.operation("local_analysis", "get_local_beatmap_background"));
                let result = match size.unwrap_or_default() {
                    BackgroundSize::Thumbnail => service.beatmap_background(client, &resource_id),
                    BackgroundSize::Stage => service.beatmap_background_sized(
                        client,
                        &resource_id,
                        BackgroundSize::Stage,
                    ),
                };
                crate::infrastructure::logging::finish_span(span, result)
            })
            .await
            .map_err(|error| {
                CommandError::new(
                    "LOCAL_BACKGROUND_TASK_ERROR",
                    format!("谱面背景处理任务异常结束：{error}"),
                )
            })?
        })
        .await
}

#[tauri::command(async)]
pub async fn get_local_beatmap_set(
    client: LocalClient,
    set_key: String,
    ruleset: Ruleset,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapSetSummary> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_beatmap_set", move || {
        service.complete_beatmap_set(client, &set_key, ruleset)
    })
    .await?
}

#[tauri::command(async)]
pub async fn pick_random_local_beatmap_set(
    query: BeatmapQuery,
    exclude_set_key: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Option<LocalBeatmapSetSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("pick_random_local_beatmap_set", move || {
        service.random_beatmap_set(query, exclude_set_key.as_deref())
    })
    .await?
}

#[tauri::command]
pub async fn get_local_beatmap_audio(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapAudioPayload> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("beatmap_audio", move || {
        service.beatmap_audio(client, &resource_id)
    })
    .await
    .map_err(|e| CommandError::new("LOCAL_AUDIO_TASK_ERROR", e.to_string()))?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：将资源导出到用户指定的位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn export_local_beatmap_set(
    client: LocalClient,
    set_key: String,
    out_dir: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("local_analysis", move || {
        service.export_beatmap_set_osz(client, &set_key, Path::new(&out_dir))
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_EXPORT_TASK_ERROR",
            format!("谱面集导出任务异常结束：{error}"),
        )
    })?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：将资源导出到用户指定的位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn export_local_skin(
    client: LocalClient,
    skin_resource_id: String,
    out_dir: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("local_analysis", move || {
        service.export_skin_osk(client, &skin_resource_id, Path::new(&out_dir))
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_EXPORT_TASK_ERROR",
            format!("Skin 导出任务异常结束：{error}"),
        )
    })?
}

#[tauri::command(async)]
pub async fn query_local_skins(
    query: SkinQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalSkinSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("query_local_skins", move || {
        service.query_skins(query)
    })
    .await?
}

#[tauri::command(async)]
pub async fn get_local_skin_detail(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinDetail> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_skin_detail", move || {
        service.skin_detail(client, &resource_id)
    })
    .await?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_skin_preview(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinPreview> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("skin_preview", move || {
        service.skin_preview(client, &resource_id)
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_SKIN_PREVIEW_TASK_ERROR",
            format!("Skin 预览索引任务异常结束：{error}"),
        )
    })?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_skin_asset(
    client: LocalClient,
    skin_resource_id: String,
    asset_resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinAssetPayload> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("local_analysis", move || {
        service.skin_asset(client, &skin_resource_id, &asset_resource_id)
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_SKIN_ASSET_TASK_ERROR",
            format!("Skin 资源预览任务异常结束：{error}"),
        )
    })?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn replace_local_skin_asset(
    client: LocalClient,
    skin_resource_id: String,
    asset_resource_id: String,
    replacement_path: String,
    save_as_new: bool,
    new_skin_name: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("local_analysis", move || {
        service.replace_skin_asset(
            client,
            &skin_resource_id,
            &asset_resource_id,
            Path::new(&replacement_path),
            save_as_new,
            new_skin_name.as_deref(),
        )
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_SKIN_REPLACE_TASK_ERROR",
            format!("Skin 资源替换任务异常结束：{error}"),
        )
    })?
}
