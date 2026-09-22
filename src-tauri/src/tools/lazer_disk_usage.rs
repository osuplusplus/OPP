//! osu!lazer 数据目录占用统计：总大小（逻辑）与排除硬链接后的实际占用。
//! lazer 从 stable 导入的文件以硬链接形式存在（另一链接指向 stable 目录），
//! 删除 lazer 目录不会释放这部分空间，因此实际占用不计入任何硬链接文件

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;
use walkdir::WalkDir;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::{LocalAnalysisService, LocalClient},
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

#[derive(Debug, Clone, Serialize)]
pub struct LazerDiskUsage {
    pub path: String,
    pub total_size: u64,
    pub unique_size: u64,
    pub file_count: u64,
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_lazer_disk_usage(
    state: tauri::State<'_, AppState>,
) -> CommandResult<LazerDiskUsage> {
    let span = global().map(|logger| logger.operation("tools", "get_lazer_disk_usage"));
    finish_span(span, disk_usage(&state.local_analysis).await)
}

async fn disk_usage(analysis: &LocalAnalysisService) -> CommandResult<LazerDiskUsage> {
    let root = usage_root(analysis)?;
    let path = root.display().to_string();
    let (total_size, unique_size, file_count) =
        crate::infrastructure::tasks::background("tools", move || compute_size(&root))
            .await
            .map_err(|join| CommandError::new("LAZER_SCAN_FAILED", join.to_string()))?;
    Ok(LazerDiskUsage {
        path,
        total_size,
        unique_size,
        file_count,
    })
}

fn usage_root(analysis: &LocalAnalysisService) -> CommandResult<PathBuf> {
    let source = analysis.source_status(LocalClient::Lazer)?;
    source
        .data_root
        .filter(|_| source.valid)
        .map(PathBuf::from)
        .ok_or_else(|| CommandError::new("LAZER_NOT_FOUND", "未配置有效的 osu!lazer 数据目录"))
}

fn compute_size(root: &Path) -> (u64, u64, u64) {
    let files: Vec<(u64, bool)> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .collect::<Vec<_>>()
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let size = std::fs::metadata(path).ok()?.len();
            Some((size, hard_linked(path)))
        })
        .collect();
    let total_size: u64 = files.iter().map(|(size, _)| size).sum();
    let unique_size: u64 = files
        .iter()
        .filter(|(_, linked)| !linked)
        .map(|(size, _)| size)
        .sum();
    (total_size, unique_size, files.len() as u64)
}

/// 文件是否存在目录外的硬链接（链接数 > 1）。
#[cfg(not(windows))]
fn hard_linked(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.nlink() > 1)
        .unwrap_or(false)
}

#[cfg(windows)]
fn hard_linked(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_EXISTING,
    };

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    unsafe {
        let handle = CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let ok = GetFileInformationByHandle(handle, &mut info);
        CloseHandle(handle);
        ok != 0 && info.nNumberOfLinks > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lazer_usage_follows_override_and_rejects_missing_source() {
        let app = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join("client.realm"), b"realm").unwrap();
        std::fs::create_dir(data.path().join("files")).unwrap();
        std::fs::write(data.path().join("files").join("sample"), b"sample").unwrap();
        let service = LocalAnalysisService::new(app.path()).unwrap();
        service.set_source(LocalClient::Lazer, data.path()).unwrap();
        let result = disk_usage(&service).await.unwrap();
        assert_eq!(
            Path::new(&result.path).canonicalize().unwrap(),
            data.path().canonicalize().unwrap()
        );
        assert_eq!(
            (result.total_size, result.unique_size, result.file_count),
            (11, 11, 2)
        );
        std::fs::remove_file(data.path().join("client.realm")).unwrap();
        assert_eq!(
            disk_usage(&service).await.unwrap_err().code,
            "LAZER_NOT_FOUND"
        );
    }
}
