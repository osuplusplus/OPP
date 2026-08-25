pub(crate) mod lazer_realm;
mod models;
pub(crate) mod parser;
mod service;
mod sources;

use std::{path::Path, sync::Arc};

pub use models::{
    BeatmapQuery, LocalBeatmapDetail, LocalBeatmapSetSummary, LocalBeatmapSummary, LocalClient,
    LocalIndexLoadStatus, LocalLibrarySummary, LocalScanProgress, LocalSkinAssetPayload,
    LocalSkinDetail, LocalSkinPreview, LocalSkinSummary, LocalSourceStatus, Page, SkinQuery,
    SkinSort, SortDirection, StrainAnalysis,
};
pub use service::LocalAnalysisService;
use tauri::{AppHandle, Emitter, State};

use crate::{
    error::{CommandError, CommandResult},
    app::state::AppState,
};

#[tauri::command(async)]
pub fn get_local_sources(state: State<'_, AppState>) -> CommandResult<Vec<LocalSourceStatus>> {
    state.local_analysis.source_statuses()
}

#[tauri::command(async)]
pub fn get_local_index_status(state: State<'_, AppState>) -> CommandResult<LocalIndexLoadStatus> {
    state.local_analysis.index_load_status()
}

#[tauri::command(async)]
pub fn set_local_source(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSourceStatus> {
    state
        .local_analysis
        .set_source(client, Path::new(path.trim()))
}

#[tauri::command(async)]
pub fn reset_local_source(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<LocalSourceStatus> {
    state.local_analysis.reset_source(client)
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
pub fn query_local_beatmaps(
    query: BeatmapQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalBeatmapSummary>> {
    state.local_analysis.query_beatmaps(query)
}

#[tauri::command(async)]
pub fn query_local_beatmap_sets(
    query: BeatmapQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalBeatmapSetSummary>> {
    state.local_analysis.query_beatmap_sets(query)
}

#[tauri::command(async)]
pub fn get_local_beatmap_detail(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapDetail> {
    state.local_analysis.beatmap_detail(client, &resource_id)
}

#[tauri::command(async)]
pub fn get_local_beatmap_path(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    state.local_analysis.beatmap_file_path(client, &resource_id)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_beatmap_background(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let service = Arc::clone(&state.local_analysis);
    tokio::task::spawn_blocking(move || service.beatmap_background(client, &resource_id))
        .await
        .map_err(|error| {
            CommandError::new(
                "LOCAL_BACKGROUND_TASK_ERROR",
                format!("谱面背景处理任务异常结束：{error}"),
            )
        })?
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
    tokio::task::spawn_blocking(move || {
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
    tokio::task::spawn_blocking(move || {
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
pub fn query_local_skins(
    query: SkinQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalSkinSummary>> {
    state.local_analysis.query_skins(query)
}

#[tauri::command(async)]
pub fn get_local_skin_detail(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinDetail> {
    state.local_analysis.skin_detail(client, &resource_id)
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
    tokio::task::spawn_blocking(move || service.skin_preview(client, &resource_id))
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
    tokio::task::spawn_blocking(move || {
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
    tokio::task::spawn_blocking(move || {
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
