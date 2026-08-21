//! 实时回放预览,双后端:
//!
//! - **Windows(高帧率模式)**:`#[cfg(windows)]`wgpu 直接渲染到窗口 surface
//! - **其他平台(canvas 模式)**
//!

use crate::error::{CommandError, CommandResult};
use osu_replay_render::{build_atlas, draw, game, render::Renderer, scene};
use std::sync::mpsc::{Sender, TryRecvError, channel};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use osu_replay_render::surface;

#[cfg(windows)]
use windows_sys::Win32::Foundation::POINT;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_HWNDPARENT,
    GetWindowRect, HTTRANSPARENT, RegisterClassW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_NCHITTEST, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

/// 内部渲染分辨率(16:9,canvas/原生窗口均由 CSS 等比缩放)。
const RENDER_W: u32 = 1280;
const RENDER_H: u32 = 720;
const CLEAR: [f64; 4] = [0.055, 0.055, 0.075, 1.0];
/// canvas 模式:超过该时长没有前端拉帧(页面隐藏/切走)则暂停 GPU
/// 渲染,音频照常。
#[cfg(not(windows))]
const FRAME_PULL_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct PreviewRect {
    /// CSS 像素,相对 WebView 视口左上角(仅 Windows 原生模式使用)。
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 前端检测到应用内弹窗(对话框/确认框)打开时置 true:原生预览窗口
    /// 压在 WebView 之上,会盖住弹窗,需临时隐藏,弹窗关闭后恢复。
    #[serde(default)]
    pub suppressed: bool,
}

/// 实时预览渲染参数(对齐 osu-replay-render CLI 的选项语义,见其 README)。
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveOptions {
    /// UR 显示(UR 条:刻度/中心标记/均值箭头/判定色轴/UR 数值)整体
    /// 一个开关,默认开。
    pub ur_bar: bool,
    /// 按键输入展示(右下角 Z/X/C 键位计数,Argon key overlay),默认开。
    pub key_overlay: bool,
    /// 物件之间的引导线(follow points),默认开。
    pub follow_points: bool,
    /// 绘制谱面背景图([Events] 0,0,...,全屏铺满)。
    pub bg: bool,
    /// 背景不透明度 0..1,默认 0.3 = 1 - DimLevel 0.7。
    pub bg_opacity: f32,
    /// 播放 BGM([General] AudioFilename,相对谱面目录)。
    pub audio: bool,
    /// BGM 对齐偏移 ms,默认 0(音频位置 = 回放时间 − 偏移,
    /// 界面里用户可自行调整)。
    pub audio_offset: f64,
}

impl Default for LiveOptions {
    fn default() -> Self {
        Self {
            ur_bar: true,
            key_overlay: true,
            follow_points: true,
            bg: false,
            bg_opacity: 0.3,
            audio: true,
            audio_offset: 0.0,
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct LiveRenderState {
    pub active: bool,
    pub playing: bool,
    pub time_ms: f64,
    pub duration_ms: f64,
}

/// open 的返回:总时长与实际使用的后端模式。
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOpenInfo {
    pub duration_ms: f64,
    /// "native"(Windows 直渲) | "canvas"(离屏 + IPC)。
    pub mode: &'static str,
}

#[derive(Debug)]
enum Cmd {
    Open {
        app: AppHandle,
        beatmap_path: String,
        replay_path: String,
        /// 仅 Windows 原生模式使用;其他平台忽略。
        rect: PreviewRect,
        scale: f64,
        options: LiveOptions,
        reply: Sender<Result<LiveOpenInfo, String>>,
    },
    Move {
        rect: PreviewRect,
        scale: f64,
    },
    /// 渲染参数原地生效(零重载):即时字段直接改 SceneState;
    /// bg 重建图集并热替换纹理;audio 懒加载/静音。
    SetOptions(LiveOptions),
    Seek(f64),
    Play,
    Pause,
    Close,
}

static CHANNEL: LazyLock<Mutex<Sender<Cmd>>> = LazyLock::new(|| {
    let (tx, rx) = channel::<Cmd>();
    std::thread::Builder::new()
        .name("live-render".into())
        .spawn(move || worker(rx))
        .expect("spawn live-render thread");
    Mutex::new(tx)
});

/// canvas 模式的最新一帧(RGBA tight;空 = 尚无帧)。原生模式不使用。
static LATEST_FRAME: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(Vec::new()));
#[cfg(not(windows))]
static LAST_PULL: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));

fn send(cmd: Cmd) {
    let _ = CHANNEL.lock().unwrap().send(cmd);
}

// ---- Windows:原生子窗口(高帧率直渲) ----------------------------------------

#[cfg(windows)]
mod native {
    use super::*;

