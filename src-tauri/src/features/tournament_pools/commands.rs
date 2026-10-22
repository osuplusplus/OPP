//! 图池相关 Tauri 命令：查询、同步、打开图池与修复元数据的薄封装。
use serde::Serialize;
use tauri::{Emitter, State};

use super::{models::*, service};
use crate::{
    error::CommandResult,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

/// 为已保存的图池条目回填谱面展示元数据；不请求图池来源，也不改动收藏夹成员。
/// Backfill display metadata for saved entries without requesting or synchronising the pool source.
#[tauri::command]
pub async fn repair_tournament_pool_metadata(
    folder_id: String,
    state: State<'_, AppState>,
) -> CommandResult<(usize, usize)> {
    let span = global().map(|logger| logger.operation("tournament_pools", "repair_metadata"));
    finish_span(span, service::repair_metadata(&folder_id, &state).await)
}

/// 拉取并补全图池，仅供前端查看，不写入收藏夹。
#[tauri::command]
pub async fn get_tournament_pool(
    reference: TournamentPoolRef,
    state: State<'_, AppState>,
) -> CommandResult<TournamentPool> {
    let span = global().map(|logger| logger.operation("tournament_pools", "get_tournament_pool"));
    finish_span(span, service::load(&reference, &state, |_| {}).await)
}

/// 拉取图池并以替换方式同步到关联的收藏夹文件夹。
#[tauri::command]
pub async fn sync_tournament_pool_collection(
    reference: TournamentPoolRef,
    state: State<'_, AppState>,
) -> CommandResult<TournamentPoolSyncResult> {
    let span = global()
        .map(|logger| logger.operation("tournament_pools", "sync_tournament_pool_collection"));
    let result =
        async { service::save(service::load(&reference, &state, |_| {}).await?, &state) }.await;
    finish_span(span, result)
}

/// 导入进度事件载荷：前端凭 `request_id` 匹配本次请求，`phase` 为阶段名。
#[derive(Clone, Serialize)]
struct ImportProgress<'a> {
    request_id: u64,
    phase: &'a str,
}

/// 打开图池（本地优先）：命中已关联文件夹则直接复用、不请求来源；
/// 需要导入时通过 `tournament-pool-import-progress` 事件回报进度。
/// Opening is local-first; only an explicit sync replaces an existing snapshot.
#[tauri::command]
pub async fn open_tournament_pool(
    reference: TournamentPoolRef,
    request_id: u64,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<PoolOpenResult> {
    let span = global().map(|logger| logger.operation("tournament_pools", "open_tournament_pool"));
    let result = async {
        let progress = |phase: &str| {
            let _ = app.emit_to(
                "main",
                "tournament-pool-import-progress",
                ImportProgress { request_id, phase },
            );
        };
        service::open_or_import(&reference, &state, || async {
            let pool = service::load(&reference, &state, progress).await?;
            progress("saving");
            Ok(pool)
        })
        .await
    }
    .await;
    finish_span(span, result)
}
