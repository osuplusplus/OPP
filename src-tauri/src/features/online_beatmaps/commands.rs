//! 在线谱面模块的 Tauri 命令层：搜索、收集、详情、背景图、镜像状态、批量下载与取消。

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{
    error::{CommandError, CommandResult},
    features::account::ensure_access_token,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

use super::models::{
    BeatmapDownloadProgress, BeatmapDownloadRequest, BeatmapDownloadResult, CollectedBeatmapsets,
    OnlineBeatmapSearchQuery,
};
use super::providers;
use super::tools::{
    MAX_BATCH_ITEMS, MAX_COLLECT_RESULTS, annotate_source, emit_progress, prepare_destination,
    search_with_adapters,
};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// 搜索始终走 osu! 官方 API；镜像只参与下载，不参与搜索。
#[tauri::command]
/// 供前端调用的 Tauri 命令：搜索远程资源。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn search_online_beatmapsets(
    query: OnlineBeatmapSearchQuery,
    state: State<'_, AppState>,
) -> CommandResult<Value> {
    let span = global().map(|logger| logger.operation("online_beatmaps", "search_beatmapsets"));

    if let Some(ref s) = span {
        s.info(
            "搜索在线谱面",
            Some(serde_json::json!({
                "query": query.query,
                "title": query.title,
                "title_unicode": query.title_unicode,
                "artist": query.artist,
                "status": query.status,
            })),
        );
    }

    let result = search_with_adapters(&query, &state).await;
    finish_span(span, result)
}

/// 按官方游标翻页收集去重结果，页间停留 150ms 以降低限流风险。
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
    // 谱面集 ID 去重；游标去重用于在 API 重复返回同一游标时终止翻页。
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

/// 读取官方谱面集详情（需登录令牌）。
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
/// 读取当前谱面集的原始背景。结果会持久缓存，只为舞台上已稳定选中的谱面请求远端。
pub async fn get_online_beatmap_background(
    beatmapset_id: u64,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    state.online_artwork.load_or_fetch(beatmapset_id).await
}

/// 难度（beatmap）级元数据来自 Nerinyan 镜像。
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

/// 返回各镜像的健康状态与能力矩阵，供下载源选择器展示。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_online_beatmap_provider_status(
    state: State<'_, AppState>,
) -> CommandResult<Vec<providers::ProviderStatus>> {
    Ok(state.providers.statuses().await)
}

/// 批量下载入口：队列先去重，再由 `batch::run` 以受限并发执行，
/// 全程经 `beatmap-download-progress` 事件汇报进度；同一时刻仅允许一个批量任务。
#[tauri::command]
/// 供前端调用的 Tauri 命令：下载所选资源。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn download_online_beatmapsets(
    app: AppHandle,
    request: BeatmapDownloadRequest,
) -> CommandResult<BeatmapDownloadResult> {
    let mut span = global().map(|logger| logger.operation("online_beatmaps", "download_batch"));

    if let Some(ref s) = span {
        s.info(
            format!("开始批量下载 {} 个谱面", request.items.len()),
            Some(serde_json::json!({
                "count": request.items.len(),
                "provider": request.provider,
                "include_video": request.include_video,
                "overwrite": request.overwrite,
            })),
        );
    }

    if request.items.is_empty() {
        let error = CommandError::new("EMPTY_DOWNLOAD_QUEUE", "下载队列为空");
        if let Some(ref mut s) = span {
            s.finish_error(&error);
        }
        return Err(error);
    }
    if request.items.len() > MAX_BATCH_ITEMS {
        let error = CommandError::new(
            "DOWNLOAD_LIMIT_EXCEEDED",
            format!("单次最多下载 {MAX_BATCH_ITEMS} 个谱面集"),
        );
        if let Some(ref mut s) = span {
            s.finish_error(&error);
        }
        return Err(error);
    }

    let state = app.state::<AppState>();
    let destination = prepare_destination(&request.destination)?;
    // 同一谱面集只保留第一条，避免重复下载与重复计数。
    let mut unique_ids = HashSet::new();
    let items = request
        .items
        .iter()
        .filter(|item| unique_ids.insert(item.beatmapset_id))
        .cloned()
        .collect::<Vec<_>>();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        // 注册取消句柄并做单例互斥：已有批量任务在运行时直接拒绝。
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
    emit_progress(
        &app,
        BeatmapDownloadProgress {
            phase: "started".into(),
            total,
            processed: 0,
            completed: 0,
            skipped: 0,
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

    let result = super::batch::run(&app, &state, &items, &request, &destination, &cancel).await;
    let completed = result.completed;
    let skipped = result.skipped;
    let cancelled = result.cancelled;
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
            completed_paths: Some(result.completed_paths.clone()),
            destination: Some(result.destination.clone()),
        },
    );

    // 请求未显式指定时，回落到用户设置中的“下载后自动打开”。
    let open_after_download = request.open_after_download.unwrap_or_else(|| {
        state
            .store
            .snapshot()
            .is_ok_and(|saved| saved.settings.open_downloaded_beatmaps_after_download)
    });
    if open_after_download {
        tokio::time::sleep(Duration::from_millis(500)).await;
        for path in &result.completed_paths {
            let _ = open_downloaded_path(app.clone(), path.clone());
        }
    }

    // 仅当槽内仍是本次任务的句柄时才清空，避免误清后续任务。
    if let Ok(mut runtime) = state.beatmap_download.lock()
        && runtime
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &cancel))
    {
        *runtime = None;
    }

    if let Some(ref mut s) = span {
        s.finish_ok(Some(serde_json::json!({
            "total": result.total,
            "completed": result.completed,
            "skipped": result.skipped,
            "failed": result.failed,
            "cancelled": result.cancelled,
        })));
    }

    Ok(result)
}

/// 用系统默认程序打开下载产物；文件已不存在时返回可操作的错误提示。
#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn open_downloaded_path(app: AppHandle, path: String) -> CommandResult<()> {
    let span = global().map(|logger| logger.operation("online_beatmaps", "open_downloaded_path"));
    let path = PathBuf::from(path);
    if !path.exists() {
        return finish_span(
            span,
            Err(CommandError::new(
                "DOWNLOAD_PATH_MISSING",
                "下载文件已被移动或删除，请重新下载或检查保存目录",
            )),
        );
    }
    let result = app
        .opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| CommandError::new("OPEN_DOWNLOAD_FAILED", error.to_string()));
    finish_span(span, result)
}

/// 仅置位取消标志；实际停止是协作式的，由下载循环在检查点感知并退出。
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
