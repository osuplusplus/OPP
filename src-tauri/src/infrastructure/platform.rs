//! 平台抽象层：把所有操作系统差异业务模块面向这里的函数与 [`Capabilities`]
//! 不再各自散写 `#[cfg(...)]` 判断

use std::env;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// SQLite WAL requires a local filesystem; reject recognized network volumes.
///
/// SQLite 的 WAL 与原子改名在网络卷上不可靠，因此按平台识别网络位置
/// （Windows 的 UNC 路径／映射网络盘，Linux 的 NFS、CIFS 等挂载）并直接拒绝。
pub(crate) fn validate_local_database_directory(path: &Path) -> crate::error::CommandResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::path::{Component, Prefix};
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        use windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOTE;
        // 从路径前缀判别：UNC 前缀本身就是网络共享；盘符需查询驱动器类型。
        if let Some(Component::Prefix(prefix)) = path.components().next() {
            match prefix.kind() {
                Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) => {
                    return Err(crate::error::CommandError::new(
                        "DATABASE_NETWORK_DIRECTORY",
                        "请选择本机磁盘目录，数据库不支持网络共享",
                    ));
                }
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    let root = format!("{}:\\", char::from(letter));
                    let wide: Vec<u16> = std::ffi::OsStr::new(&root)
                        .encode_wide()
                        .chain(Some(0))
                        .collect();
                    // SAFETY: wide is a valid null-terminated drive-root string.
                    if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_REMOTE {
                        return Err(crate::error::CommandError::new(
                            "DATABASE_NETWORK_DIRECTORY",
                            "请选择本机磁盘目录，数据库不支持映射网络驱动器",
                        ));
                    }
                }
                _ => {
                    return Err(crate::error::CommandError::new(
                        "DATABASE_PATH_INVALID",
                        "请选择本机磁盘上的普通文件夹",
                    ));
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // 解析 /proc/self/mountinfo：找到覆盖目标路径的最长挂载点，
        // 取其文件系统类型，再对照网络文件系统黑名单判断。
        let mount_path = Path::new("/proc/self/mountinfo");
        let mounts = crate::infrastructure::local_database::fs_io(
            "read_mounts",
            mount_path,
            fs::read_to_string(mount_path),
        )?;
        let filesystem = mounts
            .lines()
            .filter_map(|line| {
                let (fields, details) = line.split_once(" - ")?;
                let mount = fields
                    .split_whitespace()
                    .nth(4)?
                    .replace("\\040", " ")
                    .replace("\\011", "\t")
                    .replace("\\012", "\n")
                    .replace("\\134", "\\");
                let fs_type = details.split_whitespace().next()?;
                path.starts_with(&mount).then_some((mount.len(), fs_type))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, fs_type)| fs_type);
        if filesystem.is_some_and(|fs_type| {
            matches!(
                fs_type,
                "nfs"
                    | "nfs4"
                    | "cifs"
                    | "smb3"
                    | "9p"
                    | "ceph"
                    | "afs"
                    | "fuse.sshfs"
                    | "fuse.rclone"
                    | "fuse.glusterfs"
                    | "fuse.davfs"
            )
        }) {
            return Err(crate::error::CommandError::new(
                "DATABASE_NETWORK_DIRECTORY",
                "请选择本机磁盘目录，数据库不支持网络挂载",
            ));
        }
    }
    Ok(())
}

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
        let Some(value) = line
            .strip_prefix("FullPath")
            .and_then(|rest| rest.trim_start().strip_prefix('='))
        else {
            continue;
        };
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    None
}

/// 解析 osu!lazer 实际使用的数据根：`storage.ini` 的 `FullPath` 存在时
/// 无条件只认它，不回退；没有 ini 才用默认数据根。
pub fn resolve_lazer_data_root() -> Option<PathBuf> {
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
        // Windows 端暂未实现该检测，恒为 false，调用方需自行降级处理。
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

/// Windows 下用 tasklist 按 IMAGENAME 过滤判断进程是否在运行。
#[cfg(windows)]
pub fn windows_tasklist_running(image_name: &str) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("tasklist")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/FI", &format!("IMAGENAME eq {image_name}"), "/NH"])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains(&image_name.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

/// 按 `/proc` 的 cmdline 全文匹配（等价 `pgrep -f`）。OpenTabletDriver 等
/// 以 dotnet／AppImage 方式启动时 comm 会被内核截断，只能靠命令行识别。
#[cfg(not(windows))]
pub fn unix_cmdline_running(needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
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
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let text = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        if text.to_ascii_lowercase().contains(&needle) {
            return true;
        }
    }
    false
}