    static CLASS_REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // 点击穿透:HTTRANSPARENT 让鼠标消息落到下层的 WebView。
        // (实测 WS_EX_LAYERED 在本机会让 WS_CHILD 创建直接失败,
        // HTTRANSPARENT 是无样式依赖的等效方案;见 create_child 注释。)
        if msg == WM_NCHITTEST {
            return HTTRANSPARENT as LRESULT;
        }
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }

    unsafe fn register_class() -> Result<(), String> {
        if CLASS_REGISTERED.get().is_some() {
            return Ok(());
        }
        let class_name: Vec<u16> = "OPPLiveRender\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_CLASS_ALREADY_EXISTS {
                let _ = CLASS_REGISTERED.set(());
                return Ok(());
            }
            return Err(format!("注册窗口类失败 (GetLastError={err})"));
        }
        let _ = CLASS_REGISTERED.set(());
        Ok(())
    }

    unsafe fn create_child(parent: isize, x: i32, y: i32, w: i32, h: i32) -> Result<isize, String> {
        unsafe { register_class() }?;
        let name: Vec<u16> = "OPPLiveRender\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                // 注意:不加 WS_EX_LAYERED——独立测试证明它在 Win11 26200
                // 上会让 WS_CHILD 的 CreateWindowExW 直接失败(err 0/6);
                // 穿透由 wnd_proc 的 WM_NCHITTEST→HTTRANSPARENT 实现。
                (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE) as WINDOW_EX_STYLE,
                name.as_ptr(),
                std::ptr::null(),
                (WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS) as WINDOW_STYLE,
                x,
                y,
                w.max(1),
                h.max(1),
                parent as HWND,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!(
                "无法创建预览窗口 (GetLastError={err}, parent=0x{parent:X})"
            ));
        }
        Ok(hwnd as isize)
    }

    /// WS_CHILD 子窗口必须与父窗口同线程:创建在 Tauri 主线程执行,
    /// 渲染线程只持有 HWND(present 不依赖创建线程)。
    pub fn create_child_on_main(
        app: &AppHandle,
        parent: isize,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Result<isize, String> {
        let (tx, rx) = channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(unsafe { create_child(parent, x, y, w, h) });
        })
        .map_err(|e| format!("无法调度到主线程: {e}"))?;
        rx.recv().map_err(|_| "主线程无响应".to_string())?
    }

    /// DestroyWindow 只能由创建线程调用。
    pub fn destroy_child_on_main(app: &AppHandle, hwnd: isize) {
        let _ = app.run_on_main_thread(move || unsafe {
            DestroyWindow(hwnd as HWND);
        });
    }

    /// 有属主的顶层弹出窗口:顶层窗口可在任意线程创建(无子窗口的
    /// 线程亲和规则),owner 关系保证它始终盖在主窗口上方且随主窗口
    /// 最小化/还原。坐标为屏幕像素。
    pub fn create_popup(x: i32, y: i32, w: i32, h: i32) -> Result<isize, String> {
        unsafe { register_class() }?;
        let name: Vec<u16> = "OPPLiveRender\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TOOLWINDOW)
                    as WINDOW_EX_STYLE,
                name.as_ptr(),
                std::ptr::null(),
                (WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS) as WINDOW_STYLE,
                x,
                y,
                w.max(1),
                h.max(1),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!("无法创建预览弹出窗口 (GetLastError={err})"));
        }
        Ok(hwnd as isize)
    }

    /// 顶层窗口的 owner(保持 z-order 在主窗口之上)。
    pub fn set_owner(hwnd: isize, owner: isize) {
        unsafe {
            SetWindowLongPtrW(hwnd as HWND, GWLP_HWNDPARENT, owner as _);
        }
    }

    /// 主窗口客户区坐标 → 屏幕坐标。
    pub fn client_origin_on_screen(main_hwnd: isize) -> (i32, i32) {
        let mut pt = POINT { x: 0, y: 0 };
        unsafe { ClientToScreen(main_hwnd as HWND, &mut pt) };
        (pt.x, pt.y)
    }

    pub fn window_rect(hwnd: isize) -> (i32, i32, i32, i32) {
        let mut rect: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
        unsafe { GetWindowRect(hwnd as HWND, &mut rect) };
        (rect.left, rect.top, rect.right, rect.bottom)
    }

    /// 定位/显隐。SetWindowPos 可跨线程调用(同步消息)。定位同时压到
    /// 兄弟窗口栈顶(HWND_TOP):WebView2 与本窗口同为主窗口的子窗口,
    /// 激活/重排时会把我们盖下去,必须持续保持在它之上。
    pub fn place(hwnd: isize, x: i32, y: i32, w: i32, h: i32, visible: bool) {
        unsafe {
            if visible {
                // hWndInsertAfter = HWND_TOP(0):置于兄弟顶层。
                SetWindowPos(
                    hwnd as HWND,
                    std::ptr::null_mut(),
                    x,
                    y,
                    w.max(1),
                    h.max(1),
                    SWP_NOACTIVATE,
                );
                ShowWindow(hwnd as HWND, SW_SHOW);
            } else {
                ShowWindow(hwnd as HWND, SW_HIDE);
            }
        }
    }

    /// 只调整 Z 序到顶层(尺寸位置不动),供渲染循环周期性调用,
    /// 对抗 WebView2 被激活后重新抬高自己的行为。
    pub fn bring_to_top(hwnd: isize) {
        unsafe {
            SetWindowPos(
                hwnd as HWND,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

// ---- 音频(rodio,跨平台) ----------------------------------------------------

struct AudioClip {
    samples: std::sync::Arc<Vec<f32>>,
    rate: u32,
    channels: u16,
}

/// 从任意起点流式播放的样本源。
struct ClipSource {
    samples: std::sync::Arc<Vec<f32>>,
    pos: usize,
    rate: u32,
    channels: u16,
}

impl Iterator for ClipSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let v = self.samples.get(self.pos).copied();
        if v.is_some() {
            self.pos += 1;
        }
        v
    }
}

impl rodio::Source for ClipSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some((self.samples.len() - self.pos) / self.channels.max(1) as usize)
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs_f64(
            self.samples.len() as f64 / self.channels.max(1) as f64 / self.rate as f64,
        ))
    }
}

fn load_audio(path: &std::path::Path) -> Option<AudioClip> {
    use rodio::Source as _;
    let file = std::fs::File::open(path).ok()?;
    let decoder = rodio::Decoder::new(std::io::BufReader::new(file)).ok()?;
    let rate = decoder.sample_rate();
    let channels = decoder.channels();
    let samples: Vec<f32> = decoder.convert_samples().collect();
    if samples.is_empty() {
        return None;
    }
    Some(AudioClip {
        samples: std::sync::Arc::new(samples),
        rate,
        channels,
    })
}

// ---- BGRA(padded)→ RGBA(tight),canvas 模式 ---------------------------------

fn to_rgba_tight(src: &[u8], width: u32, height: u32, padded_row: u32, dst: &mut [u8]) {
    let stride = padded_row as usize;
    for row in 0..height as usize {
        let s = row * stride;
        let d = row * width as usize * 4;
        for px in 0..width as usize {
            let si = s + px * 4;
            let di = d + px * 4;
            dst[di] = src[si + 2];
            dst[di + 1] = src[si + 1];
            dst[di + 2] = src[si];
            dst[di + 3] = src[si + 3];
        }
    }
}

// ---- 会话 --------------------------------------------------------------------

