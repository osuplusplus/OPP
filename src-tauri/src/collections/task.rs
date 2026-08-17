use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};

use super::CollectionTaskProgress;
use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

pub(super) fn ensure_collection_task_active(state: &AppState) -> CommandResult<()> {
    if state.collection_task_cancel.load(Ordering::Relaxed) {
        Err(CommandError::new(
            "COLLECTION_TASK_CANCELLED",
            "收藏夹同步已取消",
        ))
    } else {
        Ok(())
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：初始化可取消的异步流程。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn begin_collection_task(state: State<'_, AppState>) {
    state.collection_task_cancel.store(false, Ordering::Relaxed);
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_collection_task(state: State<'_, AppState>) -> CommandResult<()> {
    state.collection_task_cancel.store(true, Ordering::Relaxed);
    if let Some(cancel) = state
        .beatmap_download
        .lock()
        .map_err(|_| CommandError::new("STATE_ERROR", "下载队列状态锁已损坏"))?
        .as_ref()
    {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

pub(super) fn emit_collection_progress(
    app: &AppHandle,
    phase: &str,
    processed: usize,
    total: usize,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "collection-task-progress",
        CollectionTaskProgress {
            phase: phase.into(),
            processed,
            total,
            message: message.into(),
        },
    );
}
