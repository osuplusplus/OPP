use super::{models::*, queue::Navigation};
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::global,
    state::AppState,
};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

pub(crate) enum Action {
    Control(Control),
    Queue(Vec<Track>, bool, bool, usize, u64),
    InitializeQueue(Vec<Track>, u64),
    Shutdown,
}
struct Request {
    action: Action,
    reply: Option<oneshot::Sender<CommandResult<()>>>,
}

pub struct MusicRuntime {
    sender: Mutex<Option<mpsc::Sender<Request>>>,
    pub state: Arc<Mutex<PlayerState>>,
    pub(crate) queue: Arc<RwLock<Vec<Track>>>,
    pub(crate) session: Mutex<WindowSession>,
    pub(crate) switching: Mutex<Option<String>>,
    pub(crate) directory: PathBuf,
    pub(crate) intent: Arc<AtomicU64>,
    persistence: Arc<Mutex<()>>,
}

impl MusicRuntime {
    pub async fn initialize_queue(
        &self,
        local: Arc<crate::features::local_analysis::LocalAnalysisService>,
    ) {
        if self.snapshot().map_or(true, |state| state.total > 0) {
            return;
        }
        let intent = self.intent.load(Ordering::Relaxed);
        let result = tauri::async_runtime::spawn_blocking(move || {
            let candidates = local.music_candidates(None)?;
            let (mut tracks, _) =
                super::catalog::build_queue(candidates, &QueueRequest::default(), None);
            use rand_core::{OsRng, RngCore};
            for index in (1..tracks.len()).rev() {
                tracks.swap(index, (OsRng.next_u64() % (index as u64 + 1)) as usize);
            }
            Ok::<_, CommandError>(tracks)
        })
        .await;
        match result {
            Ok(Ok(tracks)) if !tracks.is_empty() => {
                if let Err(error) = self.dispatch(Action::InitializeQueue(tracks, intent)).await {
                    crate::log_warn!("music_player", "生成启动播放列表失败：{}", error.message);
                }
            }
            Ok(Err(error)) => {
                crate::log_warn!("music_player", "生成启动播放列表失败：{}", error.message)
            }
            Err(error) => crate::log_warn!("music_player", "生成启动播放列表失败：{}", error),
            _ => {}
        }
    }

