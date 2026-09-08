use std::sync::Arc;

use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::LocalClient,
    state::AppState,
};

use super::models::*;

#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn open_skin_workshop_package(
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<crate::features::local_analysis::LocalSkinSummary> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || service.open_package(std::path::Path::new(&path)))
        .await
        .map_err(task_error)?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：执行已校验的工作流动作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn execute_skin_workshop_action(
    target_skin_resource_id: String,
    mode: SkinWorkshopWriteMode,
    action: SkinWorkshopAction,
    state: State<'_, AppState>,
) -> CommandResult<SkinWorkshopMutationResult> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || {
        service.execute_action(&target_skin_resource_id, mode, action)
    })
    .await
    .map_err(task_error)?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：执行已校验的工作流动作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn execute_skin_workshop_preset(
    target_skin_resource_id: String,
    mode: SkinWorkshopWriteMode,
    preset: SkinWorkshopPreset,
    state: State<'_, AppState>,
) -> CommandResult<SkinWorkshopMutationResult> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || {
        service.execute_preset(&target_skin_resource_id, mode, preset)
    })
    .await
    .map_err(task_error)?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_skin_workshop_tree(
    client: LocalClient,
    skin_resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<SkinTree> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || service.tree(client, &skin_resource_id))
        .await
        .map_err(task_error)?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_skin_workshop_part_preview(
    client: LocalClient,
    skin_resource_id: String,
    part_key: String,
    state: State<'_, AppState>,
) -> CommandResult<SkinPartPreview> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || {
        service.part_preview(client, &skin_resource_id, &part_key)
    })
    .await
    .map_err(task_error)?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_skin_workshop_asset(
    client: LocalClient,
    skin_resource_id: String,
    asset_id: String,
    state: State<'_, AppState>,
) -> CommandResult<SkinAssetPayload> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || {
        service.asset(client, &skin_resource_id, &asset_id)
    })
    .await
    .map_err(task_error)?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_skin_workshop_config(
    client: LocalClient,
    skin_resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<SkinConfigDocument> {
    let service = Arc::clone(&state.skin_workshop);
    tauri::async_runtime::spawn_blocking(move || service.config(client, &skin_resource_id))
        .await
        .map_err(task_error)?
}

fn task_error(error: impl ToString) -> CommandError {
    CommandError::new("SKIN_WORKSHOP_TASK_ERROR", error.to_string())
}
