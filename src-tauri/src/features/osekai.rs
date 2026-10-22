//! Osekai（inex.osekai.net）奖章 API 的薄封装：奖章列表、详情与关联谱面查询。
use crate::error::{CommandError, CommandResult};
use crate::infrastructure::http::{HttpClientSpec, http_client};
use reqwest::StatusCode;
use serde_json::Value;

const BASE: &str = "https://inex.osekai.net/api";

/// 统一请求入口：429 限流映射为专用错误码，其余失败分别按网络/服务端错误返回。
async fn get(path: &str) -> CommandResult<Value> {
    let client = http_client(HttpClientSpec {
        // Medal detail endpoints are optional; keep the UI responsive when
        // Osekai is unreachable instead of leaving every dialog spinning.
        timeout: Some(std::time::Duration::from_secs(5)),
        ua_note: " Osekai medal client",
        connect_timeout: None,
    })?;
    let response = client
        .get(format!("{BASE}{path}"))
        .send()
        .await
        .map_err(|e| CommandError::network(format!("Osekai 请求失败：{e}")))?;
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CommandError::new(
            "OSEKAI_RATE_LIMITED",
            "Osekai 请求过于频繁",
        ));
    }
    if !response.status().is_success() {
        return Err(CommandError::new(
            "OSEKAI_SERVER_ERROR",
            format!("Osekai 返回 HTTP {}", response.status()),
        ));
    }
    response
        .json()
        .await
        .map_err(|e| CommandError::network(format!("Osekai 响应解析失败：{e}")))
}

/// 供前端调用的 Tauri 命令：拉取全部奖章列表。
#[tauri::command]
pub async fn get_osekai_medals() -> CommandResult<Value> {
    get("/medals/get_all").await
}

/// 供前端调用的 Tauri 命令：读取单个奖章的补充信息。
#[tauri::command]
pub async fn get_osekai_medal_detail(medal_id: u64) -> CommandResult<Value> {
    get(&format!("/medals/{medal_id}/extra")).await
}

/// 供前端调用的 Tauri 命令：读取奖章关联的谱面推荐。
#[tauri::command]
pub async fn get_osekai_medal_beatmaps(medal_id: u64) -> CommandResult<Value> {
    get(&format!("/medals/{medal_id}/beatmaps")).await
}
