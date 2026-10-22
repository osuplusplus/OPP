//! Windows 便携版 EXE 自更新的替换与回滚机制：下载与 sha256 校验由上游完成，
//! 本模块用当前 EXE 的临时副本充当更新助手，等主进程退出后原地改名换位，
//! 新版本启动早期崩溃则回滚旧 EXE，应用稳定运行后再清理残留文件。
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

/// 更新助手模式的命令行标志：EXE 临时副本以此参数启动，跳过正常应用初始化。
const APPLY_UPDATE_FLAG: &str = "--opp-apply-portable-update";

/// 一次便携更新涉及的四个路径，全部位于当前 EXE 所在目录。
#[derive(Debug, Clone)]
pub struct PortableUpdatePaths {
    /// 正在运行的 EXE 原路径，更新后保持不变。
    pub current: PathBuf,
    /// 校验通过、等待顶替原位的新版本 EXE。
    pub staged: PathBuf,
    /// 旧版本备份，供回滚使用。
    pub backup: PathBuf,
    /// 当前 EXE 的临时副本，充当更新助手进程。
    pub helper: PathBuf,
}

/// 由当前 EXE 路径与目标版本推导四个路径；staged、backup、helper 均为同目录
/// 下的点前缀隐藏文件，避免被用户当作独立程序误运行。
pub fn paths_for(current: &Path, version: &str) -> io::Result<PortableUpdatePaths> {
    let parent = current
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "当前程序路径没有父目录"))?;
    let file_name = current
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "当前程序文件名无效"))?;
    Ok(PortableUpdatePaths {
        current: current.to_path_buf(),
        staged: parent.join(format!(".{file_name}.update-{version}.exe")),
        backup: parent.join(format!(".{file_name}.opp-old")),
        helper: parent.join(format!(".{file_name}.opp-updater.exe")),
    })
}

/// 使用当前 EXE 的临时副本等待主进程退出，从而保持便携程序原路径不变。
#[cfg(windows)]
pub fn launch_helper(paths: &PortableUpdatePaths) -> io::Result<()> {
    remove_if_exists(&paths.helper)?;
    remove_if_exists(&paths.backup)?;
    fs::copy(&paths.current, &paths.helper)?;
    if let Err(error) = Command::new(&paths.helper)
        .arg(APPLY_UPDATE_FLAG)
        .arg(&paths.current)
        .arg(&paths.staged)
        .arg(&paths.backup)
        .spawn()
    {
        let _ = remove_if_exists(&paths.helper);
        return Err(error);
    }
    Ok(())
}

/// 非 Windows 没有“运行中 EXE 被锁定”的替换问题，也不走便携更新流程，
/// 直接返回不支持。
#[cfg(not(windows))]
pub fn launch_helper(_paths: &PortableUpdatePaths) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "当前平台不支持便携 EXE 自动替换",
    ))
}

/// 在 Tauri 初始化前拦截更新助手参数，避免临时副本启动第二个应用窗口。
pub fn run_helper_if_requested() -> bool {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(APPLY_UPDATE_FLAG)) {
        return false;
    }
    let current = args.next().map(PathBuf::from);
    let staged = args.next().map(PathBuf::from);
    let backup = args.next().map(PathBuf::from);
    let result = match (current, staged, backup) {
        (Some(current), Some(staged), Some(backup)) => run_helper(&current, &staged, &backup),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "便携更新助手参数不完整",
        )),
    };
    if let Err(error) = result {
        let _ = fs::write("opp-update-error.log", error.to_string());
    }
    true
}

/// 应用稳定运行一段时间后再清理备份，崩溃时仍给下一次恢复留下旧 EXE。
#[cfg(windows)]
pub fn schedule_stale_cleanup() {
    let Ok(current) = env::current_exe() else {
        return;
    };
    let Ok(paths) = paths_for(&current, "cleanup") else {
        return;
    };
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(12));
        for _ in 0..20 {
            let helper_removed = remove_if_exists(&paths.helper).is_ok();
            let backup_removed = remove_if_exists(&paths.backup).is_ok();
            if helper_removed && backup_removed {
                break;
            }
            thread::sleep(Duration::from_millis(250));
        }
    });
}

