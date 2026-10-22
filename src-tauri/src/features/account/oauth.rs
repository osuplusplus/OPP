//! osu! OAuth 授权码登录：在本机 42831 端口接收浏览器回调，以随机 `state` 防 CSRF，
//! 授权码就地换取令牌并写入系统凭据存储。
use chrono::{Duration as ChronoDuration, Utc};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    time::{Duration, Instant, timeout},
};
use url::Url;
use uuid::Uuid;

use crate::{
    domain::{OAuthResult, PendingOAuth, TokenSet},
    error::{CommandError, CommandResult},
    state::AppState,
};

/// 浏览器回调地址，需与 osu! OAuth 应用中配置的 Callback URL 完全一致。
pub const CALLBACK_URL: &str = "http://127.0.0.1:42831/oauth/callback";
/// 回调 TCP 监听地址，对应 `CALLBACK_URL` 的主机与端口。
const CALLBACK_ADDRESS: &str = "127.0.0.1:42831";
const AUTHORIZATION_URL: &str = "https://osu.ppy.sh/oauth/authorize";

/// 建立一次仅在本机回调端口上有效的 OAuth 会话，并用随机 state 抵御 CSRF 回调。
pub async fn begin(app: AppHandle) -> CommandResult<PendingOAuth> {
    let app_state = app.state::<AppState>();
    let client_id = app_state
        .store
        .read(|state| state.client_id.clone())?
        .filter(|value| !value.is_empty())
        .ok_or_else(CommandError::credentials_required)?;
    if app_state.credentials.get_client_secret()?.is_none() {
        return Err(CommandError::credentials_required());
    }

    // 先绑定回调端口再返回授权链接：端口被占用时立即报错，用户不必白跑浏览器。
    let listener = TcpListener::bind(CALLBACK_ADDRESS).await.map_err(|error| {
        CommandError::new(
            "CALLBACK_PORT_OCCUPIED",
            format!("OAuth 回调端口 42831 无法使用：{error}"),
        )
    })?;

    let csrf_state = Uuid::new_v4().simple().to_string();
    let authorization_url = build_authorization_url(&client_id, &csrf_state)?;
    let (cancel_tx, cancel_rx) = oneshot::channel();

    // 全局同一时刻只保留一个待授权会话：先取消旧等待任务，再写入新会话的 state。
    {
        let mut runtime = app_state
            .oauth
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "OAuth 状态锁已损坏"))?;
        if let Some(previous) = runtime.cancel.take() {
            let _ = previous.send(());
        }
        runtime.state = Some(csrf_state.clone());
        runtime.cancel = Some(cancel_tx);
    }

    let app_for_task = app.clone();
    let session_state = csrf_state.clone();
    // 后台等待浏览器回调，结果通过 `oauth-result` 事件推送给前端。
    tauri::async_runtime::spawn(async move {
        let result = wait_for_callback(
            app_for_task.clone(),
            listener,
            session_state.clone(),
            cancel_rx,
        )
        .await;
        let event = match result {
            Ok(()) => OAuthResult {
                ok: true,
                code: "CONNECTED".into(),
                message: "osu! 账号已连接".into(),
            },
            Err(error) => OAuthResult {
                ok: false,
                code: error.code,
                message: error.message,
            },
        };
        let _ = app_for_task.emit("oauth-result", event);

        let state = app_for_task.state::<AppState>();
        // 仅当仍是同一会话时才清理，避免误清后续新会话写入的 state。
        if let Ok(mut runtime) = state.oauth.lock()
            && runtime.state.as_deref() == Some(session_state.as_str())
        {
            runtime.cancel = None;
            runtime.state = None;
        }
    });

    // 会话有效期 5 分钟，与回调等待超时（300 秒）保持一致。
    Ok(PendingOAuth {
        authorization_url,
        expires_at: Utc::now() + ChronoDuration::minutes(5),
    })
}

/// 取消当前授权等待；接收端任务会收到 oneshot 信号并清理与该会话绑定的 state。
pub fn cancel(app_state: &AppState) -> CommandResult<()> {
    let mut runtime = app_state
        .oauth
        .lock()
        .map_err(|_| CommandError::new("STATE_ERROR", "OAuth 状态锁已损坏"))?;
    if let Some(cancel) = runtime.cancel.take() {
        let _ = cancel.send(());
    }
    runtime.state = None;
    Ok(())
}

