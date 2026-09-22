//! Bounded parallel downloads. Completion IDs, rather than queue positions, drive reconciliation.
use super::{
    download::{download_file_name, download_with_adapters, validate_osz},
    models::{
        BeatmapDownloadFailure, BeatmapDownloadItem, BeatmapDownloadRequest, BeatmapDownloadResult,
        DownloadProgressCounts,
    },
    tools::{emit_progress, find_existing_beatmapset, progress_for_item},
};
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::{finish_span, global},
    state::AppState,
};
use futures_util::{Stream, StreamExt, stream};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::AppHandle;
use uuid::Uuid;

const DOWNLOAD_CONCURRENCY: usize = 2;

fn concurrent_jobs<I, F>(jobs: I) -> impl Stream<Item = F::Output>
where
    I: IntoIterator<Item = F>,
    F: std::future::Future,
{
    stream::iter(jobs).buffer_unordered(DOWNLOAD_CONCURRENCY)
}

pub async fn run(
    app: &AppHandle,
    state: &AppState,
    items: &[BeatmapDownloadItem],
    request: &BeatmapDownloadRequest,
    destination: &Path,
    cancel: &AtomicBool,
) -> BeatmapDownloadResult {
    let counts = Mutex::new(DownloadProgressCounts {
        total: items.len(),
        processed: 0,
        completed: 0,
        skipped: 0,
        failed: 0,
    });
    let mut result = BeatmapDownloadResult {
        destination: destination.to_string_lossy().into_owned(),
        total: items.len(),
        completed: 0,
        skipped: 0,
        failed: 0,
        cancelled: false,
        failures: vec![],
        completed_paths: vec![],
        succeeded_beatmapset_ids: vec![],
    };
    let jobs: Vec<_> = items
        .iter()
        .map(|item| {
            let counts = &counts;
            async move {
                let span = global().map(|logger| {
                    logger.operation(
                        "online_beatmaps",
                        format!("download_item:{}", item.beatmapset_id),
                    )
                });
                let outcome =
                    download_one(app, state, item, request, destination, cancel, counts).await;
                (item, finish_span(span, outcome))
            }
        })
        .collect();
    let results = concurrent_jobs(jobs);
    futures_util::pin_mut!(results);
    while let Some((item, outcome)) = results.next().await {
        let (phase, message) = match outcome {
            Ok((path, skipped)) => {
                if skipped {
                    result.skipped += 1;
                } else {
                    result.completed += 1;
                }
                result.succeeded_beatmapset_ids.push(item.beatmapset_id);
                result
                    .completed_paths
                    .push(path.to_string_lossy().into_owned());
                (
                    if skipped { "skipped" } else { "completed" },
                    path.to_string_lossy().into_owned(),
                )
            }
            Err(error) if error.code == "DOWNLOAD_CANCELLED" => continue,
            Err(error) => {
                result.failed += 1;
                result.failures.push(BeatmapDownloadFailure {
                    beatmapset_id: item.beatmapset_id,
                    title: item.title.clone(),
                    message: error.message.clone(),
                });
                ("failed", error.message)
            }
        };
        let updated = DownloadProgressCounts {
            total: result.total,
            processed: result.completed + result.skipped + result.failed,
            completed: result.completed,
            skipped: result.skipped,
            failed: result.failed,
        };
        *counts.lock().expect("download progress lock") = updated.clone();
        emit_progress(app, progress_for_item(phase, updated, item, Some(message)));
    }
    result.cancelled = cancel.load(Ordering::Relaxed);
    result
}

fn check_cancel(cancel: &AtomicBool) -> CommandResult<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(CommandError::new("DOWNLOAD_CANCELLED", "下载已取消"))
    } else {
        Ok(())
    }
}

