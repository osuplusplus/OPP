//! 播放服务：`MusicRuntime` 对外提供状态快照与动作分发，
//! 实际播放由独立的 `Worker` 线程串行处理，驱动 `rodio` 音频输出、
//! 状态发布（前端事件 + 系统媒体面板）与 JSON 持久化。
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

/// 发送给工作线程的动作；所有动作在工作线程内串行执行。
pub(crate) enum Action {
    /// 播放控制（来自前端或系统媒体键）。
    Control(Control),
    /// 设置队列：曲目、是否追加、是否从预览点播放、收藏夹缺失数、`intent` 版本。
    Queue(Vec<Track>, bool, bool, usize, u64),
    /// 启动时随机初始化队列；仅当队列仍为空且 `intent` 未变时生效。
    InitializeQueue(Vec<Track>, u64),
    /// 停止工作线程（保存进度后退出）。
    Shutdown,
}
/// 一次动作请求；携带 `reply` 时工作线程处理完会回传结果（`dispatch` 使用）。
struct Request {
    action: Action,
    reply: Option<oneshot::Sender<CommandResult<()>>>,
}

/// 音乐播放运行时：由 `AppState` 持有，向命令层暴露状态与分发接口。
pub struct MusicRuntime {
    /// 发往工作线程的请求通道；`None` 表示线程未启动或已退出。
    sender: Mutex<Option<mpsc::Sender<Request>>>,
    /// 与命令层共享的状态快照，由工作线程发布时写入。
    pub state: Arc<Mutex<PlayerState>>,
    /// 播放队列；读多写少故用 `RwLock`。
    pub(crate) queue: Arc<RwLock<Vec<Track>>>,
    /// 窗口会话（几何/路由/置顶），随窗口切换持久化。
    pub(crate) session: Mutex<WindowSession>,
    /// 进行中的窗口切换令牌，防止并发切换。
    pub(crate) switching: Mutex<Option<String>>,
    /// 持久化目录 `<应用数据>/music-player`。
    pub(crate) directory: PathBuf,
    /// 用户操作版本号：新的队列操作会使先前异步构建的队列结果作废。
    pub(crate) intent: Arc<AtomicU64>,
    /// 持久化互斥锁：让 `shutdown` 的紧急写盘与工作线程的常规写盘互斥。
    persistence: Arc<Mutex<()>>,
}

