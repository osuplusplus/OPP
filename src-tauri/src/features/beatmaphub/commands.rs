//! BeatmapHub 的 Tauri 命令薄层。
use tauri::State;

use crate::{error::CommandResult, state::AppState};

use super::models::*;

/// 查询 BeatmapHub 认证状态与身份信息。
#[tauri::command]
pub fn get_beatmaphub_auth_status(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    state.beatmaphub.status()
}

/// 以当前 osu 登录身份连接 BeatmapHub。
#[tauri::command]
pub async fn login_beatmaphub(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    bootstrap_from_osu(&state).await
}

/// 与 `login_beatmaphub` 等价的兼容入口。
#[tauri::command]
pub async fn bootstrap_beatmaphub(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    bootstrap_from_osu(&state).await
}

/// 重置本地设备身份后重新连接 BeatmapHub。
#[tauri::command]
pub async fn reconnect_beatmaphub(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    let (user_id, username) = state
        .store
        .read(|persisted| (persisted.current_user_id, persisted.username.clone()))?;
    state
        .beatmaphub
        .reconnect(
            user_id.ok_or_else(crate::error::CommandError::auth_required)?,
            username.ok_or_else(crate::error::CommandError::auth_required)?,
        )
        .await
}

/// 从本地持久化读取 osu 身份并发起 bootstrap，未登录 osu 时报认证错误。
async fn bootstrap_from_osu(state: &AppState) -> CommandResult<AuthStatus> {
    let (user_id, username) = state
        .store
        .read(|persisted| (persisted.current_user_id, persisted.username.clone()))?;
    let user_id = user_id.ok_or_else(crate::error::CommandError::auth_required)?;
    let username = username.ok_or_else(crate::error::CommandError::auth_required)?;
    state.beatmaphub.bootstrap(user_id, username).await
}

/// 注销 BeatmapHub 会话（本地保留设备身份）。
#[tauri::command]
pub async fn logout_beatmaphub(state: State<'_, AppState>) -> CommandResult<()> {
    state.beatmaphub.logout().await
}

/// 获取当前用户资料与已绑定设备列表。
#[tauri::command]
pub async fn get_beatmaphub_profile(state: State<'_, AppState>) -> CommandResult<Profile> {
    state.beatmaphub.profile().await
}

/// 吊销指定设备。
#[tauri::command]
pub async fn revoke_beatmaphub_device(
    device_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.revoke_device(&device_id).await
}

/// 按分享码获取曲包详情。
#[tauri::command]
pub async fn get_beatmaphub_pack(
    share_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Pack> {
    state.beatmaphub.get_pack(&share_id).await
}

/// 获取推荐曲包，`limit` 缺省为 20。
#[tauri::command]
pub async fn get_beatmaphub_recommendations(
    limit: Option<u8>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Pack>> {
    state.beatmaphub.recommendations(limit.unwrap_or(20)).await
}

/// 按关键词搜索曲包，`limit` 缺省为 20。
#[tauri::command]
pub async fn search_beatmaphub_packs(
    query: String,
    limit: Option<u8>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Pack>> {
    state.beatmaphub.search(&query, limit.unwrap_or(20)).await
}

/// 预览曲包，返回本地已有/缺失谱面集的拆分结果。
#[tauri::command]
pub async fn preview_beatmaphub_pack(
    share_id: String,
    state: State<'_, AppState>,
) -> CommandResult<PackPreview> {
    state.beatmaphub.preview_pack(&state, &share_id).await
}

/// 将收藏夹发布为曲包。
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

/// 用收藏夹内容更新已有曲包。
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

/// 删除曲包。
#[tauri::command]
pub async fn delete_beatmaphub_pack(
    share_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.delete_pack(&share_id).await
}

/// 为曲包打 1 到 5 星评分。
#[tauri::command]
pub async fn rate_beatmaphub_pack(
    share_id: String,
    score: u8,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.rate(&share_id, score).await
}

/// 收藏或取消收藏曲包。
#[tauri::command]
pub async fn favorite_beatmaphub_pack(
    share_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.favorite(&share_id, enabled).await
}

/// 点赞或取消点赞曲包。
#[tauri::command]
pub async fn like_beatmaphub_pack(
    share_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.like(&share_id, enabled).await
}

/// 拉取曲包评论，`limit` 缺省为 50。
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

/// 发表曲包评论。
#[tauri::command]
pub async fn create_beatmaphub_comment(
    share_id: String,
    content: String,
    state: State<'_, AppState>,
) -> CommandResult<PackComment> {
    state.beatmaphub.create_comment(&share_id, content).await
}

/// 编辑自己发表的评论。
#[tauri::command]
pub async fn update_beatmaphub_comment(
    comment_id: String,
    content: String,
    state: State<'_, AppState>,
) -> CommandResult<PackComment> {
    state.beatmaphub.update_comment(&comment_id, content).await
}

/// 删除评论。
#[tauri::command]
pub async fn delete_beatmaphub_comment(
    comment_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state.beatmaphub.delete_comment(&comment_id).await
}

/// 导入曲包为本地收藏夹，`resolved` 为前端解析的谱面集元数据。
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

/// 把本地谱面集 ID 列表整体上传为云端备份（覆盖式替换）。
#[tauri::command]
pub async fn upload_beatmap_backup(
    state: State<'_, AppState>,
) -> CommandResult<BackupUploadResult> {
    state.beatmaphub.upload_backup(&state).await
}

/// 读取云端谱面备份，返回与本地库的已有 / 缺失统计。
#[tauri::command]
pub async fn get_beatmap_backup(state: State<'_, AppState>) -> CommandResult<BeatmapBackup> {
    state.beatmaphub.get_backup(&state).await
}

/// 删除云端谱面备份。
#[tauri::command]
pub async fn delete_beatmap_backup(state: State<'_, AppState>) -> CommandResult<()> {
    state.beatmaphub.delete_backup().await
}

/// 把云端谱面备份恢复为新的本地收藏夹。
#[tauri::command]
pub async fn restore_beatmap_backup(
    state: State<'_, AppState>,
) -> CommandResult<BackupRestoreResult> {
    state.beatmaphub.restore_backup(&state).await
}
