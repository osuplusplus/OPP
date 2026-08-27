use crate::error::{CommandError, CommandResult};
use reqwest::StatusCode;
use serde_json::Value;

const BASE: &str = "https://inex.osekai.net/api";

async fn get(path: &str) -> CommandResult<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("OPP Osekai medal client")
        .build()
        .map_err(|e| CommandError::network(e.to_string()))?;
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

#[tauri::command]
pub async fn get_osekai_medals() -> CommandResult<Value> {
    get("/medals/get_all").await
}

#[tauri::command]
pub async fn get_osekai_medal_detail(medal_id: u64) -> CommandResult<Value> {
    get(&format!("/medals/{medal_id}/extra")).await
}

#[tauri::command]
pub async fn get_osekai_medal_beatmaps(medal_id: u64) -> CommandResult<Value> {
    get(&format!("/medals/{medal_id}/beatmaps")).await
}