impl MusicRuntime {
    /// 应用启动时若没有任何曲目，在后台扫描全部候选并随机洗牌生成启动队列。
    /// 先记录 `intent`，若用户在此期间手动操作，该结果送达时会被丢弃。
    pub async fn initialize_queue(
        &self,
        local: Arc<crate::features::local_analysis::LocalAnalysisService>,
    ) {
        if self.snapshot().map_or(true, |state| state.total > 0) {
            return;
        }
        let intent = self.intent.load(Ordering::Relaxed);
        let result =
            crate::infrastructure::tasks::background("initialize_music_queue", move || {
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

    /// 从 `<root>/music-player` 恢复持久化的状态、队列与窗口会话。
    ///
    /// 易失字段（播放中/迷你模式/提示/后台任务）一律重置；音量与进度会剔除
    /// 非有限值并钳制到合法范围；当前曲目不在恢复的队列中时回退到队首。
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
    /// 启动工作线程并接入系统媒体控制。
    ///
    /// 媒体键回调中音量/模式/跳转类指令不递增 `intent`：
    /// 它们不会替换队列，无需让进行中的队列构建作废。
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
                // 主循环：500ms 超时轮询，即使没有请求也周期执行 tick（进度/发布/持久化）。
                loop {
                    match rx.recv_timeout(Duration::from_millis(500)) {
                        // 停机：保存进度后退出循环，线程随之结束。
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
    /// 把动作发给工作线程并等待处理结果。
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
    /// 请求工作线程停机，并立即把状态与会话落盘（不等线程退出）。
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
    /// 读取当前状态快照。
    pub fn snapshot(&self) -> CommandResult<PlayerState> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| failure("播放状态不可用"))
    }
    /// 按“标题 + 艺人”不区分大小写的子串过滤并分页返回队列，每页至多 100 条。
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

/// 构造错误码为 `MUSIC_PLAYER_ERROR` 的播放器错误。
pub(crate) fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("MUSIC_PLAYER_ERROR", message)
}
/// 读取并反序列化 JSON；文件缺失或内容损坏都返回 `None`（回退默认值），
/// 保证损坏的持久化文件不会阻止应用启动。
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
/// 原子写入 JSON：先写同名 `.tmp` 文件再重命名，避免崩溃留下半截文件。
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

/// 工作线程的全量状态：串行执行所有播放动作，独占音频输出与媒体控制句柄。
struct Worker {
    app: AppHandle,
    /// 工作线程本地的状态副本，发布时合并回 `shared`。
    state: PlayerState,
    /// 与命令层共享的状态（`snapshot` 的数据源）。
    shared: Arc<Mutex<PlayerState>>,
    queue: Arc<RwLock<Vec<Track>>>,
    directory: PathBuf,
    /// rodio 输出流；全部曲目失败或清空队列时释放。
    stream: Option<OutputStream>,
    /// 当前曲目的播放 Sink；`empty()` 表示已播完。
    sink: Option<Sink>,
    /// 队列导航（前进历史 + 洗牌袋）。
    navigation: Navigation,
    /// 系统媒体控制句柄（注册失败则为 `None`）。
    media: Option<souvlaki::MediaControls>,
    /// 上次持久化与上次发布的时间，分别用于 10 秒存盘与 500ms 发布节流。
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
    /// 从 `Sink` 读取实际播放进度，同步到本地状态。
    fn update_position(&mut self) {
        if let Some(sink) = &self.sink {
            self.state.position = sink.get_pos().as_secs_f64();
        }
    }
    /// 播放指定曲目：按 `assets` 的优先级依次尝试各客户端音频，
    /// 文件缺失或解码失败则跳过下一个；首个可用资源决定时长与 `resource_id`。
    /// `preview` 为真时从音频预览点开始，否则从 `resume` 秒处续播。
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
        // 输出流懒打开并全程复用，仅在全部失败或清空队列时释放。
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
            // 时长未知或定位点未超时长时才尝试跳到预览点/续播位置。
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
    /// 播放指定曲目，失败时（文件损坏、格式不支持）沿队列顺延尝试其余曲目。
    ///
    /// 输出设备不可用（`MUSIC_OUTPUT_UNAVAILABLE`）时立即上抛——换曲无济于事；
    /// 一圈下来无可播曲目时释放输出流并停止播放。
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
    /// 工作线程的动作入口：初始化队列、设置队列或转发控制指令。
    fn apply(&mut self, action: Action) -> CommandResult<()> {
        match action {
            Action::InitializeQueue(tracks, intent) => {
                // 用户已手动操作（intent 变化）或队列非空：丢弃过期的启动队列。
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
                    // 追加时按曲目 ID 原位更新，避免同一曲目重复入队。
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
                // 覆盖队列、预览请求或当前无曲目时立即开播；纯追加不打断正在播放的曲子。
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
    /// 播放控制状态机；入口先同步真实进度，让指令基于最新状态执行。
    fn control(&mut self, control: Control) -> CommandResult<()> {
        self.update_position();
        match control {
            // 恢复播放：Sink 未耗尽直接续播，否则从保存的进度重播当前曲。
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
            // 删除当前曲目时停播并顺延到原位置的后一首；删其他曲目不打断播放。
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
    /// 切到下一首：`ended` 为真表示自然播完（受单曲循环影响）；
    /// 顺序模式到队尾无下一首时停止播放并释放输出流。
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
    /// 周期任务（约每 500ms）：同步迷你标志与后台任务数、回收已完成任务的
    /// 隐藏主窗口、推进进度并在曲目自然播完时切歌、按 10 秒节流保存进度。
    fn tick(&mut self) {
        let app = self.app.state::<AppState>();
        let mini = app.music.snapshot().map(|s| s.mini).unwrap_or(false);
        let tasks = super::windows::background_tasks(&app);
        let changed = self.state.mini != mini || self.state.background_tasks != tasks;
        self.state.mini = mini;
        self.state.background_tasks = tasks;
        // 迷你模式且已无后台任务：释放仅完整模式需要的空闲索引。
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
    /// 把状态发布给前端（事件）与系统媒体面板；常规发布 500ms 节流，
    /// `force` 时立即发送并同步媒体面板；仅在存在可见窗口或强制时才发事件。
    fn publish(&mut self, force: bool) {
        if !force && self.last_publish.elapsed() < Duration::from_millis(500) {
            return;
        }
        // The window coordinator owns mini; avoid overwriting a transition with an old worker snapshot.
        {
            let mut shared = self.shared.lock().unwrap();
            // 版本号对齐共享快照后再自增，保证与窗口协调器的写入合流后仍单调递增。
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
    /// 保存 `state.json`；`queue` 为真时改存 `queue.json`。失败仅记日志。
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