/// 拼接 osu! 授权页链接；`scope` 固定为 `public identify`，不含写权限。
fn build_authorization_url(client_id: &str, csrf_state: &str) -> CommandResult<String> {
    // state 与浏览器回调进行一一比对，不能省略或复用。
    let mut url = Url::parse(AUTHORIZATION_URL)
        .map_err(|error| CommandError::new("INVALID_URL", error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", CALLBACK_URL)
        .append_pair("response_type", "code")
        .append_pair("scope", "public identify")
        .append_pair("state", csrf_state);
    Ok(url.into())
}

/// 在回调端口循环等待：匹配的回调结束流程；陈旧 `state` 或路径不符的回调在回复
/// 浏览器后忽略并继续等待，直到成功、失败、超时（300 秒）或被取消。
async fn wait_for_callback(
    app: AppHandle,
    listener: TcpListener,
    expected_state: String,
    mut cancel_rx: oneshot::Receiver<()>,
) -> CommandResult<()> {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CommandError::new("AUTH_TIMEOUT", "授权等待已超时"));
        }
        let accepted = tokio::select! {
            accepted = timeout(remaining, listener.accept()) => {
                match accepted {
                    Ok(Ok(value)) => value,
                    Ok(Err(error)) => return Err(CommandError::new("CALLBACK_FAILED", error.to_string())),
                    Err(_) => return Err(CommandError::new("AUTH_TIMEOUT", "授权等待已超时")),
                }
            }
            _ = &mut cancel_rx => {
                return Err(CommandError::new("AUTH_CANCELLED", "授权已取消"));
            }
        };

        let (mut stream, _) = accepted;
        let result = process_callback(&app, &mut stream, &expected_state).await;
        let ignored_callback = is_ignorable_callback(&result);
        let (title, message, success) = match &result {
            Ok(()) => (
                "连接成功",
                "OPP 已安全连接到你的 osu! 账号，可以关闭此页面。",
                true,
            ),
            Err(error) if ignored_callback => ("授权请求未匹配", error.message.as_str(), false),
            Err(error) => ("连接失败", error.message.as_str(), false),
        };
        write_browser_response(&mut stream, title, message, success).await;

        if ignored_callback {
            continue;
        }
        return result;
    }
}

/// 判定回调错误能否忽略：陈旧 `state` 或路径不符通常来自旧会话与错误配置，
/// 忽略后当前会话可继续等待真正的回调。
fn is_ignorable_callback(result: &CommandResult<()>) -> bool {
    matches!(
        result,
        Err(error)
            if matches!(
                error.code.as_str(),
                "INVALID_OAUTH_STATE" | "CALLBACK_PATH_INVALID"
            )
    )
}

/// 校验回调参数并用授权码换取令牌，随后写入凭据存储并重置本地用户缓存。
async fn process_callback(
    app: &AppHandle,
    stream: &mut TcpStream,
    expected_state: &str,
) -> CommandResult<()> {
    let mut buffer = vec![0_u8; 8192];
    let bytes = timeout(Duration::from_secs(10), stream.read(&mut buffer))
        .await
        .map_err(|_| CommandError::new("CALLBACK_FAILED", "未收到浏览器回调"))?
        .map_err(|error| CommandError::new("CALLBACK_FAILED", error.to_string()))?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let callback = parse_callback_request(&request)?;

    if callback.state.as_deref() != Some(expected_state) {
        return Err(CommandError::new(
            "INVALID_OAUTH_STATE",
            "授权校验失败，请重新尝试",
        ));
    }
    if let Some(error) = callback.error {
        return Err(CommandError::new(
            "AUTH_DENIED",
            callback
                .error_description
                .unwrap_or_else(|| format!("osu! 拒绝授权：{error}")),
        ));
    }
    let code = callback
        .code
        .ok_or_else(|| CommandError::new("MISSING_AUTH_CODE", "回调中缺少授权码"))?;

    let app_state = app.state::<AppState>();
    let client_id = app_state
        .store
        .read(|state| state.client_id.clone())?
        .ok_or_else(CommandError::credentials_required)?;
    let client_secret = app_state
        .credentials
        .get_client_secret()?
        .ok_or_else(CommandError::credentials_required)?;
    let response = app_state
        .api
        .exchange_code(&client_id, &client_secret, &code, CALLBACK_URL)
        .await?;
    let tokens = TokenSet {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at: Utc::now() + ChronoDuration::seconds(response.expires_in),
    };
    app_state.credentials.set_tokens(&tokens)?;
    // 令牌已换绑新账号：旧的用户身份、缓存与手动刷新记录全部作废。
    app_state.store.update(|state| {
        state.token_expires_at = Some(tokens.expires_at);
        state.current_user_id = None;
        state.username = None;
        state.cache.clear();
        state.last_manual_refresh.clear();
    })?;
    Ok(())
}

/// 从回调查询串提取的字段；`error` 与 `error_description` 仅在用户拒绝授权时出现。
#[derive(Debug, PartialEq, Eq)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

