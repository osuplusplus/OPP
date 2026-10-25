//! tosu 游戏内渲染集成（全屏覆盖方案）。
//!
//! tosu 内置 ingame overlay（asdf-overlay，D3D/OpenGL 钩子直接渲染进游戏
//! 画面，独占全屏可见）。本模块把 OPP 的 HUD 页安装进 tosu 的 static 目录
//! 并注册到 `__ingame__` profile，同时在本地起一个小型 HTTP 服务为该页
//! 提供 OPP 侧状态（MP 补缺/下载进度/tosu 快照）。

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tokio::net::TcpListener;

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

use super::irc::OverlayRuntime;

/// 本地状态服务端口（HUD 页轮询 http://127.0.0.1:<port>/api/status）。
pub const STATUS_PORT: u16 = 24099;
/// tosu `static/` 下 OPP HUD 的目录名，同时作为 profile 中的 folderName 标识。
const HUD_FOLDER: &str = "opp-hud";
/// ingame HUD 的注册尺寸，与窗口模式的 `HUD_SIZE`（340×190）数值一致。
const HUD_WIDTH: i64 = 340;
const HUD_HEIGHT: i64 = 190;

/// 安装命令返回给前端的结果：安装状态、tosu 目录与用户提示。
#[derive(Debug, Serialize)]
pub struct IngameInstallInfo {
    pub installed: bool,
    pub tosu_dir: String,
    pub message: String,
}

/// 从设置中的 tosu 可执行文件路径推导其所在目录；未配置或目录缺失时报错。
fn tosu_dir(state: &AppState) -> CommandResult<PathBuf> {
    let raw = state
        .store
        .settings_snapshot()
        .ok()
        .and_then(|settings| settings.tosu_executable_path)
        .ok_or_else(|| {
            CommandError::new("TOSU_NOT_CONFIGURED", "请先在工具 > tosu 中选择 tosu.exe")
        })?;
    let dir = PathBuf::from(raw)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| CommandError::new("TOSU_NOT_CONFIGURED", "tosu 路径无效"))?;
    if !dir.is_dir() {
        return Err(CommandError::new(
            "TOSU_NOT_CONFIGURED",
            "tosu 目录不存在，请重新选择 tosu 可执行文件",
        ));
    }
    Ok(dir)
}

