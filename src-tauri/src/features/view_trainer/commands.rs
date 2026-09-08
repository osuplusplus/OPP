use super::{models::*, service};
use crate::{error::CommandResult, state::AppState};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn view_trainer_get_timeline(
    client: crate::features::local_analysis::LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Timeline> {
    let analysis = Arc::clone(&state.local_analysis);
    tauri::async_runtime::spawn_blocking(move || {
        service::timeline_for_analysis(&analysis, client, &resource_id)
    })
    .await
    .map_err(|error| {
        crate::error::CommandError::new("VIEW_TRAINER_TIMELINE_FAILED", error.to_string())
    })?
}

#[tauri::command(async)]
pub async fn view_trainer_import(
    client: crate::features::local_analysis::LocalClient,
    resource_id: String,
    staged_path: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let analysis = std::sync::Arc::clone(&state.local_analysis);
    tauri::async_runtime::spawn_blocking(move || {
        let source_path =
            std::path::PathBuf::from(analysis.beatmap_file_path(client, &resource_id)?);
        // Lazer 谱面来自内容寻址物化缓存，不能把导入结果写回缓存目录。
        // 导入编辑器时优先使用 Stable 的实际 Songs 根目录。
        let target_root = if client == crate::features::local_analysis::LocalClient::Lazer {
            let root = analysis
                .source_status(crate::features::local_analysis::LocalClient::Stable)
                .ok()
                .and_then(|status| status.install_root)
                .map(std::path::PathBuf::from)
                .map(|root| {
                    if root
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("Songs"))
                    {
                        root
                    } else {
                        root.join("Songs")
                    }
                });
            if root.as_ref().is_none_or(|path| !path.is_dir()) {
                return Err(crate::error::CommandError::new(
                    "VIEW_TRAINER_IMPORT_FAILED",
                    "Lazer 谱面导入需要可用的 osu! Stable Songs 目录",
                ));
            }
            root
        } else {
            None
        };
        service::import_staged_at_path(source_path, &staged_path, target_root)
    })
    .await
    .map_err(|error| {
        crate::error::CommandError::new("VIEW_TRAINER_IMPORT_FAILED", error.to_string())
    })?
}

#[tauri::command(async)]
pub async fn view_trainer_generate(
    request: ViewTrainerRequest,
    state: State<'_, AppState>,
) -> CommandResult<crate::features::trainer::TrainerResult> {
    let analysis = Arc::clone(&state.local_analysis);
    tauri::async_runtime::spawn_blocking(move || {
        let request = service::resolve_request_with_analysis(&analysis, request)?;
        let source_path = std::path::PathBuf::from(
            analysis.beatmap_file_path(request.client, &request.resource_id)?,
        );
        let legacy = crate::features::trainer::TrainerRequest {
            client: request.client,
            resource_id: request.resource_id,
            rate: request.rate,
            ar: request.ar,
            od: request.od,
            cs: request.cs,
            hp: request.hp,
            no_spinners: request.no_spinners,
            change_pitch: request.change_pitch,
            preview_only: request.preview_only,
            min_bpm: request.min_bpm,
            max_bpm: request.max_bpm,
            start_time_ms: request.start_time_ms,
            end_time_ms: request.end_time_ms,
        };
        crate::features::trainer::stage_trainer_beatmap_at_path(legacy, source_path)
    })
    .await
    .map_err(|error| {
        crate::error::CommandError::new("VIEW_TRAINER_GENERATE_FAILED", error.to_string())
    })?
}
