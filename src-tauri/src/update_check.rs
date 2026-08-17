use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    models::AppSettings,
    state::AppState,
};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/osuplusplus/OPP/releases/latest";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResult {
    current_version: String,
    latest_version: String,
    latest_tag: String,
    is_latest: bool,
    release_name: Option<String>,
    release_url: String,
    published_at: Option<String>,
    release_notes: Option<String>,
}

fn parse_release_version(value: &str) -> Result<Version, semver::Error> {
    Version::parse(value.trim().trim_start_matches(['v', 'V']))
}

fn build_update_result(
    current_version: &str,
    release: GitHubRelease,
) -> CommandResult<UpdateCheckResult> {
    let latest = parse_release_version(&release.tag_name).map_err(|_| {
        CommandError::new(
            "INVALID_RELEASE_TAG",
            format!("GitHub Release 标签不是有效版本号：{}", release.tag_name),
        )
    })?;
    let current = parse_release_version(current_version).map_err(|_| {
        CommandError::new(
            "INVALID_APP_VERSION",
            format!("当前应用版本号无效：{current_version}"),
        )
    })?;

    Ok(UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version: latest.to_string(),
        latest_tag: release.tag_name,
        is_latest: current >= latest,
        release_name: release.name,
        release_url: release.html_url,
        published_at: release.published_at,
        release_notes: release.body.filter(|body| !body.trim().is_empty()),
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn check_for_updates(app: tauri::AppHandle) -> CommandResult<UpdateCheckResult> {
    let current_version = app.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(format!("OPP/{current_version}"))
        .build()
        .map_err(|error| CommandError::network(format!("无法创建版本检查请求：{error}")))?;

    let response = client
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| CommandError::network(format!("无法连接 GitHub：{error}")))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(CommandError::network(format!(
            "GitHub 版本检查失败（HTTP {status}）"
        )));
    }

    let release = response.json::<GitHubRelease>().await.map_err(|error| {
        CommandError::new(
            "INVALID_RELEASE_DATA",
            format!("无法读取 GitHub Release：{error}"),
        )
    })?;
    build_update_result(&current_version, release)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：记录用户忽略的版本。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn ignore_update_version(
    version: String,
    state: State<'_, AppState>,
) -> CommandResult<AppSettings> {
    let normalized = parse_release_version(&version)
        .map_err(|_| CommandError::new("INVALID_UPDATE_VERSION", "要忽略的更新版本号无效"))?;
    state.store.update(|persisted| {
        persisted.settings.ignored_update_version = Some(normalized.to_string());
        persisted.settings.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::{GitHubRelease, build_update_result, parse_release_version};

    #[test]
    fn parses_release_tags_with_optional_v_prefix() {
        assert_eq!(
            parse_release_version("v1.2.3").unwrap().to_string(),
            "1.2.3"
        );
        assert_eq!(
            parse_release_version("V2.0.0").unwrap().to_string(),
            "2.0.0"
        );
        assert_eq!(parse_release_version("1.4.0").unwrap().to_string(), "1.4.0");
    }

    #[test]
    fn follows_semantic_version_ordering() {
        let current = parse_release_version("0.10.0").unwrap();
        let older = parse_release_version("v0.9.9").unwrap();
        assert!(current > older);
    }

    #[test]
    fn maps_release_notes_and_version_status() {
        let result = build_update_result(
            "1.0.0",
            GitHubRelease {
                tag_name: "v1.1.0".to_string(),
                name: Some("OPP 1.1.0".to_string()),
                html_url: "https://github.com/osuplusplus/OPP/releases/tag/v1.1.0".to_string(),
                published_at: Some("2026-08-13T00:00:00Z".to_string()),
                body: Some("- 新功能".to_string()),
            },
        )
        .expect("build update result");

        assert!(!result.is_latest);
        assert_eq!(result.latest_version, "1.1.0");
        assert_eq!(result.release_notes.as_deref(), Some("- 新功能"));
    }

    #[test]
    fn rejects_invalid_release_tags() {
        let error = build_update_result(
            "1.0.0",
            GitHubRelease {
                tag_name: "latest".to_string(),
                name: None,
                html_url: "https://example.test".to_string(),
                published_at: None,
                body: None,
            },
        )
        .expect_err("invalid tag should fail");

        assert_eq!(error.code, "INVALID_RELEASE_TAG");
    }
}