/// 非 Windows 没有替换流程，也不会产生需要清理的残留文件。
#[cfg(not(windows))]
pub fn schedule_stale_cleanup() {}

/// 助手主流程：等旧进程退出并完成文件替换，拉起新版本并观察最初 8 秒
///（80 次 × 100ms）；非零退出码视为更新失败，回滚旧 EXE 并重新启动，
/// 用户主动正常关闭不当作失败。
fn run_helper(current: &Path, staged: &Path, backup: &Path) -> io::Result<()> {
    remove_if_exists(backup)?;
    wait_for_exit_and_replace(current, staged, backup)?;

    let mut child = match Command::new(current).spawn() {
        Ok(child) => child,
        Err(error) => {
            rollback(current, backup)?;
            let _ = Command::new(current).spawn();
            return Err(error);
        }
    };

    // 新版本若在启动早期异常退出，则恢复旧版本；正常关闭不视为更新失败。
    for _ in 0..80 {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                rollback(current, backup)?;
                let _ = Command::new(current).spawn();
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// 轮询把旧 EXE 改名为备份（最长 30 秒）：Windows 下旧进程未退出时文件被
/// 锁定，改名会在其退出后自然成功；随后把 staged 顶替到原路径，顶替失败
/// 立即把备份改回去。
fn wait_for_exit_and_replace(current: &Path, staged: &Path, backup: &Path) -> io::Result<()> {
    let mut last_error = None;
    for _ in 0..300 {
        match fs::rename(current, backup) {
            Ok(()) => {
                if let Err(error) = fs::rename(staged, current) {
                    let _ = fs::rename(backup, current);
                    return Err(error);
                }
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("等待旧程序退出超时")))
}

/// 回滚：移除损坏的新 EXE，把备份恢复到原路径。
fn rollback(current: &Path, backup: &Path) -> io::Result<()> {
    remove_if_exists(current)?;
    fs::rename(backup, current)
}

/// 幂等删除：目标不存在同样视为成功，便于重试清理。
fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::{rollback, wait_for_exit_and_replace};

    #[cfg(unix)]
    use super::run_helper;

    // 替换后原路径应为新内容，旧内容保留在备份文件中。
    #[test]
    fn replacement_keeps_the_original_path_and_backup() {
        let directory = tempdir().expect("tempdir");
        let current = directory.path().join("OPP.exe");
        let staged = directory.path().join("OPP.update.exe");
        let backup = directory.path().join("OPP.old");
        fs::write(&current, b"old").expect("write old");
        fs::write(&staged, b"new").expect("write new");

        wait_for_exit_and_replace(&current, &staged, &backup).expect("replace files");

        assert_eq!(fs::read(&current).expect("read current"), b"new");
        assert_eq!(fs::read(&backup).expect("read backup"), b"old");
    }

    // 回滚应把备份内容恢复到原路径并清掉备份文件。
    #[test]
    fn rollback_restores_the_previous_executable() {
        let directory = tempdir().expect("tempdir");
        let current = directory.path().join("OPP.exe");
        let backup = directory.path().join("OPP.old");
        fs::write(&current, b"broken").expect("write broken");
        fs::write(&backup, b"old").expect("write old");

        rollback(&current, &backup).expect("rollback");

        assert_eq!(fs::read(&current).expect("read current"), b"old");
        assert!(!backup.exists());
    }

    // 新版本启动即失败时应恢复旧版本并重新拉起（仅在 Unix 上模拟）。
    #[cfg(unix)]
    #[test]
    fn early_startup_failure_restores_and_restarts_the_previous_executable() {
        let directory = tempdir().expect("tempdir");
        let current = directory.path().join("OPP.exe");
        let staged = directory.path().join("OPP.update.exe");
        let backup = directory.path().join("OPP.old");
        fs::write(&current, b"#!/bin/sh\nexit 0\n").expect("write old executable");
        fs::write(&staged, b"#!/bin/sh\nexit 1\n").expect("write broken executable");
        fs::set_permissions(&current, fs::Permissions::from_mode(0o755)).expect("chmod old");
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).expect("chmod staged");

        run_helper(&current, &staged, &backup).expect("helper handles early failure");

        assert_eq!(
            fs::read(&current).expect("read restored executable"),
            b"#!/bin/sh\nexit 0\n"
        );
        assert!(!backup.exists());
    }
}
