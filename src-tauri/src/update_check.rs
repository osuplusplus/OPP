use std::{
    env,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CACHE_CONTROL};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::{fs, io::AsyncWriteExt};

use crate::{
    app::models::AppSettings,
    app::state::AppState,
    error::{CommandError, CommandResult},
    portable_update,
};

const LATEST_MANIFEST_URL: &str =
    "https://github.com/osuplusplus/OPP/releases/latest/download/latest.json";
const MAX_UPDATE_BYTES: u64 = 1024 * 1024 * 1024;
const UPDATE_PROGRESS_EVENT: &str = "update-progress";

static UPDATE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Deserialize)]
struct UpdateManifest {
    version: String,
    url: String,
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
    can_auto_update: bool,
    download_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgress {
    phase: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: String,
}

struct UpdateGuard;

impl UpdateGuard {
    fn acquire() -> CommandResult<Self> {
        UPDATE_IN_PROGRESS
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| CommandError::new("UPDATE_ALREADY_RUNNING", "已有更新任务正在运行"))?;
        Ok(Self)
    }
}

impl Drop for UpdateGuard {
    fn drop(&mut self) {
        UPDATE_IN_PROGRESS.store(false, Ordering::Release);
    }
}

fn parse_release_version(value: &str) -> Result<Version, semver::Error> {
    Version::parse(value.trim().trim_start_matches(['v', 'V']))
}

fn supports_auto_update() -> bool {
    cfg!(all(target_os = "windows", target_arch = "x86_64"))
}

fn build_update_result(
    current_version: &str,
    manifest: UpdateManifest,
    can_auto_update: bool,
) -> CommandResult<UpdateCheckResult> {
    let latest = parse_release_version(&manifest.version).map_err(|_| {
        CommandError::new(
            "INVALID_RELEASE_TAG",
            format!("更新清单版本号无效：{}", manifest.version),
        )
    })?;
    let current = parse_release_version(current_version).map_err(|_| {
        CommandError::new(
            "INVALID_APP_VERSION",
            format!("当前应用版本号无效：{current_version}"),
        )
    })?;
    validate_download_url(&manifest.url)?;

    let latest_version = latest.to_string();
    Ok(UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_tag: format!("v{latest_version}"),
        is_latest: current >= latest,
        release_name: Some(format!("OPP v{latest_version}")),
        release_url: format!("https://github.com/osuplusplus/OPP/releases/tag/v{latest_version}"),
        published_at: None,
        release_notes: None,
        can_auto_update,
        download_size: None,
        latest_version,
    })
}

fn validate_download_url(value: &str) -> CommandResult<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|error| CommandError::new("INVALID_UPDATE_URL", error.to_string()))?;
    if url.scheme() != "https" {
        return Err(CommandError::new(
            "INVALID_UPDATE_URL",
            "更新包下载地址必须使用 HTTPS",
        ));
    }
    Ok(())
}

fn http_client(current_version: &str) -> CommandResult<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // 只限制单次读取停顿，避免网络较慢时整个大文件被总时限中断。
        .read_timeout(Duration::from_secs(60))
        .user_agent(format!("OPP/{current_version}"))
        .build()
        .map_err(|error| CommandError::network(format!("无法创建更新请求：{error}")))
}

async fn fetch_manifest(client: &reqwest::Client) -> CommandResult<UpdateManifest> {
    let response = client
        .get(LATEST_MANIFEST_URL)
        .header(ACCEPT, "application/json")
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .await
        .map_err(|error| CommandError::network(format!("无法连接 GitHub：{error}")))?;
    if !response.status().is_success() {
        return Err(CommandError::network(format!(
            "GitHub 更新清单读取失败（HTTP {}）",
            response.status()
        )));
    }
    response.json::<UpdateManifest>().await.map_err(|error| {
        CommandError::new("INVALID_RELEASE_DATA", format!("无法读取更新清单：{error}"))
    })
}

