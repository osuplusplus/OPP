mod download;
mod models;
pub(crate) mod providers;
mod tools;

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    account::ensure_access_token,
    error::{CommandError, CommandResult},
    state::AppState,
};

use download::{download_file_name, download_with_adapters};
use models::{
    BeatmapDownloadFailure, BeatmapDownloadProgress, BeatmapDownloadRequest, BeatmapDownloadResult,
    CollectedBeatmapsets, DownloadProgressCounts, OnlineBeatmapSearchQuery,
};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tools::{
    MAX_BATCH_ITEMS, MAX_COLLECT_RESULTS, annotate_source, emit_progress, find_existing_beatmapset,
    prepare_destination, progress_for_item, search_with_adapters,
};
use uuid::Uuid;

#[tauri::command]
/// 供前端调用的 Tauri 命令：搜索远程资源。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn search_online_beatmapsets(
    query: OnlineBeatmapSearchQuery,
    state: State<'_, AppState>,
) -> CommandResult<Value> {
    search_with_adapters(&query, &state).await
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：收集远程资源的候选结果。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn collect_online_beatmapsets(
    mut query: OnlineBeatmapSearchQuery,
    limit: usize,
    state: State<'_, AppState>,
) -> CommandResult<CollectedBeatmapsets> {
    let limit = limit.clamp(1, MAX_COLLECT_RESULTS);
    query.cursor_string = None;
    let mut items = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_cursors = HashSet::new();
    let mut available_total = None;

    loop {
        let response = search_with_adapters(&query, &state).await?;
        available_total = available_total.or_else(|| response.get("total").and_then(Value::as_u64));
        let page_items = response
            .get("beatmapsets")
            .and_then(Value::as_array)
            .ok_or_else(|| CommandError::new("INVALID_DATA", "osu! 搜索响应缺少 beatmapsets"))?;

        for item in page_items {
            let Some(id) = item.get("id").and_then(Value::as_u64) else {
                continue;
            };
            if seen_ids.insert(id) {
                items.push(item.clone());
            }
            if items.len() >= limit {
                break;
            }
        }
        if items.len() >= limit {
            break;
        }

        let Some(cursor) = response
            .get("cursor_string")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|cursor| !cursor.is_empty())
        else {
            break;
        };
        if !seen_cursors.insert(cursor.clone()) {
            break;
        }
        query.cursor_string = Some(cursor);
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let truncated = available_total.is_some_and(|total| items.len() < total as usize);
    Ok(CollectedBeatmapsets {
        items,
        available_total,
        truncated,
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_online_beatmapset(
    beatmapset_id: u64,
    state: State<'_, AppState>,
) -> CommandResult<Value> {
    let access_token = ensure_access_token(&state).await?;
    let mut value = state
        .api
        .get_beatmapset(&access_token, beatmapset_id)
        .await?;
    annotate_source(&mut value, "official");
    Ok(value)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_online_beatmap(
    beatmap_id: u64,
    state: State<'_, AppState>,
) -> CommandResult<Value> {
    let mut value = state.providers.nerinyan_beatmap(beatmap_id).await?;
    annotate_source(&mut value, "nerinyan");
    Ok(value)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_online_beatmap_provider_status(
    state: State<'_, AppState>,
) -> CommandResult<Vec<providers::ProviderStatus>> {
    Ok(state.providers.statuses().await)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：下载所选资源。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn download_online_beatmapsets(
    app: AppHandle,
    request: BeatmapDownloadRequest,
) -> CommandResult<BeatmapDownloadResult> {
    if request.items.is_empty() {
        return Err(CommandError::new("EMPTY_DOWNLOAD_QUEUE", "下载队列为空"));
    }
    if request.items.len() > MAX_BATCH_ITEMS {
        return Err(CommandError::new(
            "DOWNLOAD_LIMIT_EXCEEDED",
            format!("单次最多下载 {MAX_BATCH_ITEMS} 个谱面集"),
        ));
    }

    let state = app.state::<AppState>();
    let destination = prepare_destination(&request.destination)?;
    let mut unique_ids = HashSet::new();
    let items = request
        .items
        .into_iter()
        .filter(|item| unique_ids.insert(item.beatmapset_id))
        .collect::<Vec<_>>();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut runtime = state
            .beatmap_download
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "下载队列状态锁已损坏"))?;
        if runtime.is_some() {
            return Err(CommandError::new(
                "DOWNLOAD_ALREADY_RUNNING",
                "已有一个批量下载任务正在运行",
            ));
        }
        *runtime = Some(cancel.clone());
    }

    let total = items.len();
    let mut completed = 0;
    let mut completed_paths = Vec::<PathBuf>::new();
    let mut skipped = 0;
    let mut failures = Vec::new();
    emit_progress(
        &app,
        BeatmapDownloadProgress {
            phase: "started".into(),
            total,
            processed: 0,
            completed,
            skipped,
            failed: 0,
            current_beatmapset_id: None,
            current_title: None,
            message: Some(format!("准备下载 {total} 个谱面集")),
            downloaded_bytes: 0,
            total_bytes: None,
            bytes_per_second: 0.0,
            completed_paths: None,
            destination: None,
        },
    );

    for (index, item) in items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let processed = index;
        if !request.overwrite
            && let Some(existing) = find_existing_beatmapset(&destination, item.beatmapset_id)
        {
            skipped += 1;
            // Callers that post-process archives (for example collection
            // completion) also need paths for files that were already present.
            completed_paths.push(existing);
            emit_progress(
                &app,
                progress_for_item(
                    "skipped",
                    DownloadProgressCounts {
                        total,
                        processed: processed + 1,
                        completed,
                        skipped,
                        failed: failures.len(),
                    },
                    item,
                    Some("目标目录中已存在该谱面集".into()),
                ),
            );
            continue;
        }

        emit_progress(
            &app,
            progress_for_item(
                "downloading",
                DownloadProgressCounts {
                    total,
                    processed,
                    completed,
                    skipped,
                    failed: failures.len(),
                },
                item,
                None,
            ),
        );

        let started_at = Instant::now();
        let mut last_progress_emit = started_at - Duration::from_secs(1);
        let mut last_speed_at = started_at;
        let mut last_speed_bytes = 0;
        let mut smoothed_speed: Option<f64> = None;
        match download_with_adapters(
            &state,
            item.beatmapset_id,
            &request.provider,
            request.include_video,
            cancel.as_ref(),
            |downloaded_bytes, total_bytes| {
                let now = Instant::now();
                if now.duration_since(last_progress_emit) < Duration::from_millis(100)
                    && total_bytes != Some(downloaded_bytes)
                {
                    return;
                }
                last_progress_emit = now;
                let interval_seconds = now.duration_since(last_speed_at).as_secs_f64().max(0.001);
                let interval_bytes = if downloaded_bytes >= last_speed_bytes {
                    downloaded_bytes - last_speed_bytes
                } else {
                    downloaded_bytes
                };
                last_speed_at = now;
                last_speed_bytes = downloaded_bytes;
                let mut byte_progress = progress_for_item(
                    "downloading",
                    DownloadProgressCounts {
                        total,
                        processed,
                        completed,
                        skipped,
                        failed: failures.len(),
                    },
                    item,
                    Some("正在接收下载数据".into()),
                );
                byte_progress.downloaded_bytes = downloaded_bytes;
                byte_progress.total_bytes = total_bytes;
                let instantaneous_speed = interval_bytes as f64 / interval_seconds;
                let speed = smoothed_speed
                    .map(|previous| previous * 0.72 + instantaneous_speed * 0.28)
                    .unwrap_or(instantaneous_speed);
                smoothed_speed = Some(speed);
                byte_progress.bytes_per_second = speed;
                emit_progress(&app, byte_progress);
            },
        )
        .await
        {
            Ok(download) if cancel.load(Ordering::Relaxed) => {
                let _ = download;
                break;
            }
            Ok(download) => {
                let downloaded_bytes = download.bytes.len() as u64;
                let mut byte_progress = progress_for_item(
                    "downloading",
                    DownloadProgressCounts {
                        total,
                        processed,
                        completed,
                        skipped,
                        failed: failures.len(),
                    },
                    item,
                    Some("正在写入下载文件".into()),
                );
                byte_progress.downloaded_bytes = downloaded_bytes;
                byte_progress.total_bytes = Some(downloaded_bytes);
                byte_progress.bytes_per_second = smoothed_speed.unwrap_or_else(|| {
                    downloaded_bytes as f64 / started_at.elapsed().as_secs_f64().max(0.001)
                });
                emit_progress(&app, byte_progress);
                let file_name = download_file_name(item, download.suggested_filename.as_deref());
                let target = destination.join(file_name);
                let temporary = destination.join(format!(
                    ".opp-{}-{}.part",
                    item.beatmapset_id,
                    Uuid::new_v4().simple()
                ));
                let write_result = async {
                    tokio::fs::write(&temporary, download.bytes).await?;
                    if request.overwrite && target.exists() {
                        tokio::fs::remove_file(&target).await?;
                    }
                    tokio::fs::rename(&temporary, &target).await
                }
                .await;

                match write_result {
                    Ok(()) => {
                        completed += 1;
                        completed_paths.push(target.clone());
                        emit_progress(
                            &app,
                            progress_for_item(
                                "completed",
                                DownloadProgressCounts {
                                    total,
                                    processed: processed + 1,
                                    completed,
                                    skipped,
                                    failed: failures.len(),
                                },
                                item,
                                Some(target.to_string_lossy().into_owned()),
                            ),
                        );
                    }
                    Err(error) => {
                        let _ = tokio::fs::remove_file(&temporary).await;
                        failures.push(BeatmapDownloadFailure {
                            beatmapset_id: item.beatmapset_id,
                            title: item.title.clone(),
                            message: error.to_string(),
                        });
                        emit_progress(
                            &app,
                            progress_for_item(
                                "failed",
                                DownloadProgressCounts {
                                    total,
                                    processed: processed + 1,
                                    completed,
                                    skipped,
                                    failed: failures.len(),
                                },
                                item,
                                Some(error.to_string()),
                            ),
                        );
                    }
                }
            }
            Err(error) => {
                let terminal_error = matches!(error.code.as_str(), "PERMISSION_DENIED");
                failures.push(BeatmapDownloadFailure {
                    beatmapset_id: item.beatmapset_id,
                    title: item.title.clone(),
                    message: error.message.clone(),
                });
                emit_progress(
                    &app,
                    progress_for_item(
                        "failed",
                        DownloadProgressCounts {
                            total,
                            processed: processed + 1,
                            completed,
                            skipped,
                            failed: failures.len(),
                        },
                        item,
                        Some(error.message),
                    ),
                );
                if terminal_error {
                    break;
                }
            }
        }

        if index + 1 < total && !cancel.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    let cancelled = cancel.load(Ordering::Relaxed);
    let result = BeatmapDownloadResult {
        destination: destination.to_string_lossy().into_owned(),
        total,
        completed,
        skipped,
        failed: failures.len(),
        cancelled,
        failures,
        completed_paths: completed_paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    };
    emit_progress(
        &app,
        BeatmapDownloadProgress {
            phase: if cancelled { "cancelled" } else { "finished" }.into(),
            total,
            processed: completed + skipped + result.failed,
            completed,
            skipped,
            failed: result.failed,
            current_beatmapset_id: None,
            current_title: None,
            message: Some(if cancelled {
                "下载任务已取消".into()
            } else {
                format!(
                    "下载完成：成功 {}，跳过 {}，失败 {}",
                    completed, skipped, result.failed
                )
            }),
            downloaded_bytes: 0,
            total_bytes: None,
            bytes_per_second: 0.0,
            completed_paths: Some(
                completed_paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            ),
            destination: Some(result.destination.clone()),
        },
    );

    let open_after_download = request.open_after_download.unwrap_or_else(|| {
        state
            .store
            .snapshot()
            .is_ok_and(|saved| saved.settings.open_downloaded_beatmaps_after_download)
    });
    if open_after_download {
        tokio::time::sleep(Duration::from_millis(500)).await;
        for path in completed_paths {
            let _ = app.opener().open_path(path.to_string_lossy(), None::<&str>);
        }
    }

    if let Ok(mut runtime) = state.beatmap_download.lock()
        && runtime
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &cancel))
    {
        *runtime = None;
    }
    Ok(result)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn open_downloaded_path(app: AppHandle, path: String) -> CommandResult<()> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(CommandError::new(
            "DOWNLOAD_PATH_MISSING",
            "Downloaded file is no longer available",
        ));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| CommandError::new("OPEN_DOWNLOAD_FAILED", error.to_string()))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_online_beatmap_download(state: State<'_, AppState>) -> CommandResult<()> {
    let runtime = state
        .beatmap_download
        .lock()
        .map_err(|_| CommandError::new("STATE_ERROR", "下载队列状态锁已损坏"))?;
    let Some(cancel) = runtime.as_ref() else {
        return Err(CommandError::new(
            "DOWNLOAD_NOT_RUNNING",
            "当前没有批量下载任务",
        ));
    };
    cancel.store(true, Ordering::Relaxed);
    Ok(())
}
