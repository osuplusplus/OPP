//! 平台抽象层：把所有操作系统差异业务模块面向这里的函数与 [`Capabilities`]
//! 不再各自散写 `#[cfg(...)]` 判断

use std::env;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// 当前操作系统（编译期常量）。目前仅适配 `"windows"` 与 `"linux"`。
pub fn current_os() -> &'static str {
    env::consts::OS
}

/// 用户主目录。
pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// 类 Unix 用户数据目录：优先 `XDG_DATA_HOME`，回退到 `~/.local/share`。
#[cfg(not(windows))]
pub fn data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
}

/// 默认下载目录（`~/Downloads`）。
pub fn default_download_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join("Downloads"))
}

/// osu!stable 自动检测候选目录
pub fn stable_install_candidates() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut candidates = Vec::new();
        if let Some(install) = registry_install(|name| name.eq_ignore_ascii_case("osu!")) {
            candidates.push(install);
        }
        if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            candidates.push(local.join("osu!"));
        }
        candidates
    }
    #[cfg(not(windows))]
    {
        // osu-wine 方案的实际安装目录是 `osu-wine/osu!`。
        data_dir()
            .into_iter()
            .map(|data| data.join("osu-wine").join("osu!"))
            .collect()
    }
}

/// osu!lazer 安装候选目录
pub fn lazer_install_candidates() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut candidates = Vec::new();
        if let Some(install) = registry_install(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("osu!") && (name.contains("lazer") || name == "osu!")
        }) {
            candidates.push(install);
        }
        if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            for name in ["osu", "osu!", "osulazer"] {
                candidates.push(local.join(name));
            }
        }
        candidates
    }
    #[cfg(not(windows))]
    {
        data_dir()
            .into_iter()
            .map(|data| data.join("osu"))
            .collect()
    }
}

/// osu!lazer 数据根（含 `client.realm` 与 `storage.ini`）。Windows 为
/// `%APPDATA%/osu`，类 Unix 为 `~/.local/share/osu`。
pub fn lazer_data_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|appdata| appdata.join("osu"))
    }
    #[cfg(not(windows))]
    {
        data_dir().map(|data| data.join("osu"))
    }
}

/// 读取 lazer `storage.ini` 中的 `FullPath`（用户自定义的数据目录）。
fn read_storage_ini_fullpath(storage_ini: &Path) -> Option<PathBuf> {
    let reader = io::BufReader::new(fs::File::open(storage_ini).ok()?);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(value) = line
            .strip_prefix("FullPath")
            .and_then(|rest| rest.split('=').nth(1))
        {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
    }
    None
}

/// osu!lazer 的文件存储根：优先取 `storage.ini` 的 `FullPath`，否则回退到数据根。
pub fn lazer_files_root() -> Option<PathBuf> {
    let data_root = lazer_data_root()?;
    read_storage_ini_fullpath(&data_root.join("storage.ini")).or(Some(data_root))
}

/// Linux 上启动/识别 osu! 客户端用的系统命令名（stable → `osu-wine`，lazer →
/// `osu-lazer`）。Windows 调用方使用安装目录内的可执行文件。
#[cfg(not(windows))]
pub fn game_command(client: &str) -> Option<&'static str> {
    match client {
        "stable" => Some("osu-wine"),
        "lazer" => Some("osu-lazer"),
        _ => None,
    }
}

/// 判断某个 osu! 客户端当前是否正在运行。
pub fn game_process_running(client: &str) -> bool {
    #[cfg(not(windows))]
    {
        match client {
            "stable" => any_process(|comm| comm.ends_with("osu!.exe")),
            "lazer" => any_process(|comm| comm == "osu!" || comm == "osu-lazer"),
            _ => false,
        }
    }
    #[cfg(windows)]
    {
        let _ = client;
        false
    }
}

/// 判断 OBS Studio 是否正在运行
#[cfg(not(windows))]
pub fn obs_process_running() -> bool {
    any_process(|comm| comm == "obs")
}

/// 遍历 `/proc` 的进程名（comm，统一小写），命中即返回 `true`。
#[cfg(not(windows))]
fn any_process(predicate: impl Fn(&str) -> bool) -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
            .is_none()
        {
            continue;
        }
        let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        if predicate(&comm.trim().to_ascii_lowercase()) {
            return true;
        }
    }
    false
}

/// tosu 停止标志文件。pkexec 启动的 root 看门狗脚本轮询它：OPP 创建该文件
/// 即视为停止指令（无需再次认证）；位于 `XDG_RUNTIME_DIR`（用户可写、root 可读）。
#[cfg(not(windows))]
pub fn tosu_stop_flag() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("opp-tosu-stop")
}