#[tauri::command]
/// 读取固定 Release 清单并比较当前版本；启动检查失败不会阻塞其他功能。
pub async fn check_for_updates(app: AppHandle) -> CommandResult<UpdateCheckResult> {
    let current_version = app.package_info().version.to_string();
    let client = http_client(&current_version)?;
    let manifest = fetch_manifest(&client).await?;
    build_update_result(&current_version, manifest, supports_auto_update())
}

#[tauri::command]
/// 下载最新便携 EXE，随后启动临时助手完成原路径替换和重启。
pub async fn download_and_install_update(
    app: AppHandle,
    expected_version: String,
) -> CommandResult<()> {
    let _guard = UpdateGuard::acquire()?;
    if !supports_auto_update() {
        return Err(CommandError::new(
            "AUTO_UPDATE_UNSUPPORTED",
            "当前平台暂不支持便携 EXE 应用内更新",
        ));
    }

    let current_version = app.package_info().version.to_string();
    let client = http_client(&current_version)?;
    // 安装前重新读取清单，避免检查后 latest 指向了另一个版本。
    let manifest = fetch_manifest(&client).await?;
    let latest = parse_release_version(&manifest.version)
        .map_err(|_| CommandError::new("INVALID_RELEASE_TAG", "更新清单版本号无效"))?;
    let expected = parse_release_version(&expected_version)
        .map_err(|_| CommandError::new("INVALID_UPDATE_VERSION", "目标更新版本号无效"))?;
    let current = parse_release_version(&current_version)
        .map_err(|_| CommandError::new("INVALID_APP_VERSION", "当前应用版本号无效"))?;
    if latest != expected || latest <= current {
        return Err(CommandError::new(
            "UPDATE_VERSION_CHANGED",
            "最新版本已经变化，请重新检查更新",
        ));
    }
    validate_download_url(&manifest.url)?;

    let current_exe = env::current_exe().map_err(|error| {
        CommandError::new("UPDATE_PATH_ERROR", format!("无法定位当前程序：{error}"))
    })?;
    let paths = portable_update::paths_for(&current_exe, &latest.to_string()).map_err(|error| {
        CommandError::new("UPDATE_PATH_ERROR", format!("无法准备更新路径：{error}"))
    })?;
    let partial = paths.staged.with_extension("exe.part");
    remove_file_if_exists(&partial).await?;
    remove_file_if_exists(&paths.staged).await?;

    let downloaded = match download_asset(&app, &client, &manifest.url, &partial).await {
        Ok(downloaded) => downloaded,
        Err(error) => {
            let _ = fs::remove_file(&partial).await;
            return Err(error);
        }
    };
    emit_progress(
        &app,
        "preparing",
        downloaded,
        downloaded,
        "正在准备替换程序文件",
    );

    if let Err(error) = fs::rename(&partial, &paths.staged).await {
        let _ = fs::remove_file(&partial).await;
        return Err(CommandError::new(
            "UPDATE_STAGE_FAILED",
            format!("无法保存更新包：{error}"),
        ));
    }
    if let Err(error) = portable_update::launch_helper(&paths) {
        let _ = fs::remove_file(&paths.staged).await;
        return Err(CommandError::new(
            "UPDATE_HELPER_FAILED",
            format!("无法启动更新助手：{error}"),
        ));
    }
    emit_progress(
        &app,
        "restarting",
        downloaded,
        downloaded,
        "更新已就绪，正在重启 OPP",
    );
    app.exit(0);
    Ok(())
}

