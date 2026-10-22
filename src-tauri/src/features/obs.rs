//! OBS 集成：通过 obs-websocket v5 协议连接本机 OBS，
//! 提供场景浏览、浏览器源刷新与 OBS 进程状态监控。
use std::{collections::HashSet, sync::Mutex, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    features::tosu::start_managed_tosu,
    state::AppState,
};

/// 前端展示用的 OBS 综合状态快照：进程、连接、密码配置与所选场景。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObsStatus {
    /// OBS 进程当前是否在运行（按平台探测进程名）。
    pub running: bool,
    pub websocket_url: String,
    /// 本次查询实测的 WebSocket 握手结果，不持久保存。
    pub connected: bool,
    /// 是否已保存 WebSocket 密码；密码本体只存凭据管理器，不回传前端。
    pub password_configured: bool,
    pub selected_scene: Option<String>,
    /// 最近一次连接失败的展示原因，成功时为 `None`。
    pub last_error: Option<String>,
}

/// 刷新所选场景浏览器源的结果：成功的源名列表或跳过原因。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsRefreshResult {
    pub refreshed_sources: Vec<String>,
    /// 因尚未选择场景等原因未执行刷新时为 `true`。
    pub skipped: bool,
    pub message: String,
}

/// OBS 运行时共享状态：互斥锁内保存最近一次状态快照，供命令与后台监控读写。
pub struct ObsRuntime {
    pub current: Mutex<ObsStatus>,
}
impl Default for ObsRuntime {
    fn default() -> Self {
        Self {
            current: Mutex::new(ObsStatus::default()),
        }
    }
}

/// 探测 OBS 进程是否在运行：Windows 匹配 `obs64.exe`/`obs.exe` 进程名，
/// 其他平台交给平台层的通用 OBS 探测。
fn process_running() -> bool {
    #[cfg(windows)]
    {
        crate::infrastructure::platform::named_process_running("obs64.exe", "obs")
            || crate::infrastructure::platform::named_process_running("obs.exe", "obs")
    }
    #[cfg(not(windows))]
    {
        crate::infrastructure::platform::obs_process_running()
    }
}