async fn download_one(
    app: &AppHandle,
    state: &AppState,
    item: &BeatmapDownloadItem,
    request: &BeatmapDownloadRequest,
    destination: &Path,
    cancel: &AtomicBool,
    counts: &Mutex<DownloadProgressCounts>,
) -> CommandResult<(PathBuf, bool)> {
    check_cancel(cancel)?;
    let existing = (!request.overwrite)
        .then(|| find_existing_beatmapset(destination, item.beatmapset_id))
        .flatten();
    let stale_existing = existing
        .as_ref()
        .filter(|path| {
            std::fs::File::open(path)
                .ok()
                .is_none_or(|file| validate_osz(file, item).is_err())
        })
        .cloned();
    if let Some(existing) = existing.filter(|_| stale_existing.is_none()) {
        return Ok((existing, true));
    }

    let started = Instant::now();
    let mut last_emit = started - Duration::from_secs(1);
    let mut last_bytes = 0;
    let mut speed = 0.0;
    let download = download_with_adapters(
        state,
        item,
        &request.provider,
        request.include_video,
        cancel,
        |bytes, total| {
            let now = Instant::now();
            if bytes != 0
                && now.duration_since(last_emit) < Duration::from_millis(100)
                && total != Some(bytes)
            {
                return;
            }
            if bytes == 0 {
                speed = 0.0;
            } else {
                let instantaneous = bytes.saturating_sub(last_bytes) as f64
                    / now.duration_since(last_emit).as_secs_f64().max(0.001);
                speed = if speed == 0.0 {
                    instantaneous
                } else {
                    speed * 0.72 + instantaneous * 0.28
                };
            }
            last_emit = now;
            last_bytes = bytes;
            let mut progress = progress_for_item(
                "downloading",
                counts.lock().expect("download progress lock").clone(),
                item,
                Some("正在下载曲包（最多同时 2 个）".into()),
            );
            progress.downloaded_bytes = bytes;
            progress.total_bytes = total;
            progress.bytes_per_second = speed;
            emit_progress(app, progress);
        },
    )
    .await?;
    check_cancel(cancel)?;
    let target = destination.join(download_file_name(
        item,
        download.suggested_filename.as_deref(),
    ));
    let temporary = destination.join(format!(
        ".opp-{}-{}.part",
        item.beatmapset_id,
        Uuid::new_v4().simple()
    ));
    let write_result: std::io::Result<()> = async {
        tokio::fs::write(&temporary, download.bytes).await?;
        // Preserve the previous archive until the new validated file has been installed.
        let old = stale_existing
            .as_ref()
            .or_else(|| target.exists().then_some(&target));
        let backup = destination.join(format!(
            ".opp-{}-{}.osz.stale",
            item.beatmapset_id,
            Uuid::new_v4().simple()
        ));
        if let Some(old) = old {
            tokio::fs::rename(old, &backup).await?;
        }
        match tokio::fs::rename(&temporary, &target).await {
            Ok(()) => Ok(()),
            Err(error) => {
                if let Some(old) = old {
                    let _ = tokio::fs::rename(&backup, old).await;
                }
                Err(error)
            }
        }
    }
    .await;
    if let Some(logger) = global() {
        logger
            .operation("online_beatmaps", "save_archive")
            .fs_op("write", &target, &write_result);
    }
    if let Err(error) = write_result {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(CommandError::new(
            "DOWNLOAD_WRITE_FAILED",
            error.to_string(),
        ));
    }
    Ok((target, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicUsize};

    #[tokio::test]
    async fn slow_first_job_does_not_block_following_jobs_and_concurrency_is_bounded() {
        let gate = Arc::new(tokio::sync::Notify::new());
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let jobs = (0..5).map(|id| {
            let gate = gate.clone();
            let active = &active;
            let peak = &peak;
            async move {
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(count, Ordering::SeqCst);
                if id == 0 {
                    gate.notified().await;
                }
                tokio::task::yield_now().await;
                active.fetch_sub(1, Ordering::SeqCst);
                id
            }
        });
        let results = concurrent_jobs(jobs);
        futures_util::pin_mut!(results);
        for id in 1..5 {
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), results.next())
                    .await
                    .unwrap(),
                Some(id)
            );
        }
        gate.notify_one();
        assert_eq!(results.next().await, Some(0));
        assert_eq!(peak.load(Ordering::SeqCst), DOWNLOAD_CONCURRENCY);
    }
}