    pub fn new(root: &Path) -> CommandResult<Self> {
        let directory = root.join("music-player");
        fs::create_dir_all(&directory)?;
        let mut state: PlayerState = read_json(&directory.join("state.json")).unwrap_or_default();
        let queue: Vec<Track> = read_json(&directory.join("queue.json")).unwrap_or_default();
        state.playing = false;
        state.mini = false;
        state.notice = None;
        state.background_tasks = 0;
        state.volume = if state.volume.is_finite() {
            state.volume.clamp(0.0, 1.0)
        } else {
            0.65
        };
        state.position = if state.position.is_finite() {
            state.position.max(0.0)
        } else {
            0.0
        };
        state.total = queue.len();
        if !queue
            .iter()
            .any(|t| Some(&t.info.id) == state.current.as_ref().map(|c| &c.id))
        {
            state.current = queue.first().map(|t| t.info.clone());
            state.position = 0.0;
        }
        Ok(Self {
            sender: Mutex::new(None),
            state: Arc::new(Mutex::new(state)),
            queue: Arc::new(RwLock::new(queue)),
            session: Mutex::new(read_json(&directory.join("window.json")).unwrap_or_default()),
            switching: Mutex::new(None),
            directory,
            intent: Arc::new(AtomicU64::new(0)),
            persistence: Arc::new(Mutex::new(())),
        })
    }
    pub fn start(&self, app: AppHandle, hwnd: Option<usize>) {
        let (tx, rx) = mpsc::channel();
        *self.sender.lock().unwrap() = Some(tx.clone());
        let state = self.state.clone();
        let queue = self.queue.clone();
        let directory = self.directory.clone();
        let intent = self.intent.clone();
        let persistence = self.persistence.clone();
        thread::Builder::new()
            .name("opp-music".into())
            .spawn(move || {
                let current = state.lock().unwrap().clone();
                let mut worker = Worker {
                    app: app.clone(),
                    state: current,
                    shared: state,
                    queue,
                    directory,
                    stream: None,
                    sink: None,
                    navigation: Navigation::default(),
                    media: None,
                    last_save: Instant::now(),
                    last_publish: Instant::now(),
                    intent: intent.clone(),
                    persistence,
                };
                worker.media = super::media::attach(app, hwnd, move |action| {
                    if !matches!(
                        action,
                        Control::Volume { .. } | Control::Mode { .. } | Control::Seek { .. }
                    ) {
                        intent.fetch_add(1, Ordering::Relaxed);
                    }
                    let _ = tx.send(Request {
                        action: Action::Control(action),
                        reply: None,
                    });
                });
                loop {
                    match rx.recv_timeout(Duration::from_millis(500)) {
                        Ok(Request {
                            action: Action::Shutdown,
                            reply,
                        }) => {
                            worker.update_position();
                            worker.persist(false);
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                            break;
                        }
                        Ok(request) => {
                            let result = worker.apply(request.action);
                            if let Err(error) = &result {
                                worker.state.notice = Some(error.message.clone());
                                crate::log_error!("music_player", "{}", error.message);
                            }
                            worker.publish(true);
                            worker.persist(false);
                            if let Some(reply) = request.reply {
                                let _ = reply.send(result);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    worker.tick();
                }
            })
            .expect("music worker thread");
    }
    pub(crate) async fn dispatch(&self, action: Action) -> CommandResult<()> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .lock()
            .map_err(|_| failure("播放服务不可用"))?
            .as_ref()
            .ok_or_else(|| failure("播放服务尚未启动"))?
            .send(Request {
                action,
                reply: Some(reply),
            })
            .map_err(|_| failure("播放服务已停止"))?;
        receive.await.map_err(|_| failure("播放服务已停止"))?
    }
    pub fn shutdown(&self) {
        if let Ok(sender) = self.sender.lock()
            && let Some(sender) = sender.as_ref()
        {
            let _ = sender.send(Request {
                action: Action::Shutdown,
                reply: None,
            });
        }
        // Never wait for the worker on the GUI thread: it may be awaiting a window operation.
        if let Ok(snapshot) = self.snapshot()
            && let Ok(_guard) = self.persistence.lock()
        {
            let _ = write_json(&self.directory.join("state.json"), &snapshot);
        }
        if let Ok(session) = self.session.lock() {
            let _ = write_json(&self.directory.join("window.json"), &*session);
        }
    }
    pub fn snapshot(&self) -> CommandResult<PlayerState> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| failure("播放状态不可用"))
    }
    pub fn page(&self, offset: usize, limit: usize, search: &str) -> CommandResult<QueuePage> {
        let queue = self.queue.read().map_err(|_| failure("播放队列不可用"))?;
        let needle = search.trim().to_lowercase();
        let matched: Vec<_> = queue
            .iter()
            .filter(|t| {
                needle.is_empty()
                    || format!("{} {}", t.info.title, t.info.artist)
                        .to_lowercase()
                        .contains(&needle)
            })
            .collect();
        Ok(QueuePage {
            items: matched
                .iter()
                .skip(offset)
                .take(limit.clamp(1, 100))
                .map(|t| t.info.clone())
                .collect(),
            total: matched.len(),
            version: self.snapshot()?.queue_version,
        })
    }
}

pub(crate) fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("MUSIC_PLAYER_ERROR", message)
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let span = global().map(|l| l.operation("music_player", "read_state"));
    let result: CommandResult<Option<T>> = (|| {
        let result = fs::read(path);
        if result
            .as_ref()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(None);
        }
        if let Some(s) = &span {
            s.fs_op("read", path, &result);
        }
        Ok(Some(serde_json::from_slice(&result?)?))
    })();
    crate::infrastructure::logging::finish_span(span, result)
        .ok()
        .flatten()
}
pub(crate) fn write_json(path: &Path, data: &impl serde::Serialize) -> CommandResult<()> {
    let span = global().map(|l| l.operation("music_player", "save_state"));
    let result = (|| {
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec(data)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    })();
    if let Some(s) = &span {
        s.fs_op("write", path, &result);
    }
    crate::infrastructure::logging::finish_span(span, result)
}