async fn download_asset(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> CommandResult<u64> {
    let response = client
        .get(url)
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .await
        .map_err(|error| CommandError::network(format!("更新包下载失败：{error}")))?;
    if !response.status().is_success() {
        return Err(CommandError::network(format!(
            "更新包下载失败（HTTP {}）",
            response.status()
        )));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    if total_bytes > MAX_UPDATE_BYTES {
        return Err(CommandError::new(
            "UPDATE_SIZE_LIMIT",
            "更新包大小超出允许范围",
        ));
    }
    emit_progress(app, "downloading", 0, total_bytes, "正在下载新版本");

    let mut file = fs::File::create(destination).await.map_err(|error| {
        CommandError::new(
            "UPDATE_DIRECTORY_NOT_WRITABLE",
            format!("当前程序目录不可写：{error}"),
        )
    })?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    let mut last_progress = Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| CommandError::network(format!("更新包下载中断：{error}")))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_UPDATE_BYTES || (total_bytes > 0 && downloaded > total_bytes) {
            return Err(CommandError::new(
                "UPDATE_SIZE_LIMIT",
                "更新包实际大小超出允许范围",
            ));
        }
        file.write_all(&chunk).await.map_err(|error| {
            CommandError::new("UPDATE_WRITE_FAILED", format!("更新包写入失败：{error}"))
        })?;
        // 限制前端事件频率，避免高速下载时每个网络分片都触发一次 React 更新。
        if (total_bytes > 0 && downloaded == total_bytes)
            || last_progress.elapsed() >= Duration::from_millis(100)
        {
            emit_progress(
                app,
                "downloading",
                downloaded,
                total_bytes,
                "正在下载新版本",
            );
            last_progress = Instant::now();
        }
    }
    file.flush().await.map_err(|error| {
        CommandError::new("UPDATE_WRITE_FAILED", format!("更新包写入失败：{error}"))
    })?;
    file.sync_all().await.map_err(|error| {
        CommandError::new("UPDATE_WRITE_FAILED", format!("更新包落盘失败：{error}"))
    })?;
    if total_bytes > 0 && downloaded != total_bytes {
        return Err(CommandError::new(
            "UPDATE_DOWNLOAD_INCOMPLETE",
            "更新包下载不完整",
        ));
    }
    // 服务端未返回 Content-Length 时也补发一次最终进度。
    emit_progress(
        app,
        "downloading",
        downloaded,
        if total_bytes > 0 {
            total_bytes
        } else {
            downloaded
        },
        "正在下载新版本",
    );
    Ok(downloaded)
}

fn emit_progress(
    app: &AppHandle,
    phase: &str,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: &str,
) {
    let _ = app.emit(
        UPDATE_PROGRESS_EVENT,
        UpdateProgress {
            phase: phase.into(),
            downloaded_bytes,
            total_bytes,
            message: message.into(),
        },
    );
}

async fn remove_file_if_exists(path: &Path) -> CommandResult<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CommandError::new(
            "UPDATE_FILE_CLEANUP_FAILED",
            format!("无法清理旧更新文件：{error}"),
        )),
    }
}

#[tauri::command]
/// 记录用户忽略的版本；忽略只影响启动自动提示，不影响手动检查。
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
    use super::{
        UpdateManifest, build_update_result, parse_release_version, validate_download_url,
    };

    fn manifest(version: &str) -> UpdateManifest {
        UpdateManifest {
            version: version.into(),
            url: format!("https://github.com/osuplusplus/OPP/releases/download/v{version}/OPP.exe"),
        }
    }

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
    }

    #[test]
    fn follows_semantic_version_ordering() {
        let current = parse_release_version("0.10.0").unwrap();
        let older = parse_release_version("v0.9.9").unwrap();
        assert!(current > older);
    }

    #[test]
    fn maps_manifest_to_an_installable_update() {
        let result =
            build_update_result("1.0.0", manifest("1.1.0"), true).expect("build update result");

        assert!(!result.is_latest);
        assert!(result.can_auto_update);
        assert_eq!(result.latest_version, "1.1.0");
        assert_eq!(result.download_size, None);
        assert_eq!(result.release_notes, None);
    }

    #[test]
    fn rejects_non_https_download_urls() {
        let error = validate_download_url("http://example.test/OPP.exe")
            .expect_err("non-HTTPS URL must fail");

        assert_eq!(error.code, "INVALID_UPDATE_URL");
    }

    #[test]
    fn rejects_invalid_manifest_versions() {
        let error = build_update_result("1.0.0", manifest("latest"), true)
            .expect_err("invalid version should fail");

        assert_eq!(error.code, "INVALID_RELEASE_TAG");
    }
}
