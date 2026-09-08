use chrono::{Duration, Utc};
use tauri::{AppHandle, State};

use crate::{
    domain::{
        AppSettings, AuthStatus, CacheRecord, Cached, DisconnectResult, OwnProfile, PendingOAuth,
        Ruleset, SavedCredentials, Score, ScoreCategory,
    },
    error::{CommandError, CommandResult},
    state::AppState,
};

use super::{oauth, token::ensure_access_token};

const PROFILE_CACHE_SECONDS: i64 = 300;
const SCORE_CACHE_SECONDS: i64 = 600;
const MANUAL_REFRESH_SECONDS: i64 = 60;

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_auth_status(state: State<'_, AppState>) -> CommandResult<AuthStatus> {
    let snapshot = state.store.snapshot()?;
    let has_secret = state.credentials.get_client_secret()?.is_some();
    let tokens = state.credentials.get_tokens()?;
    Ok(AuthStatus {
        credentials_configured: snapshot.client_id.is_some() && has_secret,
        connected: tokens.is_some(),
        client_id: snapshot.client_id,
        callback_url: oauth::CALLBACK_URL.into(),
        user_id: snapshot.current_user_id,
        username: snapshot.username,
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：校验并持久化用户配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn save_oauth_credentials(
    client_id: String,
    client_secret: String,
    state: State<'_, AppState>,
) -> CommandResult<SavedCredentials> {
    let client_id = client_id.trim().to_string();
    let client_secret = client_secret.trim();
    if client_id.is_empty()
        || !client_id
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(CommandError::new(
            "INVALID_CLIENT_ID",
            "Client ID 必须是数字",
        ));
    }
    if client_secret.len() < 16 {
        return Err(CommandError::new(
            "INVALID_CLIENT_SECRET",
            "Client Secret 格式无效",
        ));
    }

    state.credentials.set_client_secret(client_secret)?;
    state.credentials.clear_tokens()?;
    state.avatar_cache.clear()?;
    state.store.update(|persisted| {
        persisted.client_id = Some(client_id.clone());
        persisted.token_expires_at = None;
        persisted.current_user_id = None;
        persisted.username = None;
        persisted.cache.clear();
        persisted.last_manual_refresh.clear();
    })?;

    Ok(SavedCredentials {
        client_id,
        callback_url: oauth::CALLBACK_URL.into(),
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：初始化可取消的异步流程。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn begin_oauth_login(app: AppHandle) -> CommandResult<PendingOAuth> {
    oauth::begin(app).await
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_oauth_login(state: State<'_, AppState>) -> CommandResult<()> {
    oauth::cancel(&state)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn disconnect_osu(
    revoke: bool,
    state: State<'_, AppState>,
) -> CommandResult<DisconnectResult> {
    let mut revoked = false;
    let mut warning = None;
    if revoke && let Some(tokens) = state.credentials.get_tokens()? {
        match state.api.revoke_current_token(&tokens.access_token).await {
            Ok(()) => revoked = true,
            Err(error) => warning = Some(error.message),
        }
    }

    state.credentials.clear_tokens()?;
    state.avatar_cache.clear()?;
    state.store.update(|persisted| {
        persisted.token_expires_at = None;
        persisted.current_user_id = None;
        persisted.username = None;
        persisted.cache.clear();
        persisted.last_manual_refresh.clear();
    })?;
    Ok(DisconnectResult { revoked, warning })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_own_profile(
    ruleset: Ruleset,
    force_refresh: bool,
    state: State<'_, AppState>,
) -> CommandResult<Cached<OwnProfile>> {
    let key = format!("profile:{ruleset}");
    let snapshot = state.store.snapshot()?;
    let cached = snapshot.cache.get(&key).cloned();
    if !force_refresh {
        if let Some(record) = cached.as_ref()
            && Utc::now() - record.fetched_at < Duration::seconds(PROFILE_CACHE_SECONDS)
        {
            let mut cached_profile = profile_from_cache(record, false)?;
            attach_avatar(&state, &mut cached_profile.data, false).await;
            return Ok(cached_profile);
        }
    } else {
        enforce_manual_cooldown(&state, &key)?;
    }

    let access_token = ensure_access_token(&state).await?;
    match state.api.get_own_profile(&access_token, ruleset).await {
        Ok(mut profile) => {
            let fetched_at = Utc::now();
            let value = serde_json::to_value(&profile)?;
            state.store.update(|persisted| {
                persisted.current_user_id = Some(profile.id);
                persisted.username = Some(profile.username.clone());
                persisted
                    .cache
                    .insert(key, CacheRecord { value, fetched_at });
            })?;
            attach_avatar(&state, &mut profile, force_refresh).await;
            Ok(Cached {
                data: profile,
                fetched_at,
                stale: false,
            })
        }
        Err(error) if can_use_stale_cache(&error) && cached.is_some() => {
            let mut cached_profile =
                profile_from_cache(&cached.expect("cache checked above"), true)?;
            attach_avatar(&state, &mut cached_profile.data, false).await;
            Ok(cached_profile)
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_scores(
    ruleset: Ruleset,
    category: ScoreCategory,
    offset: u32,
    limit: u8,
    force_refresh: bool,
    state: State<'_, AppState>,
) -> CommandResult<Cached<Vec<Score>>> {
    if limit == 0 || limit > 100 {
        return Err(CommandError::new(
            "INVALID_SCORE_LIMIT",
            "成绩请求数量必须在 1 到 100 之间。",
        ));
    }
    let key = format!("scores:{ruleset}:{category}:{offset}:{limit}");
    let snapshot = state.store.snapshot()?;
    let cached = snapshot.cache.get(&key).cloned();
    if !force_refresh {
        if let Some(record) = cached.as_ref()
            && Utc::now() - record.fetched_at < Duration::seconds(SCORE_CACHE_SECONDS)
        {
            return scores_from_cache(record, false);
        }
    } else {
        enforce_manual_cooldown(&state, &key)?;
    }

    let user_id = snapshot
        .current_user_id
        .ok_or_else(|| CommandError::new("PROFILE_REQUIRED", "请先加载个人资料，再查看最佳成绩"))?;
    let access_token = ensure_access_token(&state).await?;
    match state
        .api
        .get_user_scores(&access_token, user_id, ruleset, category, offset, limit)
        .await
    {
        Ok(scores) => {
            let fetched_at = Utc::now();
            let value = serde_json::to_value(&scores)?;
            state.store.update(|persisted| {
                persisted
                    .cache
                    .insert(key, CacheRecord { value, fetched_at });
            })?;
            Ok(Cached {
                data: scores,
                fetched_at,
                stale: false,
            })
        }
        Err(error) if can_use_stale_cache(&error) && cached.is_some() => {
            scores_from_cache(&cached.expect("cache checked above"), true)
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：清除可安全重建的本地缓存。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn clear_profile_cache(state: State<'_, AppState>) -> CommandResult<()> {
    state.store.update(|persisted| {
        persisted.cache.clear();
        persisted.last_manual_refresh.clear();
    })?;
    state.avatar_cache.clear()
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    state.store.update(|persisted| {
        if persisted.settings.beatmap_download_directory.is_none() {
            persisted.settings.beatmap_download_directory = default_download_directory();
        }
        persisted.settings.clone()
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：记录用户已完成的引导状态。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn mark_onboarding_seen(
    version: u32,
    state: State<'_, AppState>,
) -> CommandResult<AppSettings> {
    state.store.update(|persisted| {
        persisted.settings.onboarding_version = persisted.settings.onboarding_version.max(version);
        persisted.settings.clone()
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：记录用户已完成的引导状态。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn mark_page_onboarding_seen(
    page_id: String,
    version: u32,
    state: State<'_, AppState>,
) -> CommandResult<AppSettings> {
    if page_id.is_empty()
        || page_id.len() > 64
        || !page_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CommandError::new("INVALID_PAGE_ID", "页面引导标识无效"));
    }
    state.store.update(|persisted| {
        let stored = persisted
            .settings
            .page_onboarding_versions
            .entry(page_id)
            .or_default();
        *stored = (*stored).max(version);
        persisted.settings.clone()
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：更新持久化设置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn update_settings(
    mut settings: AppSettings,
    state: State<'_, AppState>,
) -> CommandResult<AppSettings> {
    settings.cache_limit_mb = settings.cache_limit_mb.clamp(64, 10_240);
    if settings.beatmap_download_directory.is_none() {
        settings.beatmap_download_directory = default_download_directory();
    }
    state
        .local_analysis
        .set_thumbnail_cache_limit_mb(settings.cache_limit_mb)?;
    state
        .store
        .update(|persisted| persisted.settings = settings.clone())?;
    Ok(settings)
}

fn default_download_directory() -> Option<String> {
    crate::infrastructure::platform::default_download_dir().map(|path| path.display().to_string())
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：将资源导出到用户指定的位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn export_replay_video(
    video_url: String,
    file_name: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let parsed = url::Url::parse(video_url.trim())
        .map_err(|_| CommandError::new("INVALID_VIDEO_URL", "视频链接无效"))?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| host == "issou.best" || host.ends_with(".issou.best"))
    {
        return Err(CommandError::new(
            "VIDEO_HOST_NOT_ALLOWED",
            "仅允许导出 o!rdr 官方视频链接",
        ));
    }
    let directory = state
        .store
        .snapshot()?
        .settings
        .replay_export_directory
        .ok_or_else(|| {
            CommandError::new(
                "REPLAY_EXPORT_DIRECTORY_NOT_SET",
                "请先在设置中选择回放导出位置",
            )
        })?;
    std::fs::create_dir_all(&directory)?;
    let safe_name: String = file_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || ".-_()[]".contains(character) {
                character
            } else {
                '_'
            }
        })
        .collect();
    let safe_name = if safe_name.trim_matches(['.', '_']).is_empty() {
        "opp-replay.mp4".into()
    } else if safe_name.to_ascii_lowercase().ends_with(".mp4") {
        safe_name
    } else {
        format!("{safe_name}.mp4")
    };
    let target = std::path::Path::new(&directory).join(safe_name);
    let response = reqwest::Client::new()
        .get(parsed)
        .send()
        .await
        .map_err(|error| CommandError::network(format!("视频下载失败：{error}")))?
        .error_for_status()
        .map_err(|error| CommandError::network(format!("视频下载失败：{error}")))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| CommandError::network(format!("视频读取失败：{error}")))?;
    std::fs::write(&target, bytes)?;
    Ok(target.display().to_string())
}

fn enforce_manual_cooldown(state: &AppState, key: &str) -> CommandResult<()> {
    // 强制刷新会消耗远程 API 配额；按缓存键分别限流，避免一个页面影响另一个页面。
    let now = Utc::now();
    state.store.update(|persisted| {
        if let Some(previous) = persisted.last_manual_refresh.get(key) {
            let elapsed = now.signed_duration_since(*previous).num_seconds();
            if elapsed < MANUAL_REFRESH_SECONDS {
                return Err(CommandError::new("REFRESH_COOLDOWN", "请稍后再手动刷新")
                    .retry_after(Some((MANUAL_REFRESH_SECONDS - elapsed) as u64)));
            }
        }
        persisted.last_manual_refresh.insert(key.into(), now);
        Ok(())
    })?
}

fn profile_from_cache(record: &CacheRecord, stale: bool) -> CommandResult<Cached<OwnProfile>> {
    Ok(Cached {
        data: serde_json::from_value(record.value.clone())?,
        fetched_at: record.fetched_at,
        stale,
    })
}

#[tauri::command]
/// 结算屏头像落盘路径:确保账号头像已按 avatar_url 缓存到本地
/// (image-cache/avatar-{id}.bin),返回文件路径供渲染器解码;无则 None。
pub async fn resolve_avatar_file(
    user_id: u64,
    avatar_url: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    state
        .avatar_cache
        .load_or_fetch(user_id, &avatar_url, false)
        .await?;
    Ok(Some(
        state
            .avatar_cache
            .file_path(user_id)
            .to_string_lossy()
            .into_owned(),
    ))
}

async fn attach_avatar(state: &AppState, profile: &mut OwnProfile, force_refresh: bool) {
    // 头像是辅助展示数据：下载失败不应使个人资料请求整体失败。
    profile.avatar_data_url = state
        .avatar_cache
        .load_or_fetch(profile.id, &profile.avatar_url, force_refresh)
        .await
        .unwrap_or(None);
}

fn scores_from_cache(record: &CacheRecord, stale: bool) -> CommandResult<Cached<Vec<Score>>> {
    Ok(Cached {
        data: serde_json::from_value(record.value.clone())?,
        fetched_at: record.fetched_at,
        stale,
    })
}

fn can_use_stale_cache(error: &CommandError) -> bool {
    matches!(
        error.code.as_str(),
        "NETWORK_ERROR" | "SERVER_ERROR" | "RATE_LIMITED"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_profile_can_be_marked_stale() {
        let record = CacheRecord {
            value: serde_json::json!({
                "id": 1,
                "username": "cached",
                "avatar_url": "https://example.test/avatar.png",
                "country_code": "CN"
            }),
            fetched_at: Utc::now(),
        };
        let cached = profile_from_cache(&record, true).expect("cache");
        assert!(cached.stale);
        assert_eq!(cached.data.username, "cached");
    }

    #[test]
    fn only_transient_errors_allow_stale_data() {
        assert!(can_use_stale_cache(&CommandError::network("offline")));
        assert!(!can_use_stale_cache(&CommandError::auth_required()));
    }
}