/// 渲染后端。Windows 用原生窗口直渲(高帧率),其他平台离屏 + IPC。
enum Backend {
    #[cfg(windows)]
    Native {
        hwnd: isize,
        /// 子窗口(随父窗口移动,零跟踪)或有属主的顶层弹出窗口
        /// (任意线程可建,由渲染线程轮询跟踪位置)。
        popup: bool,
        /// popup 跟踪用:主窗口句柄 + 上次期望的 CSS rect 与缩放。
        main_hwnd: isize,
        last_rect: PreviewRect,
        last_scale: f64,
        renderer: surface::SurfaceRenderer,
        /// 上次 Move 的可见性(rect 非零 = 页面可见)。
        visible: bool,
    },
    Offscreen {
        renderer: Renderer,
        bgra_scratch: Vec<u8>,
    },
}

impl Backend {
    fn destroy(self, app: &AppHandle) {
        match self {
            #[cfg(windows)]
            // 子窗口由主线程创建,须回主线程销毁;popup 在渲染线程
            // 创建,当前线程直接销毁。
            Backend::Native {
                hwnd, popup: false, ..
            } => native::destroy_child_on_main(app, hwnd),
            #[cfg(windows)]
            Backend::Native {
                hwnd, popup: true, ..
            } => unsafe {
                DestroyWindow(hwnd as HWND);
            },
            Backend::Offscreen { .. } => {}
        }
    }
}

struct Session {
    app: AppHandle,
    beatmap_path: String,
    /// 谱面音频路径(open 时即解析,音频开关切换时懒加载)。
    audio_path: Option<std::path::PathBuf>,
    /// 当前是否带背景图(与 SetOptions 的 bg 比较决定是否重建图集)。
    has_bg: bool,
    game: game::GameData,
    atlas: draw::Atlas,
    bold: draw::TtfFont,
    semibold: draw::TtfFont,
    state: scene::SceneState,
    list: draw::DrawList,
    backend: Backend,
    t: f64,
    t0: f64,
    duration: f64,
    playing: bool,
    clock: Instant,
    dirty: bool,
    last_emit: Instant,
    /// Z 序重压节奏(对抗 WebView2 抬升自己)。
    last_top_assert: Instant,
    /// 音频输出(rodio)。`_stream` 必须存活整个会话。
    _stream: Option<rodio::OutputStream>,
    sink: Option<rodio::Sink>,
    clip: Option<AudioClip>,
    audio_offset: f64,
}

impl Session {
    /// 两种后端各自热替换图集纹理。
    fn set_atlas(&mut self, atlas: &draw::Atlas) {
        match &mut self.backend {
            #[cfg(windows)]
            Backend::Native { renderer, .. } => renderer.set_atlas(atlas),
            Backend::Offscreen { renderer, .. } => renderer.set_atlas(atlas),
        }
    }

    fn mode(&self) -> &'static str {
        match &self.backend {
            #[cfg(windows)]
            Backend::Native { .. } => "native",
            Backend::Offscreen { .. } => "canvas",
        }
    }

    /// 从当前 t 对应的音频位置重新起播(播放/seek 后调用)。
    fn audio_restart(&mut self) {
        let (Some(sink), Some(clip)) = (self.sink.as_ref(), self.clip.as_ref()) else {
            return;
        };
        let offset_ms = (self.t - self.audio_offset).max(0.0);
        let frame = (offset_ms / 1000.0 * clip.rate as f64) as usize;
        let pos = (frame * clip.channels as usize).min(clip.samples.len());
        sink.clear();
        sink.append(ClipSource {
            samples: std::sync::Arc::clone(&clip.samples),
            pos,
            rate: clip.rate,
            channels: clip.channels,
        });
        sink.play();
    }

    fn audio_pause(&self) {
        if let Some(sink) = self.sink.as_ref() {
            sink.pause();
        }
    }

    /// popup 模式:把期望位置(CSS rect × scale,相对主窗口客户区)换算
    /// 成屏幕坐标并贴到窗口上。主窗口被拖动/缩放时由渲染循环重复调用。
    #[cfg(windows)]
    fn enforce_native_position(&mut self) {
        let Backend::Native {
            hwnd,
            popup: true,
            main_hwnd,
            last_rect,
            last_scale,
            visible,
            ..
        } = &mut self.backend
        else {
            return;
        };
        if !*visible {
            unsafe { ShowWindow(*hwnd as HWND, SW_HIDE) };
            return;
        }
        let (ox, oy) = native::client_origin_on_screen(*main_hwnd);
        let scale = *last_scale;
        let x = ox + (last_rect.x * scale).round() as i32;
        let y = oy + (last_rect.y * scale).round() as i32;
        let w = (last_rect.width * scale).round() as i32;
        let h = (last_rect.height * scale).round() as i32;
        let (l, t, r, b) = native::window_rect(*hwnd);
        if l != x || t != y || (r - l) != w || (b - t) != h {
            unsafe {
                SetWindowPos(
                    *hwnd as HWND,
                    std::ptr::null_mut(),
                    x,
                    y,
                    w.max(1),
                    h.max(1),
                    SWP_NOACTIVATE,
                );
                ShowWindow(*hwnd as HWND, SW_SHOW);
            }
        }
    }

    /// 当前是否需要渲染(原生模式 = 窗口可见;canvas 模式 = 前端在拉帧)。
    fn visible_now(&self) -> bool {
        match &self.backend {
            #[cfg(windows)]
            Backend::Native { visible, .. } => *visible,
            #[cfg(not(windows))]
            Backend::Offscreen { .. } => LAST_PULL.lock().unwrap().elapsed() < FRAME_PULL_TIMEOUT,
            #[cfg(windows)]
            Backend::Offscreen { .. } => true,
        }
    }

    /// 渲染当前 t 的一帧并呈现(原生 present / canvas 发布帧缓冲)。
    fn draw_frame(&mut self) {
        let t = self.t;
        let snap = game::snapshot_at(&self.game, t);
        let assets = scene::Assets {
            atlas: &self.atlas,
            bold: &self.bold,
            semibold: &self.semibold,
        };
        self.list.clear();
        self.state
            .build_frame(&self.game, &assets, &snap, &mut self.list);
        self.list.finish();
        match &mut self.backend {
            #[cfg(windows)]
            Backend::Native {
                renderer, visible, ..
            } => {
                if *visible {
                    renderer.render(&self.list, CLEAR);
                }
            }
            Backend::Offscreen {
                renderer,
                bgra_scratch,
            } => {
                // 流水线:本帧提交后取上一帧的读回(GPU 不空转)。
                renderer.render_deferred(&self.list, CLEAR);
                if renderer.pending_len() > 0 {
                    renderer.read_oldest_into(bgra_scratch);
                    let mut frame = LATEST_FRAME.lock().unwrap();
                    frame.resize((RENDER_W * RENDER_H * 4) as usize, 0);
                    to_rgba_tight(
                        bgra_scratch,
                        RENDER_W,
                        RENDER_H,
                        renderer.padded_row,
                        &mut frame,
                    );
                }
            }
        }
        let _ = self.app.emit(
            "live-render-time",
            LiveRenderState {
                active: true,
                playing: self.playing,
                time_ms: t,
                duration_ms: self.duration,
            },
        );
        self.last_emit = Instant::now();
    }
}

