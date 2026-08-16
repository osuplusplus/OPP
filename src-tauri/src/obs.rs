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
    state::AppState,
    tosu::start_managed_tosu,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObsStatus {
    pub running: bool,
    pub websocket_url: String,
    pub connected: bool,
    pub password_configured: bool,
    pub selected_scene: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsRefreshResult {
    pub refreshed_sources: Vec<String>,
    pub skipped: bool,
    pub message: String,
}

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

fn process_running() -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["/FI", "IMAGENAME eq obs64.exe", "/NH"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_ascii_lowercase()
                    .contains("obs64.exe")
            })
            .unwrap_or(false)
            || Command::new("tasklist")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["/FI", "IMAGENAME eq obs.exe", "/NH"])
                .output()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .to_ascii_lowercase()
                        .contains("obs.exe")
                })
                .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        crate::platform::obs_process_running()
    }
}

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

type ObsSocket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

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

fn auth(password: &str, salt: &str, challenge: &str) -> String {
    let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    STANDARD.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

async fn connect(url: &str, password: Option<&str>) -> CommandResult<ObsSocket> {
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

async fn request(
    socket: &mut ObsSocket,
    request_type: &str,
    request_data: Value,
) -> CommandResult<Value> {
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

async fn open(state: &AppState) -> CommandResult<ObsSocket> {
    let settings = state.store.snapshot()?.settings;
    let password = state.credentials.get_obs_websocket_password()?;
    connect(&settings.obs_websocket_url, password.as_deref()).await
}

fn status(state: &AppState, connected: bool, error: Option<String>) -> CommandResult<ObsStatus> {
    let settings = state.store.snapshot()?.settings;
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

pub async fn refresh_selected(state: &AppState) -> CommandResult<ObsRefreshResult> {
    let settings = state.store.snapshot()?.settings;
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
pub async fn refresh_selected_obs_scene(
    state: State<'_, AppState>,
) -> CommandResult<ObsRefreshResult> {
    refresh_selected(&state).await
}

pub fn start_obs_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut previous = false;
        loop {
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
                        .snapshot()
                        .map(|saved| saved.settings.launch_tosu_on_obs_detect)
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
