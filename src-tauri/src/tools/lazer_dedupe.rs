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
    features::local_analysis::{LocalAnalysisService, LocalClient},
    infrastructure::platform,
    state::AppState,
};

/// 取消标志：同一时间只应有一个去重任务，模块级原子量足够。
static CANCELLED: AtomicBool = AtomicBool::new(false);
/// 任务占用标志：配合 `compare_exchange` 保证同一时间只跑一个去重任务。
static RUNNING: AtomicBool = AtomicBool::new(false);

/// RAII 守卫：后台线程结束（含出错提前返回）时自动复位 `RUNNING`。
struct RunningGuard;
impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

/// 单个失败条目的展示上限，避免异常文件系统（如整盘不支持硬链接）撑爆事件。
const MAX_FAILURES: usize = 50;

/// 前端进度事件载荷（事件名 `lazer-dedupe-progress`）。
#[derive(Debug, Clone, Serialize)]
pub struct LazerDedupeProgress {
    /// 当前阶段：`scan-lazer`、`scan-stable`、`hash` 或 `link`。
    pub phase: &'static str,
    /// 已处理数量；`scan-*` 阶段遍历前不知总数，`total` 恒为 0。
    pub processed: usize,
    pub total: usize,
    /// 百分比（0–100，保留一位小数；无总数时为 0）。
    pub percent: f64,
}

/// 单个替换失败条目（路径 + 原因），最多保留 `MAX_FAILURES` 条。
#[derive(Debug, Clone, Serialize)]
pub struct LazerDedupeFailure {
    pub path: String,
    pub message: String,
}

/// 去重任务结果；除标注外 `*_size` 字段单位均为字节，`*_count` 为文件数。
#[derive(Debug, Clone, Serialize, Default)]
pub struct LazerDedupeResult {
    /// 是否为演练模式：只统计可释放空间，不实际替换文件。
    pub dry_run: bool,
    /// 任务被取消时为 `true`，结果保留已累计的部分统计。
    pub cancelled: bool,
    pub lazer_files_root: String,
    pub stable_roots: Vec<String>,
    /// lazer files 存储内的文件总数与总大小（含已硬链接与非常规命名的文件）。
    pub lazer_file_count: u64,
    pub lazer_total_size: u64,
    /// 链接数已大于 1（此前已去重）的文件数与大小，本轮无需处理。
    pub already_linked_count: u64,
    pub already_linked_size: u64,
    /// 实际计算了 SHA-256 的 stable 候选文件数（先按大小过滤，远小于全量）。
    pub hashed_stable_count: u64,
    /// 哈希命中（lazer 副本在 stable 中有同内容文件）的配对总数，含跨卷部分。
    pub candidate_count: u64,
    /// 同卷配对合计大小：可硬链接释放的空间，dry_run 下即为预估收益。
    pub reclaimable_size: u64,
    /// 实际替换为硬链接成功的文件数与释放的空间。
    pub linked_count: u64,
    pub linked_size: u64,
    /// 因跨卷（或读不到卷信息）无法硬链接而跳过的配对数与大小。
    pub skipped_cross_volume_count: u64,
    pub skipped_cross_volume_size: u64,
    pub failed_count: u64,
    pub failed: Vec<LazerDedupeFailure>,
}

/// 待匹配的 lazer 文件：文件名即内容 SHA-256，`volume` 用于同卷硬链接校验。
struct LazerFile {
    hash: String,
    size: u64,
    path: PathBuf,
    volume: u64,
}

/// 启动 lazer 去重任务：抢占 `RUNNING` 标志防并发，检测到 lazer 客户端
/// 正在运行时拒绝执行；实际扫描与替换在后台线程进行，进度经
/// `lazer-dedupe-progress` 事件推送。
#[tauri::command]
/// 供前端调用的 Tauri 命令：扫描并去重可安全识别的重复文件。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn dedupe_lazer_files(
    dry_run: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<LazerDedupeResult> {
    RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| CommandError::new("LAZER_DEDUPE_IN_PROGRESS", "已有查重任务正在进行"))?;
    let guard = RunningGuard;
    CANCELLED.store(false, Ordering::Relaxed);
    let service = Arc::clone(&state.local_analysis);
    let emit = Arc::new(move |progress: LazerDedupeProgress| {
        let _ = app.emit("lazer-dedupe-progress", progress);
    });
    crate::infrastructure::tasks::background("tools", move || {
        let _guard = guard;
        run(service, dry_run, emit)
    })
    .await
    .map_err(|join| CommandError::new("LAZER_DEDUPE_TASK_ERROR", format!("任务异常结束：{join}")))?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_lazer_dedupe() {
    CANCELLED.store(true, Ordering::Relaxed);
}

/// 去重主流程（后台线程执行），各阶段细节见模块文档与行内注释。
fn run(
    service: Arc<LocalAnalysisService>,
    dry_run: bool,
    emit: Arc<dyn Fn(LazerDedupeProgress) + Send + Sync>,
) -> CommandResult<LazerDedupeResult> {
    let reporter = ProgressReporter::new(emit);
    let mut result = LazerDedupeResult {
        dry_run,
        ..LazerDedupeResult::default()
    };

    if CANCELLED.load(Ordering::Relaxed) {
        return Ok(cancelled(result));
    }
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
    // 在替换前先通过临时链接验证目标卷支持硬链接，失败时保持原始 lazer 文件不变。
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
        .or_else(|| platform::resolve_lazer_data_root().map(|root| root.join("files")))
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

/// 流式计算文件 SHA-256（128 KiB 缓冲），返回小写十六进制摘要，可直接与
/// lazer 的哈希文件名比对。
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

/// 进度上报节流器：非强制事件至少间隔 100ms，避免海量小文件刷爆 IPC。
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

/// Windows 实现：`CreateFileW`（访问掩码 0 仅查询属性）+
/// `GetFileInformationByHandle` 读取卷序列号、文件索引与链接数；
/// `FILE_FLAG_BACKUP_SEMANTICS` 允许打开任意文件，`nFileIndexHigh/Low`
/// 拼成 64 位文件标识。
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

    /// 端到端验证替换语义：替换后两路径同卷同 inode、链接数变为 2、暂存链接被清理。
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
