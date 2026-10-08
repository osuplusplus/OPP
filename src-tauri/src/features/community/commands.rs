use super::{models::*, service::CommunityService};
use crate::{
    error::CommandResult,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};
use tauri::State;

#[tauri::command]
pub async fn list_community_tournaments(
    query: CommunityQuery,
    state: State<'_, AppState>,
) -> CommandResult<CommunityPage<CommunityTournament>> {
    let span = global().map(|logger| logger.operation("community", "list_community_tournaments"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .list("tournaments", query, false)
            .await,
    )
}
#[tauri::command]
pub async fn get_community_tournament(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<CommunityTournament> {
    let span = global().map(|logger| logger.operation("community", "get_community_tournament"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .detail("tournaments", &id)
            .await,
    )
}
#[tauri::command]
pub async fn list_community_lobbies(
    query: CommunityQuery,
    mine: bool,
    state: State<'_, AppState>,
) -> CommandResult<CommunityPage<CommunityLobby>> {
    let span = global().map(|logger| logger.operation("community", "list_community_lobbies"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .list("lobbies", query, mine)
            .await,
    )
}
#[tauri::command]
pub async fn get_community_lobby(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<CommunityLobby> {
    let span = global().map(|logger| logger.operation("community", "get_community_lobby"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .detail("lobbies", &id)
            .await,
    )
}
#[tauri::command]
pub async fn save_community_lobby(
    id: Option<String>,
    input: LobbyInput,
    state: State<'_, AppState>,
) -> CommandResult<CommunityLobby> {
    let span = global().map(|logger| logger.operation("community", "save_community_lobby"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .save(id.as_deref(), input)
            .await,
    )
}
#[tauri::command]
pub async fn close_community_lobby(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<CommunityLobby> {
    let span = global().map(|logger| logger.operation("community", "close_community_lobby"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .close(&id)
            .await,
    )
}
#[tauri::command]
pub async fn delete_community_lobby(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let span = global().map(|logger| logger.operation("community", "delete_community_lobby"));
    finish_span(
        span,
        CommunityService::new(state.beatmaphub.as_ref())
            .delete(&id)
            .await,
    )
}
