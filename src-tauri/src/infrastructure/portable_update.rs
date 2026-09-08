use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

const APPLY_UPDATE_FLAG: &str = "--opp-apply-portable-update";

#[derive(Debug, Clone)]
pub struct PortableUpdatePaths {
    pub current: PathBuf,
    pub staged: PathBuf,
    pub backup: PathBuf,
    pub helper: PathBuf,
}

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

#[cfg(not(windows))]
pub fn schedule_stale_cleanup() {}

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

fn rollback(current: &Path, backup: &Path) -> io::Result<()> {
    remove_if_exists(current)?;
    fs::rename(backup, current)
}

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
