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

#[tauri::command(async)]
pub fn music_state(state: State<'_, AppState>) -> CommandResult<PlayerState> {
    finish_span(
        global().map(|l| l.operation("music_player", "music_state")),
        state.music.snapshot(),
    )
}
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

#[tauri::command]
pub fn music_frontend_task(active: bool, state: State<'_, AppState>) {
    let span = global().map(|l| l.operation("music_player", "music_frontend_task"));
    state
        .music_frontend_task
        .store(active, std::sync::atomic::Ordering::Relaxed);
    crate::infrastructure::logging::finish_span_ok(span, ());
}
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
