//! 皮肤工坊 Tauri 命令层：反序列化前端参数、调度任务并包装错误。
//! 修改类命令走 `background` 任务避免阻塞交互，读取类走 `interactive`。

use std::sync::Arc;

use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::LocalClient,
    state::AppState,
};

use super::models::*;

/// 打开 `.osk` 皮肤包：解压到工坊临时目录并返回皮肤摘要。
#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn open_skin_workshop_package(
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<crate::features::local_analysis::LocalSkinSummary> {
    let service = Arc::clone(&state.skin_workshop);
    crate::infrastructure::tasks::background("skin_workshop", move || {
        service.open_package(std::path::Path::new(&path))
    })
    .await
    .map_err(task_error)?
}

/// 在指定皮肤上执行单条修改动作，按写入模式覆写原位或另存副本。
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
    crate::infrastructure::tasks::background("skin_workshop", move || {
        service.execute_action(&target_skin_resource_id, mode, action)
    })
    .await
    .map_err(task_error)?
}

/// 执行预设工作流（当前为 Mania 迁移），同样受写入模式约束。
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
    crate::infrastructure::tasks::background("skin_workshop", move || {
        service.execute_preset(&target_skin_resource_id, mode, preset)
    })
    .await
    .map_err(task_error)?
}

/// 构建指定皮肤的资源分组目录树。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_skin_workshop_tree(
    client: LocalClient,
    skin_resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<SkinTree> {
    let service = Arc::clone(&state.skin_workshop);
    crate::infrastructure::tasks::interactive("skin_workshop", move || {
        service.tree(client, &skin_resource_id)
    })
    .await
    .map_err(task_error)?
}

/// 预览某个分组（含子分组）下的全部资源变体。
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
    crate::infrastructure::tasks::interactive("skin_workshop", move || {
        service.part_preview(client, &skin_resource_id, &part_key)
    })
    .await
    .map_err(task_error)?
}

/// 读取单个资源内容，以 base64 `data:` URL 返回供前端预览。
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
    crate::infrastructure::tasks::interactive("skin_workshop", move || {
        service.asset(client, &skin_resource_id, &asset_id)
    })
    .await
    .map_err(task_error)?
}

/// 读取 `skin.ini` 的结构化文档（节、条目、错误与原始源码）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_skin_workshop_config(
    client: LocalClient,
    skin_resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<SkinConfigDocument> {
    let service = Arc::clone(&state.skin_workshop);
    crate::infrastructure::tasks::interactive("skin_workshop", move || {
        service.config(client, &skin_resource_id)
    })
    .await
    .map_err(task_error)?
}

/// 把任务调度层面的失败包装为统一的命令错误。
fn task_error(error: impl ToString) -> CommandError {
    CommandError::new("SKIN_WORKSHOP_TASK_ERROR", error.to_string())
}