fn worker(rx: std::sync::mpsc::Receiver<Cmd>) {
    let mut session: Option<Session> = None;

    loop {
        if session.is_some() {
            loop {
                match rx.try_recv() {
                    Ok(cmd) => handle_cmd(cmd, &mut session),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if let Some(s) = session.take() {
                            s.backend.destroy(&s.app);
                        }
                        *LATEST_FRAME.lock().unwrap() = Vec::new();
                        return;
                    }
                }
            }
        } else {
            match rx.recv() {
                Ok(cmd) => handle_cmd(cmd, &mut session),
                Err(_) => return,
            }
            continue;
        }

        let Some(s) = session.as_mut() else { continue };

        if s.playing {
            let now = Instant::now();
            s.t += now.duration_since(s.clock).as_secs_f64() * 1000.0;
            s.clock = now;
            if s.t >= s.duration {
                s.t = s.duration;
                s.playing = false;
                s.audio_pause();
            }
            s.dirty = true;
        }

        #[cfg(windows)]
        {
            s.enforce_native_position();
            if s.last_top_assert.elapsed() >= Duration::from_millis(400) {
                if let Backend::Native {
                    hwnd,
                    visible: true,
                    ..
                } = &s.backend
                {
                    native::bring_to_top(*hwnd);
                }
                s.last_top_assert = Instant::now();
            }
        }

        let visible = s.visible_now();
        if s.dirty && visible {
            s.draw_frame();
            s.dirty = false;
        } else if s.playing && s.last_emit.elapsed() >= Duration::from_millis(100) {
            let _ = s.app.emit(
                "live-render-time",
                LiveRenderState {
                    active: true,
                    playing: true,
                    time_ms: s.t,
                    duration_ms: s.duration,
                },
            );
            s.last_emit = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}

fn handle_cmd(cmd: Cmd, session: &mut Option<Session>) {
    match cmd {
        Cmd::Open {
            app,
            beatmap_path,
            replay_path,
            rect,
            scale,
            options,
            reply,
        } => {
            if let Some(s) = session.take() {
                s.backend.destroy(&s.app);
            }
            *LATEST_FRAME.lock().unwrap() = Vec::new();
            // catch_unwind:初始化失败(如驱动/wgpu panic)不能拖死整个
            // 渲染线程,否则后续所有命令都会"渲染线程无响应"。
            let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                open_session(&app, &beatmap_path, &replay_path, &options, rect, scale)
            }));
            let opened = match opened {
                Ok(r) => r,
                Err(panic) => {
                    let msg = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "未知 panic".into());
                    let _ = reply.send(Err(format!("渲染初始化崩溃: {msg}")));
                    return;
                }
            };
            match opened {
                Ok(mut s) => {
                    let duration = s.duration;
                    let t0 = s.t;
                    let _ = app.emit(
                        "live-render-time",
                        LiveRenderState {
                            active: true,
                            playing: false,
                            time_ms: t0,
                            duration_ms: duration,
                        },
                    );
                    s.draw_frame(); // 首帧
                    let mode = s.mode();
                    *session = Some(s);
                    let _ = reply.send(Ok(LiveOpenInfo {
                        duration_ms: duration,
                        mode,
                    }));
                }
                Err(e) => {
                    let _ = reply.send(Err(e));
                }
            }
        }
        Cmd::Move { rect, scale } => {
            #[cfg(windows)]
            if let Some(s) = session.as_mut() {
                if let Backend::Native {
                    hwnd,
                    popup,
                    main_hwnd: _,
                    renderer,
                    last_rect,
                    last_scale,
                    visible,
                } = &mut s.backend
                {
                    let show = rect.width > 0.0 && rect.height > 0.0 && !rect.suppressed;
                    if !*popup {
                        // 子窗口:客户区坐标,立即生效。
                        let (x, y) = (
                            (rect.x * scale).round() as i32,
                            (rect.y * scale).round() as i32,
                        );
                        let (w, h) = (
                            (rect.width * scale).round() as i32,
                            (rect.height * scale).round() as i32,
                        );
                        native::place(*hwnd, x, y, w, h, show);
                    }
                    // popup:只记录期望位置,渲染循环换算屏幕坐标并跟踪。
                    *last_rect = rect;
                    *last_scale = scale;
                    *visible = show;
                    if show {
                        renderer.resize(
                            (rect.width * scale).round().max(1.0) as u32,
                            (rect.height * scale).round().max(1.0) as u32,
                        );
                    } else {
                        renderer.resize(0, 0);
                    }
                    s.dirty = true;
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (rect, scale);
            }
        }
        Cmd::Seek(t) => {
            if let Some(s) = session.as_mut() {
                s.t = t.clamp(s.t0, s.duration);
                s.clock = Instant::now();
                s.dirty = true;
                if s.playing {
                    s.audio_restart();
                }
            }
        }
        Cmd::Play => {
            if let Some(s) = session.as_mut() {
                if s.t >= s.duration {
                    s.t = s.t0;
                }
                s.playing = true;
                s.clock = Instant::now();
                s.audio_restart();
                let _ = s.app.emit(
                    "live-render-time",
                    LiveRenderState { active: true, playing: true, time_ms: s.t, duration_ms: s.duration },
                );
            }
        }
        Cmd::Pause => {
            if let Some(s) = session.as_mut() {
                s.playing = false;
                s.audio_pause();
                let _ = s.app.emit(
                    "live-render-time",
                    LiveRenderState { active: true, playing: false, time_ms: s.t, duration_ms: s.duration },
                );
            }
        }
        Cmd::SetOptions(options) => {
            let Some(s) = session.as_mut() else { return };
            // ---- 即时字段:下一帧生效 ----
            s.state.hud.ur_bar = options.ur_bar;
            s.state.hud.key_overlay = options.key_overlay;
            s.state.follow_points = options.follow_points;
            s.audio_offset = options.audio_offset;

            // ---- 背景图:重建图集(字体 rect 补丁依赖图集)+ 热替换纹理 ----
            if options.bg != s.has_bg {
                let bg_image = if options.bg {
                    osu_replay_render::osu_background_file(&s.beatmap_path)
                        .map(|name| {
                            std::path::Path::new(&s.beatmap_path)
                                .parent()
                                .map(|p| p.join(name))
                                .unwrap_or_default()
                        })
                        .and_then(|p| osu_replay_render::decode_image_file(&p).ok())
                } else {
                    None
                };
                let (atlas, bold, semibold) = build_atlas(bg_image);
                s.set_atlas(&atlas);
                s.atlas = atlas;
                s.bold = bold;
                s.semibold = semibold;
                s.has_bg = options.bg;
            }
            s.state.bg_opacity = if s.has_bg {
                Some(options.bg_opacity.clamp(0.0, 1.0))
            } else {
                None
            };

            // ---- 音频:开→懒解码(一次性);关→静音 ----
            if options.audio {
                if s.clip.is_none() && s.audio_path.is_some() {
                    s.clip = load_audio(s.audio_path.as_ref().unwrap());
                    if s.playing {
                        s.audio_restart();
                    }
                }
            } else {
                s.audio_pause();
                if let Some(sink) = s.sink.as_ref() {
                    sink.clear();
                }
            }
            s.dirty = true;
        }
        Cmd::Close => {
            if let Some(s) = session.take() {
                s.backend.destroy(&s.app);
            }
            *LATEST_FRAME.lock().unwrap() = Vec::new();
        }
    }
}