fn parse_callback_request(request: &str) -> CommandResult<CallbackQuery> {
    // 只解析 HTTP 请求行，不把浏览器提供的正文或头部当作可信输入。
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| CommandError::new("CALLBACK_FAILED", "OAuth 回调请求为空"))?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| CommandError::new("CALLBACK_FAILED", "OAuth 回调格式无效"))?;
    let url = Url::parse(&format!("http://127.0.0.1{path}"))
        .map_err(|error| CommandError::new("CALLBACK_FAILED", error.to_string()))?;
    if !matches!(url.path(), "/oauth/callback" | "/oauth/callback/") {
        return Err(CommandError::new(
            "CALLBACK_PATH_INVALID",
            format!(
                "收到的回调路径是 {}，请将 osu! OAuth Callback URL 设置为 {CALLBACK_URL}",
                url.path()
            ),
        ));
    }
    let values = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    Ok(CallbackQuery {
        code: values.get("code").map(|value| value.to_string()),
        state: values.get("state").map(|value| value.to_string()),
        error: values.get("error").map(|value| value.to_string()),
        error_description: values
            .get("error_description")
            .map(|value| value.to_string()),
    })
}

/// 向浏览器回写极简结果页；标题与消息先做 HTML 转义，防止回调内容注入页面。
async fn write_browser_response(stream: &mut TcpStream, title: &str, message: &str, success: bool) {
    let color = if success { "#5ce1e6" } else { "#ff6a9d" };
    let safe_title = escape_html(title);
    let safe_message = escape_html(message);
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{safe_title}</title><body style=\"margin:0;background:#080b14;color:#f6f7fb;font:16px system-ui;display:grid;place-items:center;min-height:100vh\"><main style=\"max-width:520px;padding:48px;border:1px solid #ffffff1a;border-radius:24px;background:#121726;text-align:center;box-shadow:0 24px 90px #0008\"><div style=\"width:56px;height:56px;margin:auto;border:4px solid {color};border-radius:50%;box-shadow:0 0 36px {color}\"></div><h1>{safe_title}</h1><p style=\"color:#a8afc3;line-height:1.7\">{safe_message}</p></main></body>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 授权链接必须携带回调地址与 `state`，缺失任一项都视为安全回归。
    #[test]
    fn authorization_url_contains_required_security_values() {
        let url = build_authorization_url("42", "state-value").expect("url");
        let parsed = Url::parse(&url).expect("valid url");
        let values = parsed
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            values.get("client_id").map(|value| value.as_ref()),
            Some("42")
        );
        assert_eq!(
            values.get("state").map(|value| value.as_ref()),
            Some("state-value")
        );
        assert_eq!(
            values.get("redirect_uri").map(|value| value.as_ref()),
            Some(CALLBACK_URL)
        );
        assert_eq!(
            values.get("scope").map(|value| value.as_ref()),
            Some("public identify")
        );
    }

    // 成功与用户拒绝两类回调都必须解析出对应字段。
    #[test]
    fn callback_parser_handles_success_and_denial() {
        let success = parse_callback_request("GET /oauth/callback?code=abc&state=xyz HTTP/1.1\r\n")
            .expect("success callback");
        assert_eq!(success.code.as_deref(), Some("abc"));
        assert_eq!(success.state.as_deref(), Some("xyz"));

        let denied = parse_callback_request(
            "GET /oauth/callback?error=access_denied&error_description=No HTTP/1.1\r\n",
        )
        .expect("denied callback");
        assert_eq!(denied.error.as_deref(), Some("access_denied"));
    }

    #[test]
    fn only_stale_or_unrelated_callbacks_are_ignored_while_waiting() {
        assert!(is_ignorable_callback(&Err(CommandError::new(
            "INVALID_OAUTH_STATE",
            "stale",
        ))));
        assert!(is_ignorable_callback(&Err(CommandError::new(
            "CALLBACK_PATH_INVALID",
            "wrong path",
        ))));
        assert!(!is_ignorable_callback(&Err(CommandError::new(
            "AUTH_DENIED",
            "denied",
        ))));
    }

    #[test]
    fn accepts_the_callback_path_with_or_without_a_trailing_slash() {
        for path in ["/oauth/callback", "/oauth/callback/"] {
            let callback =
                parse_callback_request(&format!("GET {path}?code=abc&state=xyz HTTP/1.1\\r\\n"))
                    .expect("valid callback path");
            assert_eq!(callback.code.as_deref(), Some("abc"));
        }
    }

    #[test]
    fn reports_the_received_invalid_callback_path() {
        let error = parse_callback_request("GET / HTTP/1.1\\r\\n").unwrap_err();
        assert_eq!(error.code, "CALLBACK_PATH_INVALID");
        assert!(error.message.contains(CALLBACK_URL));
    }

    #[test]
    fn browser_message_is_html_escaped() {
        assert_eq!(
            escape_html("<script>alert('x')</script>"),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"
        );
    }
}