/// 跨平台进程检测：Windows 用 tasklist 按 IMAGENAME 查，其他平台按 `/proc` comm 精确匹配。
pub fn named_process_running(windows_name: &str, unix_name: &str) -> bool {
    #[cfg(windows)]
    {
        let _ = unix_name;
        windows_tasklist_running(windows_name)
    }
    #[cfg(not(windows))]
    {
        let _ = windows_name;
        unix_process_running(unix_name)
    }
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
        // Windows 同样执行查找：与类 Unix 的差异在于还需按 PATHEXT 补全扩展名。
        // 按真实 shell 语义扫描进程 PATH:先试原名(可能已带扩展),
        // 再按 PATHEXT(.EXE/.CMD/.BAT…)逐个拼接。
        let exts: Vec<String> = env::var("PATHEXT")
            .map(|v| v.split(';').map(str::to_string).collect())
            .unwrap_or_else(|_| vec![".exe".into(), ".cmd".into(), ".bat".into()]);
        if let Some(path) = env::var_os("PATH") {
            for dir in env::split_paths(&path) {
                let direct = dir.join(command);
                if direct.is_file() {
                    return Some(direct);
                }
                for ext in &exts {
                    let candidate = dir.join(format!("{command}{ext}"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }
}

/// AppImage 运行时（AppRun）会把内置库目录注入动态链接器变量（`LD_LIBRARY_PATH`，
/// 偶有 `LD_PRELOAD`／`LD_AUDIT`），并原样继承给 OPP 派生的所有子进程：系统程序
/// （如 `xdg-open` → `kde-open`）会混载 AppImage 内置库与系统库，符号不匹配时直接
/// 崩溃——SteamOS 上 OAuth 授权因此打不开浏览器（issue #51）。opener 插件在内部
/// 派生进程、无法逐点注入环境，只能在启动早期从自身环境中把指向 `APPDIR` 的注入
/// 剥离。此刻 OPP 自身的符号解析已完成，内置库之间另有 linuxdeploy 写入的
/// `$ORIGIN` 相对 RUNPATH 兜底；剥离后子进程（xdg-open、danser、tosu、ffmpeg 等）
/// 回到纯净的系统库解析环境。非 AppImage 运行（dev、deb）不会设置 `APPDIR`，
/// 本函数不做任何事，用户自行设置的同名变量不受影响。
pub fn strip_appimage_loader_env() {
    #[cfg(target_os = "linux")]
    {
        let Some(raw) = env::var_os("APPDIR") else {
            return;
        };
        // AppRun 写入的条目与 APPDIR 逐字一致，按文本前缀比对即可。
        let appdir = raw.to_string_lossy().trim_end_matches('/').to_string();
        if let Some(current) = env::var_os("LD_LIBRARY_PATH") {
            match library_path_without_appdir(&current.to_string_lossy(), &appdir) {
                Some(filtered) => {
                    // SAFETY: 仅在应用启动主线程、Tauri 运行时与其他线程拉起前调用。
                    unsafe { env::set_var("LD_LIBRARY_PATH", filtered) };
                }
                None => unsafe { env::remove_var("LD_LIBRARY_PATH") },
            }
        }
        for name in ["LD_PRELOAD", "LD_AUDIT"] {
            if env::var_os(name)
                .is_some_and(|value| references_appdir(&value.to_string_lossy(), &appdir))
            {
                // SAFETY: 同上。
                unsafe { env::remove_var(name) };
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {}
}

/// 从 `LD_LIBRARY_PATH` 的冒号分隔条目中剔除指向 `appdir` 的条目，保留用户原有
/// 条目；全部被剔除时返回 `None`（应删除该变量而不是留空）。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn library_path_without_appdir(value: &str, appdir: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let kept: Vec<&str> = value
        .split(':')
        .filter(|entry| !entry_under_appdir(entry, appdir))
        .collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(":"))
    }
}

/// 判断单个 `LD_LIBRARY_PATH` 条目是否就是 `appdir` 或其子路径。
/// 空条目（ld.so 视为当前目录）不属于 AppImage 注入，应保留原语义。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn entry_under_appdir(entry: &str, appdir: &str) -> bool {
    let entry = entry.trim_end_matches('/');
    !entry.is_empty() && (entry == appdir || entry.starts_with(&format!("{appdir}/")))
}

/// `LD_PRELOAD`／`LD_AUDIT` 是空格／冒号分隔的库名或路径混合列表，无法像库路径
/// 那样逐条解析；只要文本中出现 APPDIR 路径即整体视为 AppImage 注入并删除。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn references_appdir(value: &str, appdir: &str) -> bool {
    value.contains(appdir)
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

/// 汇总当前平台的能力开关；前端据此隐藏不支持的入口（如 Linux 下的伽马调节）。
pub fn capabilities() -> Capabilities {
    Capabilities {
        os: current_os(),
        display_gamma: cfg!(windows),
        file_association: cfg!(windows),
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_capabilities() -> Capabilities {
    capabilities()
}

// ---- Windows 注册表探测 ------

/// 扫描 Windows 卸载注册表（HKCU/HKLM 各自的 Uninstall 与 WOW6432Node 两个
/// 视图），以 `DisplayName` 匹配目标程序并返回其安装目录。
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
                // osu! 的卸载项常不写 InstallLocation，路径只在 UninstallString
                // （如 `D:\osu!\osu!.exe -uninstall`）里，取可执行文件所在目录。
                if let Some(directory) = subkey
                    .get_value::<String, _>("UninstallString")
                    .ok()
                    .and_then(|value| uninstall_string_exe(&value))
                    .and_then(|exe| exe.parent().map(Path::to_path_buf))
                {
                    return Some(directory);
                }
            }
        }
    }
    None
}

/// 从 `UninstallString` 提取可执行文件路径：兼容带引号
/// （`"C:\dir\osu!.exe" -uninstall`）与不带引号（`D:\osu!\osu!.exe -uninstall`，
/// 目录含空格也正确）两种写法。
#[cfg(windows)]
fn uninstall_string_exe(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    let exe = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        let position = trimmed.to_ascii_lowercase().find(".exe")?;
        &trimmed[..position + ".exe".len()]
    };
    let path = PathBuf::from(exe.trim());
    (!path.as_os_str().is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_path_keeps_user_entries_and_drops_appdir_ones() {
        let appdir = "/tmp/.mount_OPPxYz";
        let value = format!("{appdir}/usr/lib:/opt/custom/lib:{appdir}");
        assert_eq!(
            library_path_without_appdir(&value, appdir),
            Some("/opt/custom/lib".to_string())
        );
    }

    #[test]
    fn library_path_removed_when_only_appdir_entries_remain() {
        let appdir = "/tmp/.mount_OPPxYz";
        assert_eq!(
            library_path_without_appdir("/tmp/.mount_OPPxYz/usr/lib", appdir),
            None
        );
        // 原值为空同样应删除变量，而不是保留空串。
        assert_eq!(library_path_without_appdir("", appdir), None);
    }

    #[test]
    fn library_path_prefix_must_align_on_path_component() {
        let appdir = "/tmp/.mount_OPP";
        // 前缀相似但不在 APPDIR 之下的目录不能被误删。
        assert_eq!(
            library_path_without_appdir("/tmp/.mount_OPPBACKUP/lib", appdir),
            Some("/tmp/.mount_OPPBACKUP/lib".to_string())
        );
        // 条目末尾多余的斜杠仍视为 APPDIR 注入；空条目（当前目录语义）保留。
        assert_eq!(
            library_path_without_appdir("/tmp/.mount_OPP/usr/lib/:", appdir),
            Some(String::new())
        );
    }

    #[test]
    fn preload_vars_detected_only_when_appdir_referenced() {
        let appdir = "/tmp/.mount_OPPxYz";
        assert!(references_appdir(
            "/tmp/.mount_OPPxYz/usr/lib/libpreload.so",
            appdir
        ));
        assert!(!references_appdir("mangohud.so", appdir));
    }
}
