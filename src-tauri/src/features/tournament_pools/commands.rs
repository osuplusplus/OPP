use tauri::State;

use super::{models::*, service};
use crate::{
    error::CommandResult,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

#[tauri::command]
pub async fn get_tournament_pool(
    reference: TournamentPoolRef,
    state: State<'_, AppState>,
) -> CommandResult<TournamentPool> {
    let span = global().map(|logger| logger.operation("tournament_pools", "get_tournament_pool"));
    finish_span(span, service::load(&reference, &state).await)
}

#[tauri::command]
pub async fn sync_tournament_pool_collection(
    reference: TournamentPoolRef,
    state: State<'_, AppState>,
) -> CommandResult<TournamentPoolSyncResult> {
    let span = global()
        .map(|logger| logger.operation("tournament_pools", "sync_tournament_pool_collection"));
    let result = async {
        let pool = service::load(&reference, &state).await?;
        let folder = state.collections.replace_tournament_pool(
            &reference.source_id(),
            &pool.title,
            service::candidates(&pool),
        )?;
        state.collections.notebooks.save_pool(
            &folder.id,
            crate::features::collections::notebook::PoolSnapshot {
                reference: reference.clone(),
                slots: pool
                    .entries
                    .iter()
                    .map(|entry| crate::features::collections::notebook::PoolSlot {
                        beatmap_id: entry.beatmap_id,
                        label: format!("{}{}", entry.selection_type, entry.position),
                        selected_by: entry
                            .selected_by_name
                            .clone()
                            .or_else(|| entry.selected_by.clone())
                            .unwrap_or_default(),
                        comment: entry.comment.clone(),
                    })
                    .collect(),
            },
        )?;
        Ok(TournamentPoolSyncResult {
            folder_id: folder.id,
            entry_count: folder.entries.len(),
            pool,
        })
    }
    .await;
    finish_span(span, result)
}