/// 校验用户输入的 OBS WebSocket 地址：仅接受带主机的 `ws://`/`wss://`，
/// 去掉尾部斜杠后统一持久化格式。
fn valid_url(value: &str) -> CommandResult<String> {
    let url = url::Url::parse(value.trim())
        .map_err(|_| CommandError::new("INVALID_OBS_WEBSOCKET_URL", "OBS WebSocket 地址无效"))?;
    if !matches!(url.scheme(), "ws" | "wss") || url.host_str().is_none() {
        return Err(CommandError::new(
            "INVALID_OBS_WEBSOCKET_URL",
            "OBS WebSocket 地址必须为 ws:// 或 wss:// 地址",
        ));
    }
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

/// OBS WebSocket 连接的类型别名（`wss` 时含 TLS 流，否则为裸 TCP）。
type ObsSocket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// 读取下一条文本帧并解析为 JSON；跳过其他帧型，收到关闭帧或流结束时报错。
async fn text(socket: &mut ObsSocket) -> CommandResult<Value> {
    while let Some(message) = socket.next().await {
        match message.map_err(|e| CommandError::new("OBS_WEBSOCKET_ERROR", e.to_string()))? {
            Message::Text(value) => return serde_json::from_str(&value).map_err(Into::into),
            Message::Close(_) => {
                return Err(CommandError::new(
                    "OBS_WEBSOCKET_CLOSED",
                    "OBS WebSocket 连接已关闭",
                ));
            }
            _ => {}
        }
    }
    Err(CommandError::new(
        "OBS_WEBSOCKET_CLOSED",
        "OBS WebSocket 连接已关闭",
    ))
}

/// 按 obs-websocket v5 质询算法生成鉴权串：
/// 先对“密码+salt”做 SHA256，再对“结果+challenge”做一次 SHA256，均 Base64 编码。
fn auth(password: &str, salt: &str, challenge: &str) -> String {
    let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    STANDARD.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

/// 建立连接并完成 Hello/Identify 握手：OBS 要求鉴权而本地未存密码时，
/// 返回 `OBS_PASSWORD_REQUIRED` 引导用户先保存密码。
async fn connect(url: &str, password: Option<&str>) -> CommandResult<ObsSocket> {
    // OBS WebSocket 的 Hello/Identify 握手必须在任何 RPC 请求前完成。
    let url = valid_url(url)?;
    let (mut socket, _) = connect_async(url).await.map_err(|e| {
        CommandError::new(
            "OBS_CONNECTION_FAILED",
            format!("无法连接 OBS WebSocket：{e}"),
        )
    })?;
    let hello = text(&mut socket).await?;
    if hello.get("op").and_then(Value::as_i64) != Some(0) {
        return Err(CommandError::new(
            "OBS_PROTOCOL_ERROR",
            "OBS 未返回 WebSocket v5 Hello 消息",
        ));
    }
    let authentication = hello.pointer("/d/authentication");
    let identified = if let Some(authentication) = authentication {
        let password = password.ok_or_else(|| {
            CommandError::new(
                "OBS_PASSWORD_REQUIRED",
                "OBS 要求 WebSocket 密码，请在 Tosu 页面保存密码",
            )
        })?;
        let salt = authentication
            .get("salt")
            .and_then(Value::as_str)
            .ok_or_else(|| CommandError::new("OBS_PROTOCOL_ERROR", "OBS 鉴权 salt 缺失"))?;
        let challenge = authentication
            .get("challenge")
            .and_then(Value::as_str)
            .ok_or_else(|| CommandError::new("OBS_PROTOCOL_ERROR", "OBS 鉴权 challenge 缺失"))?;
        Some(auth(password, salt, challenge))
    } else {
        None
    };
    socket
        .send(Message::Text(
            json!({"op": 1, "d": {"rpcVersion": 1, "authentication": identified}})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|e| CommandError::new("OBS_WEBSOCKET_ERROR", e.to_string()))?;
    let response = text(&mut socket).await?;
    if response.get("op").and_then(Value::as_i64) != Some(2) {
        return Err(CommandError::new(
            "OBS_AUTH_FAILED",
            "OBS WebSocket 鉴权失败",
        ));
    }
    Ok(socket)
}

/// 发送一次 RPC 请求并等待 `requestId` 匹配的响应；期间到达的事件消息被忽略，
/// 响应内 `requestStatus.result` 为假时转为带服务端注释的错误。
async fn request(
    socket: &mut ObsSocket,
    request_type: &str,
    request_data: Value,
) -> CommandResult<Value> {
    // 每个请求使用 UUID 关联响应，避免事件消息与并发 RPC 响应相互混淆。
    let request_id = Uuid::new_v4().to_string();
    socket.send(Message::Text(json!({"op":6,"d":{"requestType":request_type,"requestId":request_id,"requestData":request_data}}).to_string().into())).await.map_err(|e| CommandError::new("OBS_WEBSOCKET_ERROR", e.to_string()))?;
    loop {
        let response = text(socket).await?;
        if response.get("op").and_then(Value::as_i64) != Some(7)
            || response.pointer("/d/requestId").and_then(Value::as_str) != Some(&request_id)
        {
            continue;
        }
        let status = response
            .pointer("/d/requestStatus/result")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !status {
            return Err(CommandError::new(
                "OBS_REQUEST_FAILED",
                response
                    .pointer("/d/requestStatus/comment")
                    .and_then(Value::as_str)
                    .unwrap_or("OBS 请求失败"),
            ));
        }
        return Ok(response
            .pointer("/d/responseData")
            .cloned()
            .unwrap_or(Value::Null));
    }
}

/// 从持久化设置取地址、从凭据管理器取密码后建立 OBS 连接。
async fn open(state: &AppState) -> CommandResult<ObsSocket> {
    let settings = state.store.settings_snapshot()?;
    let password = state.credentials.get_obs_websocket_password()?;
    connect(&settings.obs_websocket_url, password.as_deref()).await
}

/// 组装状态快照：附带进程探测与密码配置存在性，`connected` 由调用方传入。
fn status(state: &AppState, connected: bool, error: Option<String>) -> CommandResult<ObsStatus> {
    let settings = state.store.settings_snapshot()?;
    Ok(ObsStatus {
        running: process_running(),
        websocket_url: settings.obs_websocket_url,
        connected,
        password_configured: state.credentials.get_obs_websocket_password()?.is_some(),
        selected_scene: settings.obs_selected_scene,
        last_error: error,
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_obs_status(state: State<'_, AppState>) -> CommandResult<ObsStatus> {
    let result = match open(&state).await {
        Ok(_) => status(&state, true, None),
        Err(error) => status(&state, false, Some(error.message)),
    }?;
    if let Ok(mut current) = state.obs.current.lock() {
        *current = result.clone();
    }
    Ok(result)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：校验并持久化用户配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn save_obs_connection(
    websocket_url: String,
    password: Option<String>,
    selected_scene: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<ObsStatus> {
    let websocket_url = valid_url(&websocket_url)?;
    if let Some(password) = password {
        state
            .credentials
            .set_obs_websocket_password(password.trim())?;
    }
    state.store.update(|persisted| {
        persisted.settings.obs_websocket_url = websocket_url;
        persisted.settings.obs_selected_scene =
            selected_scene.filter(|scene| !scene.trim().is_empty());
    })?;
    status(&state, false, None)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_obs_scenes(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    let mut socket = open(&state).await?;
    let data = request(&mut socket, "GetSceneList", json!({})).await?;
    Ok(data
        .get("scenes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|scene| {
            !scene
                .get("isGroup")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|scene| {
            scene
                .get("sceneName")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect())
}

/// 收集场景内全部浏览器源名称：编组（group）作为独立列表继续下钻，
/// 以“是否编组+名称”去重避免环。
async fn collect_sources(
    socket: &mut ObsSocket,
    scene: &str,
    sources: &mut HashSet<String>,
) -> CommandResult<()> {
    let mut pending = vec![(scene.to_owned(), false)];
    let mut visited = HashSet::new();
    while let Some((name, group)) = pending.pop() {
        if !visited.insert(format!("{group}:{name}")) {
            continue;
        }
        let method = if group {
            "GetGroupSceneItemList"
        } else {
            "GetSceneItemList"
        };
        let data = request(socket, method, json!({"sceneName": name})).await?;
        for item in data
            .get("sceneItems")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(source) = item.get("sourceName").and_then(Value::as_str) else {
                continue;
            };
            if item
                .get("inputKind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.contains("browser"))
            {
                sources.insert(source.to_owned());
            }
            if item
                .get("isGroup")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                pending.push((source.to_owned(), true));
            }
        }
    }
    Ok(())
}

/// 刷新所选场景内全部浏览器源（模拟点击属性面板的 refreshnocache 按钮）；
/// 未选择场景时返回“已跳过”而不是报错。
pub async fn refresh_selected(state: &AppState) -> CommandResult<ObsRefreshResult> {
    let settings = state.store.settings_snapshot()?;
    let Some(scene) = settings.obs_selected_scene else {
        return Ok(ObsRefreshResult {
            refreshed_sources: vec![],
            skipped: true,
            message: "尚未选择 OBS 场景，已跳过浏览器源刷新".into(),
        });
    };
    let mut socket = open(state).await?;
    let mut sources = HashSet::new();
    collect_sources(&mut socket, &scene, &mut sources).await?;
    let mut refreshed_sources = Vec::new();
    for source in sources {
        request(
            &mut socket,
            "PressInputPropertiesButton",
            json!({"inputName": source, "propertyName": "refreshnocache"}),
        )
        .await?;
        refreshed_sources.push(source);
    }
    refreshed_sources.sort();
    Ok(ObsRefreshResult {
        message: if refreshed_sources.is_empty() {
            "所选场景中没有浏览器源".into()
        } else {
            format!("已刷新 {} 个浏览器源", refreshed_sources.len())
        },
        refreshed_sources,
        skipped: false,
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：刷新外部状态。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn refresh_selected_obs_scene(
    state: State<'_, AppState>,
) -> CommandResult<ObsRefreshResult> {
    refresh_selected(&state).await
}

/// 启动后台监控任务：每秒轮询 OBS 进程状态，变化时向前端广播
/// `obs-status-changed`，并按设置在 OBS 启动时联动拉起 tosu。
pub fn start_obs_monitor(app: AppHandle) {
    // 监控器只在后台轮询连接状态；具体场景刷新仍由显式命令触发。
    tauri::async_runtime::spawn(async move {
        let mut previous = false;
        loop {
            // 仅音乐模式（music_only）下暂停进程轮询，避免无谓探测。
            if app
                .state::<AppState>()
                .music_only
                .load(std::sync::atomic::Ordering::Relaxed)
                && !previous
            {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            // 进程枚举可能阻塞（尤其 Windows），放到阻塞线程池执行。
            let running = tokio::task::spawn_blocking(process_running)
                .await
                .unwrap_or(false);
            if running != previous {
                previous = running;
                let _ = app.emit(
                    "obs-status-changed",
                    ObsStatus {
                        running,
                        ..ObsStatus::default()
                    },
                );
                if running {
                    let state = app.state::<AppState>();
                    if let Ok(mut current) = state.obs.current.lock() {
                        current.running = true;
                    }
                    if state
                        .store
                        .settings_snapshot()
                        .map(|saved| saved.launch_tosu_on_obs_detect)
                        .unwrap_or(false)
                    {
                        let _ = start_managed_tosu(&state, app.clone());
                    }
                } else if let Ok(mut current) = app.state::<AppState>().obs.current.lock() {
                    current.running = false;
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 相同密码/salt/challenge 得到相同鉴权串，密码不同则不同。
    #[test]
    fn auth_is_deterministic() {
        assert_eq!(
            auth("password", "salt", "challenge"),
            auth("password", "salt", "challenge")
        );
        assert_ne!(
            auth("password", "salt", "challenge"),
            auth("wrong", "salt", "challenge")
        );
    }
}
