use serde::Serialize;
use tauri::{Emitter, State};

use super::{models::*, service};
use crate::{
    error::CommandResult,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

/// Backfill display metadata for saved entries without requesting or synchronising the pool source.
#[tauri::command]
pub async fn repair_tournament_pool_metadata(
    folder_id: String,
    state: State<'_, AppState>,
) -> CommandResult<(usize, usize)> {
    let span = global().map(|logger| logger.operation("tournament_pools", "repair_metadata"));
    finish_span(span, service::repair_metadata(&folder_id, &state).await)
}

#[tauri::command]
pub async fn get_tournament_pool(
    reference: TournamentPoolRef,
    state: State<'_, AppState>,
) -> CommandResult<TournamentPool> {
    let span = global().map(|logger| logger.operation("tournament_pools", "get_tournament_pool"));
    finish_span(span, service::load(&reference, &state, |_| {}).await)
}

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

#[derive(Clone, Serialize)]
struct ImportProgress<'a> {
    request_id: u64,
    phase: &'a str,
}

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