/// 以 dotenv 语义更新 tosu.env 的单个键，保留其余行。
pub fn patch_tosu_env(dir: &Path, key: &str, value: &str) -> CommandResult<()> {
    let path = dir.join("tosu.env");
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let prefix = format!("{key}=");
    let mut found = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            if line.trim_start().starts_with(&prefix) {
                found = true;
                format!("{key}={value}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !found {
        lines.push(format!("{key}={value}"));
    }
    std::fs::write(&path, lines.join("\n") + "\n")?;
    Ok(())
}

/// tosu 尚无 ingame 配置文件时使用的骨架（单个空 `default` profile）。
fn default_ingame_settings() -> Value {
    json!({
        "obs_profile": "default",
        "ingame_profile": "default",
        "profiles": [
            { "id": "default", "name": "default", "overlays": [] }
        ]
    })
}

/// 在 `__ingame__.values.json` 中合并/更新 OPP 的 overlay 条目（不动用户其他配置）。
fn upsert_profile_entry(settings: &mut Value) {
    if !settings.get("profiles").is_some_and(Value::is_array) {
        settings["profiles"] = json!([{ "id": "default", "name": "default", "overlays": [] }]);
    }
    let selected = settings
        .get("ingame_profile")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_owned();
    let Some(profiles) = settings["profiles"].as_array_mut() else {
        return;
    };
    if profiles.is_empty() {
        profiles.push(json!({ "id": "default", "name": "default", "overlays": [] }));
    }
    let index = profiles
        .iter()
        .position(|profile| profile.get("name").and_then(Value::as_str) == Some(selected.as_str()))
        .unwrap_or(0);
    let profile = &mut profiles[index];
    if !profile.get("overlays").is_some_and(Value::is_array) {
        profile["overlays"] = json!([]);
    }
    let overlays = profile["overlays"].as_array_mut().unwrap();
    overlays
        .retain(|overlay| overlay.get("folderName").and_then(Value::as_str) != Some(HUD_FOLDER));
    let next_id = overlays
        .iter()
        .map(|item| item.get("id").and_then(Value::as_i64).unwrap_or(0))
        .max()
        .unwrap_or(0)
        + 1;
    overlays.push(json!({
        "_enabled": true,
        "_settings": false,
        "id": next_id,
        "folderName": HUD_FOLDER,
        "url": format!("tosu://server/{HUD_FOLDER}"),
        "width": HUD_WIDTH,
        "height": HUD_HEIGHT,
        "top": 16,
        "left": 16,
        "scale": 1,
        "z_index": 1
    }));
}

/// 移除 OPP 的 overlay 条目；所有 profile 都空时关闭 ingame 开关。
fn remove_profile_entry(settings: &mut Value) {
    let mut any_left = false;
    if let Some(profiles) = settings.get_mut("profiles").and_then(Value::as_array_mut) {
        for profile in profiles {
            if let Some(overlays) = profile.get_mut("overlays").and_then(Value::as_array_mut) {
                overlays.retain(|overlay| {
                    overlay.get("folderName").and_then(Value::as_str) != Some(HUD_FOLDER)
                });
                any_left = any_left || !overlays.is_empty();
            }
        }
    }
    // 临时标记：卸载流程读取后即移除，仅用于传递“所有 profile 已空”。
    if !any_left {
        settings["__opp_disable_flag__"] = json!(true);
    }
}

/// 安装/更新 OPP HUD 到 tosu（static 页 + profile + env 开关）。需要重启 tosu 生效。
pub fn install_ingame_overlay(state: &AppState) -> CommandResult<IngameInstallInfo> {
    let dir = tosu_dir(state)?;
    let hud_dir = dir.join("static").join(HUD_FOLDER);
    std::fs::create_dir_all(&hud_dir)?;
    std::fs::write(hud_dir.join("index.html"), include_str!("ingame_hud.html"))?;
    // tosu 扫描 static 目录的 metadata.txt（gosumemory 兼容格式）识别计数器。
    std::fs::write(
        hud_dir.join("metadata.txt"),
        format!("name: OPP HUD\nauthor: OPP\nresolution: {HUD_WIDTH}x{HUD_HEIGHT}\n"),
    )?;

    let settings_path = dir.join("settings").join("__ingame__.values.json");
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut settings: Value = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(default_ingame_settings);
    upsert_profile_entry(&mut settings);
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    patch_tosu_env(&dir, "ENABLE_INGAME_OVERLAY", "true")?;
    Ok(IngameInstallInfo {
        installed: true,
        tosu_dir: dir.display().to_string(),
        message: "已安装。重启 tosu 后生效，游戏内按 tosu 覆盖热键（默认 Ctrl+Shift+Space）显隐。"
            .into(),
    })
}

/// 卸载 OPP HUD（保留用户其他 ingame 配置）。
pub fn uninstall_ingame_overlay(state: &AppState) -> CommandResult<()> {
    let dir = tosu_dir(state)?;
    let settings_path = dir.join("settings").join("__ingame__.values.json");
    if let Ok(content) = std::fs::read_to_string(&settings_path)
        && let Ok(mut settings) = serde_json::from_str::<Value>(&content)
    {
        remove_profile_entry(&mut settings);
        let disable = settings
            .get("__opp_disable_flag__")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(object) = settings.as_object_mut() {
            object.remove("__opp_disable_flag__");
        }
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        if disable {
            patch_tosu_env(&dir, "ENABLE_INGAME_OVERLAY", "false")?;
        }
    }
    let hud_dir = dir.join("static").join(HUD_FOLDER);
    let _ = std::fs::remove_dir_all(hud_dir);
    Ok(())
}

/// 检测 OPP HUD 是否已安装（static 目录存在即视为安装）。
pub fn ingame_installed(state: &AppState) -> bool {
    tosu_dir(state)
        .map(|dir| {
            dir.join("static")
                .join(HUD_FOLDER)
                .join("index.html")
                .is_file()
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 本地状态服务（HUD 页轮询数据源，CORS 开放给 tosu overlay 进程）
// ---------------------------------------------------------------------------

// 状态服务进程内只启动一次的守卫。
static SERVER_STARTED: OnceLock<()> = OnceLock::new();

/// 幂等启动本地状态 HTTP 服务；端口绑定失败仅记录警告，不阻断应用启动。
pub fn ensure_status_server(app: &AppHandle) {
    if SERVER_STARTED.set(()).is_err() {
        return;
    }
    let overlay = app.state::<AppState>().overlay.clone();
    let tosu = app.state::<AppState>().tosu.clone();
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", STATUS_PORT)).await {
            Ok(listener) => listener,
            Err(error) => {
                crate::log_warn!("overlay", "覆盖层状态服务启动失败：{error}");
                return;
            }
        };
        let router = axum::Router::new()
            .route("/", axum::routing::get(hud_page))
            .route("/api/status", axum::routing::get(status))
            .with_state((overlay, tosu));
        if let Err(error) = axum::serve(listener, router).await {
            crate::log_warn!("overlay", "覆盖层状态服务退出：{error}");
        }
    });
}

// 对所有来源开放且禁用缓存：tosu overlay 进程与浏览器调试页都要轮询此服务。
const CORS: [(&str, &str); 2] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Cache-Control", "no-store"),
];

/// 直接返回内嵌 HUD 页（与安装进 tosu static 目录的内容同源）。
async fn hud_page() -> impl axum::response::IntoResponse {
    (CORS, axum::response::Html(include_str!("ingame_hud.html")))
}

/// 聚合 OPP 侧运行时状态：MP 补缺、IRC、下载进度、最近聊天与 tosu 实时快照。
async fn status(
    axum::extract::State((overlay, tosu)): axum::extract::State<(
        Arc<OverlayRuntime>,
        Arc<crate::features::tosu::TosuRuntime>,
    )>,
) -> impl axum::response::IntoResponse {
    let mp = overlay
        .mp
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let irc = overlay
        .irc
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let download = overlay.download.lock().ok().and_then(|value| value.clone());
    // 最近聊天（含译文）供 tosu 游戏内渲染页的聊天条展示。
    let chat = overlay
        .chat
        .lock()
        .map(|log| log.recent(15))
        .unwrap_or_default();
    let tosu_live = crate::features::tosu::live_snapshot(&tosu);
    (
        CORS,
        axum::Json(json!({
            "mp": mp,
            "irc": irc,
            "download": download,
            "chat": chat,
            "tosu": tosu_live,
        })),
    )
}
