//! 压缩 lazer 空间：lazer 的 files 存储以内容 SHA-256 为文件名。扫描 stable
//! 谱面目录（Songs）中与 lazer 文件同大小的文件，仅对这些候选计算 SHA-256 并
//! 与 lazer 文件名比对；完全一致的副本用指向 stable 文件的硬链接原子替换，从
//! 而释放重复占用的空间。逐文件校验同卷与同 inode，失败（跨分区、FAT32、权
//! 限等）只记录不中断；替换通过「临时硬链接 + rename」完成，任一时刻文件要么
//! 是原副本、要么是完整硬链接。

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};
use walkdir::WalkDir;

use crate::{
    error::{CommandError, CommandResult},
    local_analysis::{LocalAnalysisService, LocalClient},
    platform,
    state::AppState,
};

/// 取消标志：同一时间只应有一个去重任务，模块级原子量足够。
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// 单个失败条目的展示上限，避免异常文件系统（如整盘不支持硬链接）撑爆事件。
const MAX_FAILURES: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct LazerDedupeProgress {
    pub phase: &'static str,
    pub processed: usize,
    pub total: usize,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LazerDedupeFailure {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LazerDedupeResult {
    pub dry_run: bool,
    pub cancelled: bool,
    pub lazer_files_root: String,
    pub stable_roots: Vec<String>,
    pub lazer_file_count: u64,
    pub lazer_total_size: u64,
    pub already_linked_count: u64,
    pub already_linked_size: u64,
    pub hashed_stable_count: u64,
    pub candidate_count: u64,
    pub reclaimable_size: u64,
    pub linked_count: u64,
    pub linked_size: u64,
    pub skipped_cross_volume_count: u64,
    pub skipped_cross_volume_size: u64,
    pub failed_count: u64,
    pub failed: Vec<LazerDedupeFailure>,
}

struct LazerFile {
    hash: String,
    size: u64,
    path: PathBuf,
    volume: u64,
}

#[tauri::command]
pub async fn dedupe_lazer_files(
    dry_run: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<LazerDedupeResult> {
    let service = Arc::clone(&state.local_analysis);
    let emit = Arc::new(move |progress: LazerDedupeProgress| {
        let _ = app.emit("lazer-dedupe-progress", progress);
    });
    tokio::task::spawn_blocking(move || run(service, dry_run, emit))
        .await
        .map_err(|join| {
            CommandError::new("LAZER_DEDUPE_TASK_ERROR", format!("任务异常结束：{join}"))
        })?
}

#[tauri::command]
pub fn cancel_lazer_dedupe() {
    CANCELLED.store(true, Ordering::Relaxed);
}

fn run(
    service: Arc<LocalAnalysisService>,
    dry_run: bool,
    emit: Arc<dyn Fn(LazerDedupeProgress) + Send + Sync>,
) -> CommandResult<LazerDedupeResult> {
    CANCELLED.store(false, Ordering::Relaxed);
    let reporter = ProgressReporter::new(emit);
    let mut result = LazerDedupeResult {
        dry_run,
        ..LazerDedupeResult::default()
    };

    if platform::game_process_running("lazer") {
        return Err(CommandError::new(
            "LAZER_RUNNING",
            "检测到 osu!lazer 正在运行，为避免与游戏写入竞争请先关闭它再执行",
        ));
    }

    let lazer_root = resolve_lazer_files_root(&service)?;
    let stable_roots = resolve_stable_roots(&service)?;
    result.lazer_files_root = display(&lazer_root);
    result.stable_roots = stable_roots.iter().map(|path| display(path)).collect();

    // 1. 扫描 lazer 文件存储：分类出已是硬链接（无需处理）与待匹配文件。
    reporter.emit("scan-lazer", 0, 0, true);
    let lazer_entries = enumerate_files(&[lazer_root], &reporter, "scan-lazer");
    if CANCELLED.load(Ordering::Relaxed) {
        return Ok(cancelled(result));
    }
    let mut pending: Vec<LazerFile> = Vec::new();
    for (path, size) in &lazer_entries {
        result.lazer_file_count += 1;
        result.lazer_total_size += size;
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_content_hash(name) {
            continue;
        }
        let Some(info) = file_info(path) else {
            continue;
        };
        if info.links > 1 {
            result.already_linked_count += 1;
            result.already_linked_size += size;
            continue;
        }
        pending.push(LazerFile {
            hash: name.to_ascii_lowercase(),
            size: *size,
            path: path.clone(),
            volume: info.volume,
        });
    }
    if pending.is_empty() {
        return Ok(result);
    }

    // 2. 扫描 stable，只保留大小能对上待匹配 lazer 文件的候选。
    reporter.emit("scan-stable", 0, 0, true);
    let stable_entries = enumerate_files(&stable_roots, &reporter, "scan-stable");
    if CANCELLED.load(Ordering::Relaxed) {
        return Ok(cancelled(result));
    }
    let sizes: HashSet<u64> = pending.iter().map(|file| file.size).collect();
    let candidates: Vec<PathBuf> = stable_entries
        .iter()
        .filter(|(_, size)| sizes.contains(size))
        .map(|(path, _)| path.clone())
        .collect();
    result.hashed_stable_count = candidates.len() as u64;

    // 3. 并行计算候选 stable 文件的 SHA-256，与 lazer 文件名（即哈希）匹配。
    reporter.emit("hash", 0, candidates.len(), true);
    let processed = AtomicUsize::new(0);
    let by_hash: HashMap<String, PathBuf> = candidates
        .par_iter()
        .filter_map(|path| {
            if CANCELLED.load(Ordering::Relaxed) {
                return None;
            }
            let hash = hash_file(path).ok()?;
            let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
            reporter.emit("hash", done, candidates.len(), false);
            Some((hash, path.clone()))
        })
        .collect();
    if CANCELLED.load(Ordering::Relaxed) {
        return Ok(cancelled(result));
    }
    reporter.emit("hash", candidates.len(), candidates.len(), true);

    // 4. 匹配 + 同卷校验。跨卷（不同分区/文件系统）无法硬链接，单独计数。
    let mut pairs = Vec::new();
    for file in pending {
        let Some(stable) = by_hash.get(&file.hash) else {
            continue;
        };
        pairs.push((file, stable.clone()));
    }
    let mut cross_volume = Vec::new();
    let mut pairs_on_volume = Vec::new();
    for (file, stable) in pairs {
        match file_info(&stable) {
            Some(info) if info.volume == file.volume => pairs_on_volume.push((file, stable)),
            Some(_) => cross_volume.push((file, stable)),
            None => cross_volume.push((file, stable)),
        }
    }
    result.candidate_count = (pairs_on_volume.len() + cross_volume.len()) as u64;
    result.reclaimable_size = pairs_on_volume.iter().map(|(file, _)| file.size).sum();
    result.skipped_cross_volume_count = cross_volume.len() as u64;
    result.skipped_cross_volume_size = cross_volume.iter().map(|(file, _)| file.size).sum();
    if dry_run || pairs_on_volume.is_empty() {
        return Ok(result);
    }

    // 5. 逐个替换：临时硬链接 → rename 覆盖，保证中间态不出现半成品。
    let total = pairs_on_volume.len();
    reporter.emit("link", 0, total, true);
    for (index, (file, stable)) in pairs_on_volume.into_iter().enumerate() {
        if CANCELLED.load(Ordering::Relaxed) {
            return Ok(cancelled(result));
        }
        reporter.emit("link", index, total, false);
        match link_replace(&file, &stable) {
            Ok(()) => {
                result.linked_count += 1;
                result.linked_size += file.size;
            }
            Err(message) => record_failure(&mut result, &file.path, message),
        }
    }
    reporter.emit("link", total, total, true);
    Ok(result)
}

fn cancelled(mut result: LazerDedupeResult) -> LazerDedupeResult {
    result.cancelled = true;
    result
}

fn record_failure(result: &mut LazerDedupeResult, path: &Path, message: String) {
    result.failed_count += 1;
    if result.failed.len() < MAX_FAILURES {
        result.failed.push(LazerDedupeFailure {
            path: display(path),
            message,
        });
    }
}

/// 把 lazer 副本替换为指向 stable 的硬链接。
fn link_replace(file: &LazerFile, stable: &Path) -> Result<(), String> {
    let fresh = |path: &Path| fs::metadata(path).map(|metadata| metadata.len());
    if fresh(stable).unwrap_or(u64::MAX) != file.size {
        return Err("stable 文件在扫描后发生变化，已跳过".into());
    }
    if fresh(&file.path).unwrap_or(u64::MAX) != file.size {
        return Err("lazer 文件在扫描后发生变化，已跳过".into());
    }
    let temporary = file.path.with_extension("opp-link");
    let _ = fs::remove_file(&temporary);
    if let Err(error) = fs::hard_link(stable, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("无法创建硬链接：{error}"));
    }
    if let Err(error) = fs::rename(&temporary, &file.path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("无法替换原文件：{error}"));
    }
    Ok(())
}

fn resolve_lazer_files_root(service: &LocalAnalysisService) -> CommandResult<PathBuf> {
    let resolved = service.resolved_source(LocalClient::Lazer).ok();
    resolved
        .and_then(|source| source.repository_root)
        .or_else(|| platform::lazer_files_root().map(|root| root.join("files")))
        .filter(|path| path.is_dir())
        .ok_or_else(|| {
            CommandError::new(
                "LAZER_NOT_FOUND",
                "未找到 osu!lazer 文件存储目录（数据目录下的 files）",
            )
        })
}

fn resolve_stable_roots(service: &LocalAnalysisService) -> CommandResult<Vec<PathBuf>> {
    let resolved = service.resolved_source(LocalClient::Stable)?;
    // 只匹配谱面目录（默认 Songs，含用户自定义的 BeatmapDirectory），不扫 Skins。
    if let Some(beatmaps) = resolved.beatmap_root.as_ref().filter(|path| path.is_dir()) {
        return Ok(vec![beatmaps.clone()]);
    }
    let detail = if resolved.status.validation_errors.is_empty() {
        "未找到 Songs 谱面目录".to_string()
    } else {
        resolved.status.validation_errors.join("；")
    };
    Err(CommandError::new(
        "STABLE_NOT_FOUND",
        format!("未找到可扫描的 osu!stable 谱面目录：{detail}"),
    ))
}

/// 遍历目录收集文件与大小。符号链接（不跟随）天然被 `is_file` 排除。
fn enumerate_files(
    roots: &[PathBuf],
    reporter: &ProgressReporter,
    phase: &'static str,
) -> Vec<(PathBuf, u64)> {
    let mut walked = 0usize;
    let mut paths = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if CANCELLED.load(Ordering::Relaxed) {
                return Vec::new();
            }
            if !entry.file_type().is_file() {
                continue;
            }
            paths.push(entry.into_path());
            walked += 1;
            if walked.is_multiple_of(2048) {
                reporter.emit(phase, walked, 0, false);
            }
        }
    }
    reporter.emit(phase, walked, 0, true);
    paths
        .par_iter()
        .filter_map(|path| {
            let size = fs::metadata(path).ok()?.len();
            Some((path.clone(), size))
        })
        .collect()
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// lazer 文件名为内容哈希（64 位十六进制字符）；其他命名的文件不参与匹配。
fn is_content_hash(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

struct ProgressReporter {
    emit: Arc<dyn Fn(LazerDedupeProgress) + Send + Sync>,
    last_emit: Mutex<Instant>,
}

impl ProgressReporter {
    fn new(emit: Arc<dyn Fn(LazerDedupeProgress) + Send + Sync>) -> Self {
        Self {
            emit,
            last_emit: Mutex::new(Instant::now() - Duration::from_secs(1)),
        }
    }

    fn emit(&self, phase: &'static str, processed: usize, total: usize, force: bool) {
        let Ok(mut last_emit) = self.last_emit.lock() else {
            return;
        };
        if !force && last_emit.elapsed() < Duration::from_millis(100) {
            return;
        }
        *last_emit = Instant::now();
        drop(last_emit);
        let percent = if total > 0 {
            ((processed as f64 / total as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        };
        (self.emit)(LazerDedupeProgress {
            phase,
            processed,
            total,
            percent: percent.min(100.0),
        });
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// 文件标识：卷/设备号 + inode/文件索引 + 硬链接数，用于同卷与同文件判断。
struct FileInfo {
    volume: u64,
    #[cfg_attr(not(test), allow(dead_code))]
    index: u64,
    links: u32,
}

#[cfg(not(windows))]
fn file_info(path: &Path) -> Option<FileInfo> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path).ok()?;
    Some(FileInfo {
        volume: metadata.dev(),
        index: metadata.ino(),
        links: metadata.nlink() as u32,
    })
}

#[cfg(windows)]
fn file_info(path: &Path) -> Option<FileInfo> {
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
            return None;
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let ok = GetFileInformationByHandle(handle, &mut info);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        Some(FileInfo {
            volume: info.dwVolumeSerialNumber as u64,
            index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
            links: info.nNumberOfLinks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_content_hashes() {
        assert!(is_content_hash(&"a".repeat(64)));
        assert!(!is_content_hash(&"a".repeat(32)));
        assert!(!is_content_hash("song-title.mp3"));
    }

    #[test]
    fn hashes_files_incrementally() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("data.bin");
        fs::write(&path, b"opp").expect("write file");
        assert_eq!(
            hash_file(&path).expect("hash file"),
            // sha256("opp") 的十六进制表示
            "591a0354f0692cb69c9d592a101cecec3efa25be9cbc0029e58447ca2fcb3de3"
        );
    }

    #[test]
    fn replaces_duplicate_with_hard_link() {
        let directory = tempfile::tempdir().expect("temp directory");
        let stable = directory.path().join("stable.bin");
        let lazer = directory.path().join("lazer.bin");
        fs::write(&stable, b"duplicate content").expect("write stable");
        fs::write(&lazer, b"duplicate content").expect("write lazer");
        let file = LazerFile {
            hash: hash_file(&stable).expect("hash"),
            size: 17,
            path: lazer.clone(),
            volume: file_info(&stable).expect("stable info").volume,
        };
        link_replace(&file, &stable).expect("link replace");
        assert_eq!(fs::read(&lazer).expect("read lazer"), b"duplicate content");
        let stable_info = file_info(&stable).expect("stable info");
        let lazer_info = file_info(&lazer).expect("lazer info");
        assert_eq!(stable_info.volume, lazer_info.volume);
        assert_eq!(stable_info.index, lazer_info.index);
        assert_eq!(lazer_info.links, 2);
        assert!(!lazer.with_extension("opp-link").exists());
    }
}