fn open_session(
    app: &AppHandle,
    beatmap_path: &str,
    replay_path: &str,
    options: &LiveOptions,
    rect: PreviewRect,
    scale: f64,
) -> Result<Session, String> {
    let game = game::load(beatmap_path, replay_path).map_err(|e| format!("加载回放失败: {e}"))?;

    // 背景([Events] 0,0,"...",相对谱面目录,PNG/JPEG,解码进图集)。
    let map_dir = std::path::Path::new(beatmap_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let bg_image = if options.bg {
        osu_replay_render::osu_background_file(beatmap_path)
            .map(|name| map_dir.join(name))
            .and_then(|p| match osu_replay_render::decode_image_file(&p) {
                Ok(img) => Some(img),
                Err(_) => None,
            })
    } else {
        None
    };
    let has_bg = bg_image.is_some();
    let (atlas, bold, semibold) = build_atlas(bg_image);

    let t0 = game.snapshots.first().map(|s| s.time).unwrap_or(0.0);
    let duration = game.snapshots.last().map(|s| s.time).unwrap_or(0.0);
    let mut state = scene::SceneState::new(&game, RENDER_W, RENDER_H);
    // 固定 Argon-Pro 皮肤(无判定文字、滑条身体透明度 0.92)。
    state.pro_skin = true;
    state.hud.ur_bar = options.ur_bar;
    state.hud.key_overlay = options.key_overlay;
    state.follow_points = options.follow_points;
    state.bg_opacity = if has_bg {
        Some(options.bg_opacity.clamp(0.0, 1.0))
    } else {
        None
    };

    // BGM([General] AudioFilename,相对谱面目录):open 时只解析路径,
    // 解码在开启音频时进行(SetOptions 切换时懒加载,不重开会话)。
    let audio_path = osu_replay_render::osu_general_value(beatmap_path, "AudioFilename")
        .map(|name| map_dir.join(name))
        .filter(|p| p.exists());
    let audio = if options.audio {
        audio_path.as_deref().and_then(load_audio)
    } else {
        None
    };
    let (_stream, sink) = match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => (Some(stream), rodio::Sink::try_new(&handle).ok()),
        Err(_) => (None, None),
    };
    if audio.is_some() && sink.is_none() {
        eprintln!("live_render: 音频已解码但音频设备不可用,静音播放");
    }

    // ---- 后端选择 ------------------------------------------------------------
    // Windows:原生子窗口直渲(高帧率,零拷贝 present);创建失败(驱动/
    // 窗口环境异常)自动回退 canvas 模式,功能不中断。
    #[cfg(windows)]
    let backend = {
        let make_offscreen = || Backend::Offscreen {
            renderer: Renderer::new(RENDER_W, RENDER_H, &atlas),
            bgra_scratch: Vec::new(),
        };
        let build_native = |main_hwnd: isize| -> Backend {
            let (x, y) = (
                (rect.x * scale).round() as i32,
                (rect.y * scale).round() as i32,
            );
            let (w, h) = (
                (rect.width * scale).round() as i32,
                (rect.height * scale).round() as i32,
            );
            let visible = rect.width > 0.0 && rect.height > 0.0;

            // 首选:WS_CHILD 子窗口(随父窗口移动,零跟踪),主线程创建。
            // 次选:有属主的顶层弹出窗口(无线程限制),渲染线程创建并轮询跟踪位置。
            let (hwnd, popup) = match native::create_child_on_main(app, main_hwnd, x, y, w, h) {
                Ok(hwnd) => (hwnd, false),
                Err(e) => {
                    eprintln!("live_render: 子窗口创建失败({e}),改用属主弹出窗口");
                    let (ox, oy) = native::client_origin_on_screen(main_hwnd);
                    match native::create_popup(ox + x, oy + y, w, h) {
                        Ok(hwnd) => {
                            native::set_owner(hwnd, main_hwnd);
                            (hwnd, true)
                        }
                        Err(e) => {
                            eprintln!("live_render: 原生直渲不可用({e}),回退 canvas 模式");
                            return make_offscreen();
                        }
                    }
                }
            };

            match surface::SurfaceRenderer::new(RENDER_W, RENDER_H, &atlas, hwnd) {
                Ok(mut renderer) => {
                    if visible {
                        renderer.resize(w.max(1) as u32, h.max(1) as u32);
                    } else {
                        renderer.resize(0, 0);
                    }
                    Backend::Native {
                        hwnd,
                        popup,
                        main_hwnd,
                        last_rect: rect,
                        last_scale: scale,
                        renderer,
                        visible,
                    }
                }
                Err(e) => {
                    eprintln!("live_render: 初始化渲染器失败({e}),回退 canvas 模式");
                    if popup {
                        unsafe { DestroyWindow(hwnd as HWND) };
                    } else {
                        native::destroy_child_on_main(app, hwnd);
                    }
                    make_offscreen()
                }
            }
        };
        match app
            .get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as isize)
        {
            Some(main_hwnd) => build_native(main_hwnd),
            None => {
                eprintln!("live_render: 无法获取主窗口句柄,回退 canvas 模式");
                make_offscreen()
            }
        }
    };
    #[cfg(not(windows))]
    let backend = Backend::Offscreen {
        renderer: Renderer::new(RENDER_W, RENDER_H, &atlas),
        bgra_scratch: Vec::new(),
    };

    Ok(Session {
        app: app.clone(),
        beatmap_path: beatmap_path.to_string(),
        audio_path,
        has_bg,
        game,
        atlas,
        bold,
        semibold,
        state,
        list: draw::DrawList::new(),
        backend,
        t: t0,
        t0,
        duration,
        playing: false,
        clock: Instant::now(),
        dirty: true,
        last_emit: Instant::now(),
        last_top_assert: Instant::now(),
        _stream,
        sink,
        clip: audio,
        audio_offset: options.audio_offset,
    })
}