/// 在用户 PATH 中查找命令（仅类 Unix；Windows 返回 `None`）。
pub fn find_in_path(command: &str) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        let path = env::var_os("PATH")?;
        env::split_paths(&path)
            .map(|dir| dir.join(command))
            .find(|candidate| candidate.is_file())
    }
    #[cfg(windows)]
    {
        let _ = command;
        None
    }
}

/// 在系统文件管理器中定位（尽量选中）给定文件或目录：Windows 走 `explorer.exe
/// /select,`；类 Unix 经 freedesktop FileManager1 D-Bus 接口打开并选中文件，桌面
/// 不支持时回退 `xdg-open` 打开所在目录。
pub fn reveal_path(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .args(["/select,", &path.to_string_lossy()])
            .creation_flags(0x0800_0000)
            .spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let located = std::process::Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--dest=org.freedesktop.FileManager1",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:file://{}", path.display()),
                "string:",
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if located {
            return Ok(());
        }
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or_else(|| Path::new("/"))
        };
        std::process::Command::new("xdg-open").arg(target).spawn()?;
        Ok(())
    }
}

/// danser-go 在类 Unix 上把 settings 存在 XDG 配置目录（`~/.config/danser`），而非
/// 可执行文件旁边。Windows 沿用可执行文件目录的旧逻辑。
#[cfg(not(windows))]
pub fn danser_config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .map(|config| config.join("danser"))
}

/// 在类 Unix 系统按进程名（comm）精确匹配判断是否运行中。
#[cfg(not(windows))]
pub fn unix_process_running(name: &str) -> bool {
    !unix_process_ids(name).is_empty()
}

/// tosu 的 PID 列表。tosu（Node 运行时）启动后会把进程名改为 `MainThread`
/// （内核截断后可能是 `node-MainThread`），因此按 comm 为 `tosu`，或 comm 含
/// `mainthread` 且 cmdline 含 `tosu` 识别（后者排除同样带 tosu 字样的看门狗 bash）。
#[cfg(not(windows))]
pub fn tosu_process_ids() -> Vec<u32> {
    let mut pids = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let comm = fs::read_to_string(entry.path().join("comm"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let matched = comm == "tosu"
            || (comm.contains("mainthread")
                && fs::read(entry.path().join("cmdline"))
                    .map(|bytes| {
                        String::from_utf8_lossy(&bytes)
                            .to_ascii_lowercase()
                            .contains("tosu")
                    })
                    .unwrap_or(false));
        if matched {
            pids.push(pid);
        }
    }
    pids
}

/// 找出进程名（comm）等于 `name` 的 PID，name 为空时返回空。用于检测运行状态
#[cfg(not(windows))]
pub fn unix_process_ids(name: &str) -> Vec<u32> {
    let name = name.to_ascii_lowercase();
    let mut pids = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        if comm.trim().eq_ignore_ascii_case(&name) {
            pids.push(pid);
        }
    }
    pids
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    /// 当前操作系统。
    pub os: &'static str,
    /// 显示器伽马调节（依赖 Windows GDI）。
    pub display_gamma: bool,
    /// `.osz` / `.osk` 文件关联（依赖 Windows 注册表）。
    pub file_association: bool,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        os: current_os(),
        display_gamma: cfg!(windows),
        file_association: cfg!(windows),
    }
}

#[tauri::command]
pub fn get_capabilities() -> Capabilities {
    capabilities()
}

// ---- Windows 注册表探测 ------

#[cfg(windows)]
fn registry_install(matches_name: impl Fn(&str) -> bool) -> Option<PathBuf> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    };

    const KEYS: [&str; 2] = [
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];

    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let hive = RegKey::predef(hive);
        for key_name in KEYS {
            let Ok(uninstall) = hive.open_subkey(key_name) else {
                continue;
            };
            for subkey_name in uninstall.enum_keys().flatten() {
                let Ok(subkey) = uninstall.open_subkey(subkey_name) else {
                    continue;
                };
                let Ok(display_name) = subkey.get_value::<String, _>("DisplayName") else {
                    continue;
                };
                if !matches_name(&display_name) {
                    continue;
                }
                if let Ok(path) = subkey.get_value::<String, _>("InstallLocation")
                    && !path.trim().is_empty()
                {
                    return Some(PathBuf::from(path.trim().trim_matches('"')));
                }
            }
        }
    }
    None
}
