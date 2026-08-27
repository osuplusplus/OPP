//! Access-token refresh lifecycle shared by authenticated domains.

use chrono::{Duration, Utc};

use crate::{
    domain::TokenSet,
    error::{CommandError, CommandResult},
    state::AppState,
};

/// Returns a usable OAuth access token, refreshing it once when it is near expiry.
///
/// The mutex deliberately covers the whole refresh exchange so concurrent commands do not
/// invalidate each other's refresh token.
pub(crate) async fn ensure_access_token(state: &AppState) -> CommandResult<String> {
    // 令牌仍有余量时直接复用；临近过期才走刷新流程，减少不必要的 OAuth 请求。
    let _refresh_guard = state.token_refresh.lock().await;
    let tokens = state
        .credentials
        .get_tokens()?
        .ok_or_else(CommandError::auth_required)?;
    if tokens.expires_at > Utc::now() + Duration::seconds(60) {
        return Ok(tokens.access_token);
    }

    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .ok_or_else(CommandError::auth_required)?;
    let snapshot = state.store.snapshot()?;
    let client_id = snapshot
        .client_id
        .ok_or_else(CommandError::credentials_required)?;
    let client_secret = state
        .credentials
        .get_client_secret()?
        .ok_or_else(CommandError::credentials_required)?;
    let response = match state
        .api
        .refresh_token(&client_id, &client_secret, refresh_token)
        .await
    {
        Ok(response) => response,
        Err(_) => {
            state.credentials.clear_tokens()?;
            state
                .store
                .update(|persisted| persisted.token_expires_at = None)?;
            return Err(CommandError::auth_required());
        }
    };
    let refreshed = TokenSet {
        access_token: response.access_token,
        refresh_token: response.refresh_token.or(tokens.refresh_token),
        expires_at: Utc::now() + Duration::seconds(response.expires_in),
    };
    state.credentials.set_tokens(&refreshed)?;
    state
        .store
        .update(|persisted| persisted.token_expires_at = Some(refreshed.expires_at))?;
    Ok(refreshed.access_token)
}