// ---- Tauri commands ----------------------------------------------------------

#[tauri::command(async)]
pub fn live_render_open(
    app: AppHandle,
    beatmap_path: String,
    replay_path: String,
    rect: PreviewRect,
    options: LiveOptions,
) -> CommandResult<LiveOpenInfo> {
    // Windows 原生模式需要主窗口缩放比(物理坐标 = CSS px × scale)。
    #[cfg(windows)]
    let scale = app
        .get_webview_window("main")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    #[cfg(not(windows))]
    let scale = 1.0;
    let (tx, rx) = channel();
    send(Cmd::Open {
        app,
        beatmap_path,
        replay_path,
        rect,
        scale,
        options,
        reply: tx,
    });
    rx.recv()
        .map_err(|_| CommandError::new("LIVE_RENDER", "渲染线程无响应"))?
        .map_err(|e| CommandError::new("LIVE_RENDER", e))
}

/// 最新一帧(RGBA,RENDER_W×RENDER_H×4);空 payload 表示尚无帧。
/// canvas 模式专用;原生模式不产生帧数据。
#[tauri::command]
pub fn live_render_frame() -> tauri::ipc::Response {
    #[cfg(not(windows))]
    {
        *LAST_PULL.lock().unwrap() = Instant::now();
    }
    tauri::ipc::Response::new(LATEST_FRAME.lock().unwrap().clone())
}

/// 预览区域位置(CSS px)。仅 Windows 原生模式消费;其他平台 no-op。
#[tauri::command]
pub fn live_render_move(app: AppHandle, rect: PreviewRect) -> CommandResult<()> {
    #[cfg(windows)]
    {
        let scale = app
            .get_webview_window("main")
            .and_then(|w| w.scale_factor().ok())
            .unwrap_or(1.0);
        send(Cmd::Move { rect, scale });
    }
    #[cfg(not(windows))]
    {
        let _ = (app, rect);
    }
    Ok(())
}

/// 渲染参数原地生效(零重载):即时字段改 SceneState;bg 重建图集并
/// 热替换纹理;audio 懒加载/静音。不触碰 wgpu 设备/窗口/判定数据。
#[tauri::command]
pub fn live_render_set_options(options: LiveOptions) {
    send(Cmd::SetOptions(options));
}

#[tauri::command]
pub fn live_render_seek(time_ms: f64) {
    send(Cmd::Seek(time_ms));
}

#[tauri::command]
pub fn live_render_play() {
    send(Cmd::Play);
}

#[tauri::command]
pub fn live_render_pause() {
    send(Cmd::Pause);
}

#[tauri::command]
pub fn live_render_close() {
    send(Cmd::Close);
}

// NULTEST-MARKER-X

// ---- 视频导出(离屏渲染 → ffmpeg 管道 → mp4) --------------------------------

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportParams {
    pub out_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// "x264" | "x265" | "nvenc"。
    pub encoder: String,
    /// crf(软件)/ cq(nvenc),默认 18。
    pub quality: u32,
    /// 混入 BGM(第二遍 ffmpeg:视频流拷贝 + AAC)。
    pub audio: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct ExportProgress {
    /// "render" | "mux" | "done"。
    pub phase: &'static str,
    pub frame: u32,
    pub total: u32,
    pub message: String,
}

static EXPORT_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static EXPORT_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// FFmpeg 解析:设置中手动路径 > PATH 自动检测 > danser 发行包自带。
fn ffmpeg_path(
    state: &tauri::State<'_, crate::app::state::AppState>,
) -> Option<std::path::PathBuf> {
    let saved = state.store.snapshot().ok().and_then(|snapshot| {
        Some((
            snapshot.settings.ffmpeg_executable_path.clone(),
            snapshot.settings.danser_executable_path.clone(),
        ))
    });
    let (ffmpeg, danser) = saved.unzip();
    crate::danser::resolve_ffmpeg_path(ffmpeg.flatten().as_deref(), danser.flatten().as_deref())
}

/// 设置页展示用:当前生效的 FFmpeg(路径 + 版本 + 来源)。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub path: Option<String>,
    pub version: Option<String>,
    /// "manual" | "path" | "danser" | ""(未找到)。
    pub source: &'static str,
}