struct Worker {
    app: AppHandle,
    state: PlayerState,
    shared: Arc<Mutex<PlayerState>>,
    queue: Arc<RwLock<Vec<Track>>>,
    directory: PathBuf,
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    navigation: Navigation,
    media: Option<souvlaki::MediaControls>,
    last_save: Instant,
    last_publish: Instant,
    intent: Arc<AtomicU64>,
    persistence: Arc<Mutex<()>>,
}
impl Worker {
    fn ids(&self) -> Vec<String> {
        self.queue
            .read()
            .unwrap()
            .iter()
            .map(|t| t.info.id.clone())
            .collect()
    }
    fn update_position(&mut self) {
        if let Some(sink) = &self.sink {
            self.state.position = sink.get_pos().as_secs_f64();
        }
    }
    fn start_track(&mut self, id: &str, preview: bool, resume: f64) -> CommandResult<()> {
        let track = self
            .queue
            .read()
            .unwrap()
            .iter()
            .find(|t| t.info.id == id)
            .cloned()
            .ok_or_else(|| failure("歌曲已不在队列中"))?;
        self.sink.take();
        self.state.playing = false;
        self.state.current = Some(track.info.clone());
        self.state.position = 0.0;
        self.state.duration = 0.0;
        self.state.resource_id = None;
        if self.stream.is_none() {
            self.stream = Some(OutputStreamBuilder::open_default_stream().map_err(|e| {
                CommandError::new("MUSIC_OUTPUT_UNAVAILABLE", format!("无法打开音频设备：{e}"))
            })?);
        }
        for asset in &track.assets {
            let span = global().map(|l| l.operation("music_player", "open_audio"));
            let result = asset.checked_path(&asset.audio).and_then(|path| {
                let file = fs::File::open(&path);
                if let Some(s) = &span {
                    s.fs_op("read", &path, &file);
                }
                Decoder::try_from(file?).map_err(|e| failure(e.to_string()))
            });
            let decoder = match crate::infrastructure::logging::finish_span(span, result) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let duration = decoder
                .total_duration()
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
            sink.pause();
            sink.set_volume(self.state.volume);
            sink.append(decoder);
            let position = if preview {
                asset.preview_seconds()
            } else {
                resume
            };
            if position > 0.0
                && (duration == 0.0 || position < duration)
                && let Err(error) = sink.try_seek(Duration::from_secs_f64(position))
            {
                crate::log_warn!("music_player", "无法跳转：{}", error);
            }
            sink.play();
            self.sink = Some(sink);
            self.state.duration = duration;
            self.state.resource_id = Some(asset.resource_id.clone());
            self.state.playing = true;
            self.update_position();
            super::media::metadata(&mut self.media, &track.info, duration);
            return Ok(());
        }
        Err(failure(format!(
            "无法播放「{}」，正在尝试下一首",
            track.info.title
        )))
    }
    fn play_with_fallback(
        &mut self,
        first: String,
        preview: bool,
        resume: f64,
    ) -> CommandResult<()> {
        let ids = self.ids();
        let mut tried = std::collections::HashSet::new();
        let mut target = Some(first);
        self.state.notice = None;
        while let Some(id) = target {
            if !tried.insert(id.clone()) {
                break;
            }
            match self.start_track(
                &id,
                preview && tried.len() == 1,
                if tried.len() == 1 { resume } else { 0.0 },
            ) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if error.code == "MUSIC_OUTPUT_UNAVAILABLE" {
                        return Err(error);
                    }
                    self.state.notice = Some(error.message.clone());
                    crate::log_warn!("music_player", "{}", error.message);
                }
            }
            // Failure traversal must not obey repeat-one or revisit failed songs.
            let index = ids.iter().position(|v| v == &id).unwrap_or(0);
            target = (1..=ids.len())
                .map(|step| ids[(index + step) % ids.len()].clone())
                .find(|v| !tried.contains(v));
        }
        self.state.playing = false;
        self.stream.take();
        Err(failure("队列中没有可播放的歌曲，请检查本地文件或音频设备"))
    }
    fn apply(&mut self, action: Action) -> CommandResult<()> {
        match action {
            Action::InitializeQueue(tracks, intent) => {
                if intent != self.intent.load(Ordering::Relaxed)
                    || !self.queue.read().unwrap().is_empty()
                {
                    return Ok(());
                }
                self.state.total = tracks.len();
                self.state.current = tracks.first().map(|track| track.info.clone());
                self.state.position = 0.0;
                self.state.duration = 0.0;
                self.state.resource_id = None;
                self.state.queue_version += 1;
                *self.queue.write().unwrap() = tracks;
                self.persist(true);
            }
            Action::Queue(tracks, append, preview, missing, intent) => {
                if intent != self.intent.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let first = tracks.first().map(|t| t.info.id.clone());
                if first.is_none() {
                    return Err(failure(if missing > 0 {
                        format!("收藏夹有 {missing} 项尚未在本地找到可播放音频")
                    } else {
                        "没有可播放的本地歌曲，请先扫描谱库".into()
                    }));
                }
                {
                    let mut queue = self.queue.write().unwrap();
                    if !append {
                        queue.clear();
                    }
                    let mut positions: std::collections::HashMap<_, _> = queue
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (t.info.id.clone(), i))
                        .collect();
                    for track in tracks {
                        if let Some(index) = positions.get(&track.info.id) {
                            queue[*index] = track;
                        } else {
                            positions.insert(track.info.id.clone(), queue.len());
                            queue.push(track);
                        }
                    }
                    self.state.total = queue.len();
                }
                self.navigation.reset();
                self.state.queue_version += 1;
                let playback = if !append || preview || self.state.current.is_none() {
                    self.play_with_fallback(first.unwrap(), preview, 0.0)
                } else {
                    Ok(())
                };
                self.persist(true);
                playback?;
                if missing > 0 {
                    self.state.notice = Some(format!("已跳过 {missing} 项本地缺失歌曲"));
                }
            }
            Action::Control(control) => self.control(control)?,
            Action::Shutdown => {}
        }
        Ok(())
    }
    fn control(&mut self, control: Control) -> CommandResult<()> {
        self.update_position();
        match control {
            Control::Play | Control::Toggle if !self.state.playing => {
                if let Some(sink) = &self.sink
                    && !sink.empty()
                {
                    sink.play();
                    self.state.playing = true;
                } else if let Some(id) = self
                    .state
                    .current
                    .as_ref()
                    .map(|c| c.id.clone())
                    .or_else(|| self.ids().first().cloned())
                {
                    self.play_with_fallback(id, false, self.state.position)?;
                }
            }
            Control::Pause | Control::Toggle => {
                if let Some(sink) = &self.sink {
                    sink.pause();
                }
                self.state.playing = false;
            }
            Control::Play => {}
            Control::Next => self.advance(false)?,
            Control::Previous => {
                let ids = self.ids();
                let current = self.state.current.as_ref().map(|t| t.id.as_str());
                if let Some(id) = self.navigation.previous(&ids, current) {
                    self.play_with_fallback(id, false, 0.0)?;
                }
            }
            Control::Select { id } => {
                self.navigation.reset();
                self.play_with_fallback(id, false, 0.0)?;
            }
            Control::Seek { seconds } => {
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err(failure("播放位置无效"));
                }
                let seconds = if self.state.duration > 0.0 {
                    seconds.min(self.state.duration)
                } else {
                    seconds
                };
                if let Some(sink) = &self.sink {
                    sink.try_seek(Duration::from_secs_f64(seconds))
                        .map_err(|e| failure(format!("无法跳转：{e}")))?;
                }
                self.state.position = seconds;
            }
            Control::Volume { value } => {
                if !value.is_finite() {
                    return Err(failure("音量无效"));
                }
                self.state.volume = value.clamp(0.0, 1.0);
                if let Some(sink) = &self.sink {
                    sink.set_volume(self.state.volume);
                }
            }
            Control::Mode { mode } => {
                self.state.mode = mode;
                self.navigation.reset();
            }
            Control::Remove { id } => {
                let ids = self.ids();
                let index = ids.iter().position(|v| v == &id).unwrap_or(0);
                self.queue.write().unwrap().retain(|t| t.info.id != id);
                self.state.queue_version += 1;
                self.state.total = self.queue.read().unwrap().len();
                self.navigation.reset();
                self.persist(true);
                if self.state.current.as_ref().is_some_and(|t| t.id == id) {
                    let playing = self.state.playing;
                    self.sink.take();
                    self.state.playing = false;
                    self.state.position = 0.0;
                    let next = self
                        .queue
                        .read()
                        .unwrap()
                        .get(index.min(self.state.total.saturating_sub(1)))
                        .map(|t| t.info.clone());
                    self.state.current = next.clone();
                    self.state.resource_id = None;
                    self.state.duration = 0.0;
                    if playing && let Some(next) = next {
                        self.play_with_fallback(next.id, false, 0.0)?;
                    }
                }
            }
            Control::Clear => {
                self.sink.take();
                self.stream.take();
                self.queue.write().unwrap().clear();
                self.navigation.reset();
                self.state.current = None;
                self.state.resource_id = None;
                self.state.total = 0;
                self.state.position = 0.0;
                self.state.duration = 0.0;
                self.state.playing = false;
                self.state.notice = None;
                self.state.queue_version += 1;
                self.persist(true);
            }
        }
        Ok(())
    }
    fn advance(&mut self, ended: bool) -> CommandResult<()> {
        let ids = self.ids();
        let next = self.navigation.next(
            &ids,
            self.state.current.as_ref().map(|t| t.id.as_str()),
            self.state.mode,
            ended,
        );
        if let Some(next) = next {
            self.play_with_fallback(next, false, 0.0)
        } else {
            self.state.playing = false;
            self.sink.take();
            self.stream.take();
            self.state.position = 0.0;
            Ok(())
        }
    }
    fn tick(&mut self) {
        let app = self.app.state::<AppState>();
        let mini = app.music.snapshot().map(|s| s.mini).unwrap_or(false);
        let tasks = super::windows::background_tasks(&app);
        let changed = self.state.mini != mini || self.state.background_tasks != tasks;
        self.state.mini = mini;
        self.state.background_tasks = tasks;
        if mini && tasks == 0 {
            app.local_analysis.release_music_idle_indexes();
        }
        super::windows::release_completed_window(&self.app);
        if self.state.playing {
            self.update_position();
            if self.sink.as_ref().is_some_and(Sink::empty) {
                if let Err(e) = self.advance(true) {
                    self.state.notice = Some(e.message);
                }
                self.publish(true);
            } else {
                self.publish(false);
            }
            if self.last_save.elapsed() >= Duration::from_secs(10) {
                self.persist(false);
            }
        } else if changed {
            self.publish(true);
        }
    }
    fn publish(&mut self, force: bool) {
        if !force && self.last_publish.elapsed() < Duration::from_millis(500) {
            return;
        }
        // The window coordinator owns mini; avoid overwriting a transition with an old worker snapshot.
        {
            let mut shared = self.shared.lock().unwrap();
            self.state.version = self.state.version.max(shared.version) + 1;
            self.state.mini = shared.mini;
            *shared = self.state.clone();
        }
        let visible = self
            .app
            .webview_windows()
            .values()
            .any(|w| w.is_visible().unwrap_or(false));
        if force || visible {
            let _ = self.app.emit("music-player-state", &self.state);
        }
        if force {
            super::media::playback(&mut self.media, &self.state);
        }
        self.last_publish = Instant::now();
    }
    fn persist(&mut self, queue: bool) {
        let _guard = self.persistence.lock().unwrap();
        let result = if queue {
            write_json(
                &self.directory.join("queue.json"),
                &*self.queue.read().unwrap(),
            )
        } else {
            write_json(&self.directory.join("state.json"), &self.state)
        };
        if let Err(error) = result {
            crate::log_warn!("music_player", "保存播放状态失败：{}", error.message);
        }
        self.last_save = Instant::now();
    }
}
