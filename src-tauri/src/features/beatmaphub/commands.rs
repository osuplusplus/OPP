use tauri::State;

use crate::{error::CommandResult, state::AppState};

use super::models::*;

#[tauri::command]
pub fn get_beatmaphub_auth_status(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    state.beatmaphub.status()
}

#[tauri::command]
pub async fn create_beatmaphub_profile(
    display_name: String,
    device_name: String,
    state: State<'_, AppState>,
) -> CommandResult<AuthStatus> {
    state
        .beatmaphub
        .create_profile(display_name, device_name)
        .await
}

#[tauri::command]
pub async fn login_beatmaphub(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    state.beatmaphub.login().await
}

#[tauri::command]
pub async fn link_beatmaphub_device(
    link_token: String,
    device_name: String,
    state: State<'_, AppState>,
) -> CommandResult<AuthStatus> {
    state.beatmaphub.link_device(link_token, device_name).await
}

#[tauri::command]
pub async fn logout_beatmaphub(state: State<'_, AppState>) -> CommandResult<()> {
    state.beatmaphub.logout().await
}

#[tauri::command]
pub async fn get_beatmaphub_profile(state: State<'_, AppState>) -> CommandResult<Profile> {
    state.beatmaphub.profile().await
}

#[tauri::command]
pub async fn create_beatmaphub_device_link(
    state: State<'_, AppState>,
) -> CommandResult<LinkTokenResponse> {
    state.beatmaphub.create_device_link().await
}

#[tauri::command]
pub async fn revoke_beatmaphub_device(
    device_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.revoke_device(&device_id).await
}

#[tauri::command]
pub async fn get_beatmaphub_pack(
    share_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Pack> {
    state.beatmaphub.get_pack(&share_id).await
}

#[tauri::command]
pub async fn get_beatmaphub_recommendations(
    limit: Option<u8>,
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Pack>> {
    state
        .beatmaphub
        .recommendations(limit.unwrap_or(20), force_refresh.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn search_beatmaphub_packs(
    query: String,
    limit: Option<u8>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Pack>> {
    state.beatmaphub.search(&query, limit.unwrap_or(20)).await
}

#[tauri::command]
pub async fn preview_beatmaphub_pack(
    share_id: String,
    state: State<'_, AppState>,
) -> CommandResult<PackPreview> {
    state.beatmaphub.preview_pack(&state, &share_id).await
}

#[tauri::command]
pub async fn publish_beatmaphub_pack(
    folder_id: String,
    title: String,
    description: String,
    is_private: bool,
    state: State<'_, AppState>,
) -> CommandResult<PublishResult> {
    state
        .beatmaphub
        .publish(&state, &folder_id, title, description, is_private)
        .await
}

#[tauri::command]
pub async fn update_beatmaphub_pack(
    share_id: String,
    folder_id: String,
    title: String,
    description: String,
    is_private: bool,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state
        .beatmaphub
        .update_pack(
            &state,
            &share_id,
            &folder_id,
            title,
            description,
            is_private,
        )
        .await
}

#[tauri::command]
pub async fn delete_beatmaphub_pack(
    share_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.delete_pack(&share_id).await
}

#[tauri::command]
pub async fn rate_beatmaphub_pack(
    share_id: String,
    score: u8,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.rate(&share_id, score).await
}

#[tauri::command]
pub async fn favorite_beatmaphub_pack(
    share_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.favorite(&share_id, enabled).await
}

#[tauri::command]
pub async fn like_beatmaphub_pack(
    share_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.like(&share_id, enabled).await
}

#[tauri::command]
pub async fn get_beatmaphub_comments(
    share_id: String,
    limit: Option<u8>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<PackComment>> {
    state
        .beatmaphub
        .comments(&share_id, limit.unwrap_or(50))
        .await
}

#[tauri::command]
pub async fn create_beatmaphub_comment(
    share_id: String,
    content: String,
    state: State<'_, AppState>,
) -> CommandResult<PackComment> {
    state.beatmaphub.create_comment(&share_id, content).await
}

#[tauri::command]
pub async fn update_beatmaphub_comment(
    comment_id: String,
    content: String,
    state: State<'_, AppState>,
) -> CommandResult<PackComment> {
    state.beatmaphub.update_comment(&comment_id, content).await
}

#[tauri::command]
pub async fn delete_beatmaphub_comment(
    comment_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.delete_comment(&comment_id).await
}

#[tauri::command]
pub async fn import_beatmaphub_pack(
    share_id: String,
    resolved: Vec<ResolvedBeatmapset>,
    state: State<'_, AppState>,
) -> CommandResult<ImportResult> {
    state
        .beatmaphub
        .import_pack(&state, &share_id, resolved)
        .await
}