#[tauri::command]
pub fn live_render_get_ffmpeg_status(
    state: tauri::State<'_, crate::app::state::AppState>,
) -> FfmpegStatus {
    let settings = state.store.snapshot().ok().map(|s| {
        (
            s.settings.ffmpeg_executable_path.clone(),
            s.settings.danser_executable_path.clone(),
        )
    });
    let (manual, danser) = settings.unzip();
    let (manual, danser) = (manual.flatten(), danser.flatten());

    let mut source = "";
    let path = if let Some(m) = manual
        .as_deref()
        .filter(|m| std::path::Path::new(m).is_file())
    {
        source = "manual";
        Some(std::path::PathBuf::from(m))
    } else if let Some(p) = crate::platform::find_in_path("ffmpeg")
        .or_else(|| crate::platform::find_in_path("ffmpeg.exe"))
    {
        source = "path";
        Some(p)
    } else {
        danser
            .as_deref()
            .and_then(|d| crate::danser::resolve_ffmpeg_path(None, Some(d)))
            .map(|p| {
                source = "danser";
                p
            })
    };
    let version = path.as_ref().and_then(|p| {
        std::process::Command::new(p)
            .arg("-version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .next()
                    .map(String::from)
            })
    });
    FfmpegStatus {
        path: path.map(|p| p.display().to_string()),
        version,
        source,
    }
}

/// 检测用户 FFmpeg(PATH 自动检测优先,danser 发行包兜底);
/// 返回版本首行(None = 未找到)。
#[tauri::command]
pub fn live_render_check_ffmpeg(
    state: tauri::State<'_, crate::app::state::AppState>,
) -> CommandResult<Option<String>> {
    let Some(path) = ffmpeg_path(&state) else {
        return Ok(None);
    };
    match std::process::Command::new(&path).arg("-version").output() {
        Ok(out) if out.status.success() => Ok(Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("ffmpeg")
                .to_string(),
        )),
        _ => Ok(None),
    }
}

/// 导出当前回放为视频。独立线程 + 独立 wgpu 设备,不影响实时预览;
/// 进度经 `live-render-export` 事件推送。
#[tauri::command(async)]
pub fn live_render_export(
    app: AppHandle,
    state: tauri::State<'_, crate::app::state::AppState>,
    beatmap_path: String,
    replay_path: String,
    options: LiveOptions,
    params: ExportParams,
) -> CommandResult<String> {
    use std::sync::atomic::Ordering;
    let ffmpeg = ffmpeg_path(&state)
        .ok_or_else(|| CommandError::new("LIVE_RENDER", "未找到 FFmpeg(已尝试设置路径、PATH 与 danser 发行包),请在设置页配置或安装 FFmpeg 后重试"))?;
    if EXPORT_RUNNING.swap(true, Ordering::SeqCst) {
        return Err(CommandError::new("LIVE_RENDER", "已有导出任务在进行中"));
    }
    EXPORT_CANCEL.store(false, Ordering::SeqCst);
    let (tx, rx) = channel::<Result<String, String>>();
    std::thread::Builder::new()
        .name("live-render-export".into())
        .spawn(move || {
            let result = run_export(
                &app,
                &ffmpeg,
                &beatmap_path,
                &replay_path,
                &options,
                &params,
            );
            EXPORT_RUNNING.store(false, Ordering::SeqCst);
            let _ = tx.send(result);
        })
        .map_err(|e| CommandError::new("LIVE_RENDER", format!("无法启动导出线程: {e}")))?;
    rx.recv()
        .map_err(|_| CommandError::new("LIVE_RENDER", "导出线程无响应"))?
        .map_err(|e| CommandError::new("LIVE_RENDER", e))
}

#[tauri::command]
pub fn live_render_export_cancel() {
    use std::sync::atomic::Ordering;
    EXPORT_CANCEL.store(true, Ordering::SeqCst);
}

/// 在系统中打开导出文件所在文件夹。
#[tauri::command]
pub fn live_render_open_export_output(path: String) -> CommandResult<()> {
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        return Err(CommandError::new("LIVE_RENDER", "路径必须是绝对路径"));
    }
    crate::platform::reveal_path(&path)
        .map_err(|e| CommandError::new("LIVE_RENDER", format!("无法打开所在文件夹: {e}")))
}

fn repack_tight(bgra: &[u8], width: u32, height: u32, padded_row: u32, tight: &mut Vec<u8>) {
    tight.clear();
    if padded_row == width * 4 {
        tight.extend_from_slice(bgra);
        return;
    }
    tight.reserve((width * height * 4) as usize);
    for row in 0..height as usize {
        let start = row * padded_row as usize;
        tight.extend_from_slice(&bgra[start..start + width as usize * 4]);
    }
}

