//! 音乐播放器的 Tauri 命令层：只做参数反序列化、状态访问、服务调用与错误转换。
use super::{
    models::*,
    service::{Action, failure},
};
use crate::{
    error::CommandResult,
    infrastructure::logging::{finish_span, global},
    state::AppState,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::Cursor;
use tauri::State;

/// 读取播放器状态快照。
#[tauri::command(async)]
pub fn music_state(state: State<'_, AppState>) -> CommandResult<PlayerState> {
    finish_span(
        global().map(|l| l.operation("music_player", "music_state")),
        state.music.snapshot(),
    )
}
/// 返回当前播放资源对应的本地谱面定位（客户端/规则/谱集键），供前端跳转谱面页；
/// `resource_id` 不是正在播放的资源时返回 `None`。
#[tauri::command(async)]
pub fn music_location(
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<MusicLocation>> {
    let span = global().map(|l| l.operation("music_player", "music_location"));
    let result = (|| {
        let snapshot = state.music.snapshot()?;
        if snapshot.resource_id.as_deref() != Some(resource_id.as_str()) {
            return Ok(None);
        }
        let queue = state
            .music
            .queue
            .read()
            .map_err(|_| failure("播放队列不可用"))?;
        let Some(asset) = queue
            .iter()
            .find(|track| {
                Some(track.info.id.as_str())
                    == snapshot.current.as_ref().map(|track| track.id.as_str())
            })
            .and_then(|track| {
                track
                    .assets
                    .iter()
                    .find(|asset| asset.resource_id == resource_id)
            })
        else {
            return Ok(None);
        };
        Ok(state
            .local_analysis
            .music_location(asset.client, &resource_id)?
            .map(|(set_key, ruleset)| MusicLocation {
                client: asset.client,
                ruleset,
                set_key,
                resource_id,
            }))
    })();
    finish_span(span, result)
}
/// 分页查询播放队列，按标题与艺人过滤。
#[tauri::command(async)]
pub fn music_queue_page(
    offset: usize,
    limit: usize,
    search: String,
    state: State<'_, AppState>,
) -> CommandResult<QueuePage> {
    finish_span(
        global().map(|l| l.operation("music_player", "music_queue_page")),
        state.music.page(offset, limit, &search),
    )
}
/// 播放控制；非音量/模式/跳转类指令先递增 `intent`，使进行中的异步队列构建作废。
#[tauri::command]
pub async fn music_control(control: Control, state: State<'_, AppState>) -> CommandResult<()> {
    let span = global().map(|l| l.operation("music_player", "music_control"));
    if !matches!(
        control,
        Control::Volume { .. } | Control::Mode { .. } | Control::Seek { .. }
    ) {
        state
            .music
            .intent
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    finish_span(span, state.music.dispatch(Action::Control(control)).await)
}
/// 构建并应用新队列：在交互线程池扫描候选（可按收藏夹过滤）后交给工作线程。
/// `intent` 在构建前先取号，若期间用户又发起新操作，旧结果送达时会被丢弃。
#[tauri::command]
pub async fn music_set_queue(
    request: QueueRequest,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let span = global().map(|l| l.operation("music_player", "music_set_queue"));
    let local = state.local_analysis.clone();
    let collections = state.collections.clone();
    let intent = state
        .music
        .intent
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1;
    let result = async {
        let append = request.append;
        let preview = request.preview;
        let (tracks, missing) =
            crate::infrastructure::tasks::interactive("music_player", move || {
                let collection = request
                    .collection_id
                    .as_ref()
                    .map(|id| collections.folder(id))
                    .transpose()?;
                let candidates = local.music_candidates(request.query.as_ref())?;
                Ok::<_, crate::error::CommandError>(super::catalog::build_queue(
                    candidates,
                    &request,
                    collection.as_ref().map(|c| c.entries.as_slice()),
                ))
            })
            .await
            .map_err(|e| failure(e.to_string()))??;
        state
            .music
            .dispatch(Action::Queue(tracks, append, preview, missing, intent))
            .await
    }
    .await;
    finish_span(span, result)
}
/// 获取曲目封面：解码第一个可用封面，缩略到 160px 并输出 base64 PNG data URL；
/// 解码内存上限 64MB，任一步失败只是跳过该封面或返回 `None`。
#[tauri::command(async)]
pub async fn music_artwork(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let queue = state.music.queue.clone();
    crate::infrastructure::tasks::interactive("music_artwork", move || {
        let span = global().map(|l| l.operation("music_player", "music_artwork"));
        let result = (|| {
            let assets = queue
                .read()
                .map_err(|_| failure("播放队列不可用"))?
                .iter()
                .find(|t| t.info.id == id)
                .map(|t| t.assets.clone())
                .unwrap_or_default();
            for asset in assets {
                let Some(path) = &asset.artwork else { continue };
                let Ok(path) = asset.checked_path(path) else {
                    continue;
                };
                let reader = image::ImageReader::open(&path);
                if let Some(s) = &span {
                    s.fs_op("read", &path, &reader);
                }
                let Ok(reader) = reader else { continue };
                let Ok(mut reader) = reader.with_guessed_format() else {
                    continue;
                };
                let mut limits = image::Limits::default();
                limits.max_alloc = Some(64 * 1024 * 1024);
                reader.limits(limits);
                let Ok(image) = reader.decode() else { continue };
                let mut bytes = Cursor::new(Vec::new());
                image
                    .thumbnail(160, 160)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(|e| failure(e.to_string()))?;
                return Ok(Some(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(bytes.into_inner())
                )));
            }
            Ok(None)
        })();
        finish_span(span, result)
    })
    .await?
}

/// 切换迷你/主窗口；切回主窗口前先在后台线程加载缓存索引，再执行窗口操作。
#[tauri::command]
pub async fn music_window_mode(
    mini: bool,
    route: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let span = global().map(|l| l.operation("music_player", "music_window_mode"));
    if !mini {
        let local = state.local_analysis.clone();
        crate::infrastructure::tasks::background("load_cached_indexes", move || {
            local.load_cached_indexes()
        })
        .await
        .map_err(|e| failure(e.to_string()))?;
    }
    finish_span(span, super::windows::switch(&app, mini, route))
}
/// 新窗口前端就绪回调：完成切换收尾（换窗/状态更新），返回窗口会话。
#[tauri::command(async)]
pub fn music_window_ready(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> CommandResult<WindowSession> {
    finish_span(
        global().map(|l| l.operation("music_player", "music_window_ready")),
        super::windows::ready(&app, window.label()),
    )
}

/// 前端上报是否有任务在跑；迷你模式下据此决定何时回收隐藏的主窗口。
#[tauri::command]
pub fn music_frontend_task(active: bool, state: State<'_, AppState>) {
    let span = global().map(|l| l.operation("music_player", "music_frontend_task"));
    state
        .music_frontend_task
        .store(active, std::sync::atomic::Ordering::Relaxed);
    crate::infrastructure::logging::finish_span_ok(span, ());
}
/// 迷你播放器展开/收起与置顶切换：按 72/420 逻辑像素约束高度并持久化置顶状态。
#[tauri::command(async)]
pub fn music_mini_layout(
    expanded: bool,
    pinned: bool,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let span = global().map(|l| l.operation("music_player", "music_mini_layout"));
    let result = (|| {
        if window.label() != "mini-player" {
            return Err(failure("该操作仅用于迷你播放器"));
        }
        let position = window.outer_position()?;
        super::windows::constrain(
            &window,
            Some(position.x),
            Some(position.y),
            if expanded { 420.0 } else { 72.0 },
        )?;
        window.set_always_on_top(pinned)?;
        let mut session = state.music.session.lock().unwrap();
        session.pinned = Some(pinned);
        super::service::write_json(&state.music.directory.join("window.json"), &*session)
    })();
    finish_span(span, result)
}
