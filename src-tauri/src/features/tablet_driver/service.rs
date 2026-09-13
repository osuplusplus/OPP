use super::models::{OtdConfigSummary, OtdStatus};
use crate::{
    domain::AppSettings,
    error::{CommandError, CommandResult},
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct OtdRuntime {
    pub process: Mutex<Option<Child>>,
    pub last_error: Mutex<Option<String>>,
}
impl Default for OtdRuntime {
    fn default() -> Self {
        Self {
            process: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }
}

pub fn validate_executable(path: &Path) -> CommandResult<PathBuf> {
    let path = path.canonicalize().map_err(|_| {
        CommandError::new(
            "OTD_EXECUTABLE_NOT_FOUND",
            "未找到 OpenTabletDriver 可执行文件",
        )
    })?;
    if !path.is_file() {
        return Err(CommandError::new(
            "INVALID_OTD_EXECUTABLE",
            "请选择 OpenTabletDriver 可执行文件",
        ));
    }
    Ok(path)
}

fn configured_path(settings: &AppSettings) -> Option<PathBuf> {
    let configured = settings
        .otd_executable_path
        .as_ref()
        .and_then(|raw| validate_executable(Path::new(raw)).ok());
    if configured.is_some() {
        return configured;
    }
    #[cfg(not(windows))]
    {
        for name in ["OpenTabletDriver", "opentabletdriver", "otd"] {
            if let Some(path) = crate::infrastructure::platform::find_in_path(name) {
                return Some(path);
            }
        }
    }
    None
}

fn process_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("OpenTabletDriver")
        .to_owned()
}

fn process_running(name: &str) -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["/FI", &format!("IMAGENAME eq {name}.exe")])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_ascii_lowercase()
                    .contains(&format!("{name}.exe").to_ascii_lowercase())
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("pgrep")
            .args(["-f", name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

fn config_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(root).join("OpenTabletDriver"));
    }
    #[cfg(not(windows))]
    {
        if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
            roots.push(PathBuf::from(root).join("OpenTabletDriver"));
        } else if let Some(root) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(root).join(".config/OpenTabletDriver"));
        }
    }
    roots
}

fn find_config() -> Option<PathBuf> {
    for root in config_roots() {
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root)
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            if entry
                .path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("json"))
            {
                return Some(entry.path().to_path_buf());
            }
        }
    }
    None
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn summarize(path: &Path) -> Option<(OtdConfigSummary, Option<String>)> {
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let output_mode = string_field(&value, &["OutputMode", "output_mode", "OutputModeType"]);
    let area = value
        .get("Area")
        .or_else(|| value.get("area"))
        .map(|v| v.to_string());
    let filter_count = value
        .get("Filters")
        .or_else(|| value.get("filters"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let tablet_name = string_field(&value, &["Tablet", "tablet", "Name", "name"]);
    Some((
        OtdConfigSummary {
            output_mode,
            area,
            filter_count,
        },
        tablet_name,
    ))
}

pub fn status(runtime: &OtdRuntime, settings: &AppSettings) -> OtdStatus {
    let executable_path = configured_path(settings).map(|p| p.display().to_string());
    let installed = executable_path.is_some();
    let daemon_running = executable_path
        .as_deref()
        .map(Path::new)
        .map(|p| process_running(&process_name(p)))
        .unwrap_or_else(|| process_running("OpenTabletDriver"));
    let config_path = find_config();
    let config_summary = config_path.as_deref().and_then(summarize).map(|x| x.0);
    OtdStatus {
        installed,
        executable_path,
        daemon_running,
        owned_by_opp: runtime
            .process
            .lock()
            .ok()
            .and_then(|mut p| {
                p.as_mut()
                    .map(|child| child.try_wait().ok().flatten().is_none())
            })
            .unwrap_or(false),
        version: None,
        tablet_name: config_path.as_deref().and_then(summarize).and_then(|x| x.1),
        config_path: config_path.map(|p| p.display().to_string()),
        config_summary,
        last_error: runtime.last_error.lock().ok().and_then(|x| x.clone()),
    }
}

pub fn start(runtime: &OtdRuntime, settings: &AppSettings) -> CommandResult<()> {
    let path = configured_path(settings).ok_or_else(|| {
        CommandError::new("OTD_NOT_CONFIGURED", "请先选择 OpenTabletDriver 可执行文件")
    })?;
    if process_running(&process_name(&path)) {
        return Ok(());
    }
    let mut command = Command::new(&path);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let child = command
        .spawn()
        .map_err(|e| CommandError::new("OTD_START_FAILED", e.to_string()))?;
    *runtime
        .process
        .lock()
        .map_err(|_| CommandError::new("OTD_STATE_FAILED", "无法访问 OTD 运行状态"))? = Some(child);
    Ok(())
}

pub fn stop(runtime: &OtdRuntime) -> CommandResult<()> {
    let mut process = runtime
        .process
        .lock()
        .map_err(|_| CommandError::new("OTD_STATE_FAILED", "无法访问 OTD 运行状态"))?;
    if let Some(child) = process.as_mut() {
        child
            .kill()
            .map_err(|e| CommandError::new("OTD_STOP_FAILED", e.to_string()))?;
        let _ = child.wait();
    }
    *process = None;
    Ok(())
}

pub fn backup(settings: &AppSettings, destination: &Path) -> CommandResult<String> {
    let source = find_config().ok_or_else(|| {
        CommandError::new("OTD_CONFIG_NOT_FOUND", "未找到 OpenTabletDriver 配置文件")
    })?;
    if !destination.is_dir() {
        return Err(CommandError::new(
            "OTD_BACKUP_DIRECTORY_INVALID",
            "备份目录不存在",
        ));
    }
    let file = destination.join(format!(
        "OpenTabletDriver-backup-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::copy(&source, &file).map_err(|e| CommandError::new("OTD_BACKUP_FAILED", e.to_string()))?;
    let _ = settings;
    Ok(file.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::{summarize, validate_executable};
    use std::fs;

    #[test]
    fn rejects_missing_executable() {
        let result = validate_executable(std::path::Path::new("missing-opentabletdriver"));
        assert!(result.is_err());
    }

    #[test]
    fn summarizes_known_config_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tablet.json");
        fs::write(
            &path,
            r#"{"OutputMode":"Absolute","Area":{"Width":100},"Filters":[{},{}],"Tablet":"Demo"}"#,
        )
        .expect("write config");
        let (summary, tablet) = summarize(&path).expect("summary");
        assert_eq!(summary.output_mode.as_deref(), Some("Absolute"));
        assert_eq!(summary.filter_count, 2);
        assert_eq!(tablet.as_deref(), Some("Demo"));
    }
}