fn run_export(
    app: &AppHandle,
    ffmpeg: &std::path::Path,
    beatmap_path: &str,
    replay_path: &str,
    options: &LiveOptions,
    params: &ExportParams,
) -> Result<String, String> {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    let emit = |phase: &'static str, frame: u32, total: u32, message: String| {
        let _ = app.emit(
            "live-render-export",
            ExportProgress {
                phase,
                frame,
                total,
                message,
            },
        );
    };
    let cancelled = || EXPORT_CANCEL.load(Ordering::SeqCst);

    // ---- 加载(与预览会话独立,不共享 GPU 设备) ----
    let game = game::load(beatmap_path, replay_path).map_err(|e| format!("加载回放失败: {e}"))?;
    let map_dir = std::path::Path::new(beatmap_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let bg_image = if options.bg {
        osu_replay_render::osu_background_file(beatmap_path)
            .map(|name| map_dir.join(name))
            .and_then(|p| osu_replay_render::decode_image_file(&p).ok())
    } else {
        None
    };
    let has_bg = bg_image.is_some();
    let (atlas, bold, semibold) = build_atlas(bg_image);

    let mut renderer = Renderer::new(params.width, params.height, &atlas);
    let mut state = scene::SceneState::new(&game, params.width, params.height);
    state.pro_skin = true;
    state.hud.ur_bar = options.ur_bar;
    state.hud.key_overlay = options.key_overlay;
    state.follow_points = options.follow_points;
    state.bg_opacity = if has_bg {
        Some(options.bg_opacity.clamp(0.0, 1.0))
    } else {
        None
    };

    // ---- 帧时间轴(与 CLI 语义一致) ----
    let t0 = game.snapshots.first().map(|s| s.time).unwrap_or(0.0);
    let duration = game.snapshots.last().map(|s| s.time).unwrap_or(0.0);
    let frame_times: Vec<f64> = if params.fps == 60 {
        game.snapshots.iter().map(|s| s.time).collect()
    } else {
        let step = 1000.0 / params.fps as f64;
        let mut t = t0;
        let mut v = Vec::new();
        while t <= duration {
            v.push(t);
            t += step;
        }
        v
    };
    let total = frame_times.len() as u32;
    if total == 0 {
        return Err("没有可渲染的帧".into());
    }

    // ---- ffmpeg 视频轨 ----
    let tmp = format!("{}.video.tmp.mp4", params.out_path);
    let log_path = format!("{}.ffmpeg.log", params.out_path);
    let (in_pix_fmt, encode_args): (&str, Vec<String>) = if params.encoder == "nvenc" {
        (
            "bgr0",
            vec![
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-rc",
                "vbr",
                "-cq",
                &params.quality.to_string(),
                "-b:v",
                "0",
                "-movflags",
                "+faststart",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        )
    } else {
        let codec = if params.encoder == "x265" {
            "libx265"
        } else {
            "libx264"
        };
        (
            "bgra",
            vec![
                "-c:v",
                codec,
                "-preset",
                "medium",
                "-crf",
                &params.quality.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        )
    };
    let log_file = std::fs::File::create(&log_path).map_err(|e| format!("无法写编码日志: {e}"))?;
    let mut command = std::process::Command::new(&ffmpeg);
    command
        .arg("-y")
        .args(["-f", "rawvideo", "-pix_fmt", in_pix_fmt])
        .arg("-s")
        .arg(format!("{}x{}", params.width, params.height))
        .arg("-r")
        .arg(params.fps.to_string())
        .args(["-i", "-"]);
    command
        .args(&encode_args)
        .arg(&tmp)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(log_file);
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法启动 ffmpeg: {e}"))?;
    // 写入线程:有界通道背压,整帧一次写。
    let (wtx, wrx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
    let mut stdin = child.stdin.take().ok_or("无法打开 ffmpeg stdin")?;
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for frame in wrx {
            stdin.write_all(&frame)?;
        }
        drop(stdin);
        Ok(())
    });

    // ---- 渲染循环 ----
    let mut bgra = Vec::new();
    let mut tight = Vec::new();
    let assets = scene::Assets {
        atlas: &atlas,
        bold: &bold,
        semibold: &semibold,
    };
    let mut list = draw::DrawList::new();
    let mut exported = 0u32;
    let mut fail: Option<String> = None;
    for (n, &ft) in frame_times.iter().enumerate() {
        if cancelled() {
            fail = Some("已取消".into());
            break;
        }
        let snap = game::snapshot_at(&game, ft);
        list.clear();
        state.build_frame(&game, &assets, &snap, &mut list);
        list.finish();
        renderer.render_deferred(&list, CLEAR);
        if renderer.pending_len() > 0 {
            renderer.read_oldest_into(&mut bgra);
            repack_tight(
                &bgra,
                params.width,
                params.height,
                renderer.padded_row,
                &mut tight,
            );
            if wtx.send(std::mem::take(&mut tight)).is_err() {
                fail = Some("ffmpeg 写入中断(编码器提前退出,详见 .ffmpeg.log)".into());
                break;
            }
            exported += 1;
        }
        if n % 60 == 0 {
            emit("render", exported, total, format!("{}/{}", exported, total));
        }
    }
    // 排空流水线尾部帧。
    if fail.is_none() {
        while renderer.pending_len() > 0 {
            renderer.read_oldest_into(&mut bgra);
            repack_tight(
                &bgra,
                params.width,
                params.height,
                renderer.padded_row,
                &mut tight,
            );
            if wtx.send(std::mem::take(&mut tight)).is_err() {
                fail = Some("ffmpeg 写入中断".into());
                break;
            }
            exported += 1;
        }
    }
    drop(wtx);
    let write_result = writer
        .join()
        .map_err(|_| "编码线程崩溃".to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    if cancelled() {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&tmp);
        return Err("已取消".into());
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if let Some(f) = fail {
        let _ = std::fs::remove_file(&tmp);
        return Err(f);
    }
    write_result.map_err(|e| format!("ffmpeg 写入失败: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "ffmpeg 编码失败({:?}),详见 {}",
            status.code(),
            log_path
        ));
    }

    // ---- 音频混流(可选,第二遍:视频流拷贝) ----
    if params.audio {
        let audio_path = osu_replay_render::osu_general_value(beatmap_path, "AudioFilename")
            .map(|name| map_dir.join(name))
            .filter(|p| p.exists());
        if let Some(audio) = audio_path {
            emit("mux", exported, total, "混入音频…".into());
            let audio_start = ((t0 - options.audio_offset) / 1000.0).max(0.0);
            let status = std::process::Command::new(&ffmpeg)
                .args(["-y", "-v", "error"])
                .arg("-i")
                .arg(&tmp)
                .arg("-ss")
                .arg(format!("{audio_start:.3}"))
                .arg("-i")
                .arg(&audio)
                .args([
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-c:v",
                    "copy",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-shortest",
                    "-movflags",
                    "+faststart",
                ])
                .arg(&params.out_path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|e| format!("音频混流启动失败: {e}"))?;
            let _ = std::fs::remove_file(&tmp);
            if !status.success() {
                return Err("音频混流失败".into());
            }
        } else {
            std::fs::rename(&tmp, &params.out_path).map_err(|e| format!("无法写出文件: {e}"))?;
        }
    } else {
        std::fs::rename(&tmp, &params.out_path).map_err(|e| format!("无法写出文件: {e}"))?;
    }

    emit("done", exported, total, params.out_path.clone());
    Ok(params.out_path.clone())
}
