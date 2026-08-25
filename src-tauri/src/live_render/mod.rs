//! 实时回放预览:wgpu 直接渲染到原生窗口 surface(高帧率,零拷贝 present)。
//! Windows 用 WS_CHILD 子窗口(或属主弹出窗口兜底);Linux 用 X11 子窗口

use crate::error::{CommandError, CommandResult};
use osu_replay_render::{build_atlas, draw, game, hitsound, render::Renderer, scene, skin};
use std::collections::HashMap;
use std::sync::mpsc::{Sender, TryRecvError, channel};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri::{AppHandle, Emitter};

// 窗口 surface 直渲(blit 管线)由 osu-replay-render 库提供,
// 这里不再自带 wgpu 依赖。
pub use osu_replay_render::surface;
#[cfg(target_os = "linux")]
mod x11;

#[cfg(windows)]
use windows_sys::Win32::Foundation::POINT;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_HWNDPARENT,
    GetWindowRect, HTTRANSPARENT, RegisterClassW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_NCHITTEST, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

/// 内部渲染分辨率(16:9;原生窗口上 letterbox 等比缩放)。
const RENDER_W: u32 = 1280;
const RENDER_H: u32 = 720;
const CLEAR: [f64; 4] = [0.055, 0.055, 0.075, 1.0];

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct PreviewRect {
    /// 物理像素(= CSS px × window.devicePixelRatio),相对 WebView 视口
    /// 左上角(原生窗口定位使用)。由前端换算:WebKitGTK 在 X11 小数
    /// 缩放(Xft.dpi=120 → dpr 1.25)下,后端能拿到的 tauri
    /// scale_factor 只是 GDK 整数缩放(=1),换算只能以 dpr 为准。
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
    /// 预览 BGM 对齐偏移 ms,默认 0(音频位置 = 回放时间 − 偏移)。
    /// 仅实时预览使用;导出走 `ExportParams::audio_offset`,两者独立。
    pub audio_offset: f64,
    /// 预览播放音效(命中音/滑条节点/combobreak,ArgonPro 采样,
    /// lazer 语义;与 BGM 同走 Kira 输出)。
    pub hitsounds: bool,
    /// 光标尺寸倍率 0.1..=2(lazer `GameplayCursorSize`,默认 1)。
    pub cursor_size: f32,
    /// 用户皮肤目录(解包 .osk 或游戏 Skins/<name>);None = 内置
    /// Argon-Pro。变更时热重建图集与场景(legacy 皮肤缓存随之重置),
    /// 不需要重开会话。
    pub skin_path: Option<String>,
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
            hitsounds: true,
            cursor_size: 1.0,
            skin_path: None,
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

/// open 的返回:总时长。
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOpenInfo {
    pub duration_ms: f64,
}

#[derive(Debug)]
enum Cmd {
    Open {
        app: AppHandle,
        beatmap_path: String,
        replay_path: String,
        /// 原生窗口初始定位(物理 px,相对 WebView 视口)。
        rect: PreviewRect,
        options: LiveOptions,
        /// ffmpeg 路径(open 命令侧解析,DT/HT 变调预处理用)。
        ffmpeg: Option<std::path::PathBuf>,
        reply: Sender<Result<LiveOpenInfo, String>>,
    },
    Move {
        rect: PreviewRect,
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

// ---- 音频(Kira:BGM + 音效共用一个输出流) ------------------------------------

/// osu! 默认 Music 通道音量(`OsuGame.GetFrameworkConfigDefaults` 0.6)。
const MUSIC_VOLUME: f64 = 0.6;

/// 解码 BGM 文件(mp3/flac/ogg/wav,symphonia 全量入内存)。
/// DT/HT(rate≠1 且非 NC)优先走 ffmpeg「变速不变调」预处理,与导出
/// 侧 atempo 同语义;ffmpeg 未配置或执行失败时回退原样解码——kira
/// 的 playback_rate 本就是变速变调(游戏原生 DT 听感),预览不中断。
/// NC 始终原样(nightcore without the pitch isn't nightcore,与导出
/// 侧 asetrate 分支一致)。
fn load_bgm(
    path: &std::path::Path,
    rate: f64,
    nightcore: bool,
    ffmpeg: Option<&std::path::Path>,
) -> Option<kira::sound::static_sound::StaticSoundData> {
    if !nightcore
        && (rate - 1.0).abs() >= 1e-3
        && (0.5..=2.0).contains(&rate)
        && let Some(data) = load_bgm_pitch_preserved(path, rate, ffmpeg)
    {
        return Some(data);
    }
    let bytes = std::fs::read(path).ok()?;
    kira::sound::static_sound::StaticSoundData::from_cursor(std::io::Cursor::new(bytes)).ok()
}

/// ffmpeg 恒时长变调:整段音调 ÷rate、时长不变。滤镜链(采样率先
/// 归一到 44100,asetrate 才能用静态参数):aresample=44100 →
/// asetrate=44100/rate(调 ÷rate、长 ×rate)→ aresample=44100 →
/// atempo=rate(压回原长、调不变;0.5..=2.0 覆盖 DT 1.5 / HT 0.75,
/// 超界由调用方拦截)。kira 侧仍按 playback_rate(rate) 播放,升调
/// ×rate 与预处理 ÷rate 相抵 → 净效果变速不变调;帧时长不变,
/// 起播/seek 位置映射与变调路径完全一致。输出 f32 wav 经管道回读,
/// 由 kira 自带的 symphonia 解码。阻塞渲染线程数百毫秒(一次性,
/// 与整段解码同量级)。
fn load_bgm_pitch_preserved(
    path: &std::path::Path,
    rate: f64,
    ffmpeg: Option<&std::path::Path>,
) -> Option<kira::sound::static_sound::StaticSoundData> {
    let ffmpeg = ffmpeg?;
    let filters = format!(
        "aresample=44100,asetrate={},aresample=44100,atempo={:.6}",
        (44100.0 / rate).round() as i64,
        rate
    );
    let output = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .arg("-i")
        .arg(path)
        .arg("-filter:a")
        .arg(&filters)
        .args(["-map", "0:a:0", "-ac", "2", "-c:a", "pcm_f32le", "-f", "wav", "-"])
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "live_render: ffmpeg 变调预处理失败({}),回退 kira 变调播放",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    kira::sound::static_sound::StaticSoundData::from_cursor(std::io::Cursor::new(output.stdout))
        .ok()
}

// ---- 音效(Kira;命中音/滑条节点音/combobreak) --------------------------------

/// 全局 Kira 管理器:与会话无关地持有输出设备(BGM 与音效共用同
/// 一条输出流;初始化失败 = 音频/音效禁用,不影响预览其余功能)。
static KIRA: LazyLock<Mutex<Option<kira::AudioManager<kira::DefaultBackend>>>> =
    LazyLock::new(|| Mutex::new(None));

/// osu! 默认 Effect 通道音量(`OsuGame.GetFrameworkConfigDefaults` 0.6)。
const EFFECT_VOLUME: f64 = 0.6;

/// lazer `CalculateSamplePlaybackBalance`(PositionalHitsoundsLevel 0.8):
/// balance = round2(1.6·(x/512 − 0.5)),映射到 Kira 的 0..1 声像。
fn kira_panning(x: f32) -> f32 {
    let b = (1.6 * (x as f64 / 512.0 - 0.5) * 100.0).round() / 100.0;
    ((b.clamp(-1.0, 1.0) + 1.0) / 2.0).clamp(0.0, 1.0) as f32
}

/// 线性幅度 → Kira 的分贝音量(-60dB 起静音)。
fn amplitude_to_decibels(amplitude: f64) -> kira::Decibels {
    if amplitude <= 0.0 {
        kira::Decibels::SILENCE
    } else {
        kira::Decibels((20.0 * amplitude.log10()) as f32)
    }
}

/// 确保 Kira 管理器已初始化(输出设备打开一次,全局复用)。
fn ensure_kira_manager() {
    let mut guard = KIRA.lock().unwrap();
    if guard.is_none() {
        *guard = kira::AudioManager::new(Default::default()).ok();
        if guard.is_none() {
            eprintln!("live_render: Kira 音频设备初始化失败,音频/音效禁用");
        }
    }
}

/// 预加载一个采样(内嵌 ArgonPro wav → Kira StaticSoundData;
/// 播放时 Clone 并覆写 settings)。
fn load_kira_sound(bank: &str, name: &str) -> Option<kira::sound::static_sound::StaticSoundData> {
    let bytes = hitsound::sample_bytes(bank, name)?.to_vec();
    let data = kira::sound::static_sound::StaticSoundData::from_cursor(std::io::Cursor::new(bytes))
        .ok()?;
    ensure_kira_manager();
    Some(data)
}

// ---- 会话 --------------------------------------------------------------------

/// 渲染后端:原生窗口直渲(高帧率,零拷贝 present)。
enum Backend {
    Native {
        renderer: surface::SurfaceRenderer,
        /// 上次 Move 的可见性(rect 非零且未 suppressed = 预览可见)。
        visible: bool,
        /// Windows:子窗口(随父窗口移动,零跟踪)或有属主的顶层弹出
        /// 窗口(任意线程可建,由渲染线程轮询跟踪位置)。
        #[cfg(windows)]
        hwnd: isize,
        #[cfg(windows)]
        popup: bool,
        /// popup 跟踪用:主窗口句柄 + 上次期望的物理 px rect。
        #[cfg(windows)]
        main_hwnd: isize,
        #[cfg(windows)]
        last_rect: PreviewRect,
        /// Linux:X11 子窗口(主窗口 XID 之下,随父窗口移动,零跟踪)。
        #[cfg(target_os = "linux")]
        x11: x11::Window,
    },
}

impl Backend {
    fn destroy(self, app: &AppHandle) {
        // wgpu surface 引用着原生窗口,必须先于窗口本体销毁。
        #[cfg(not(windows))]
        let _ = app;
        #[cfg(windows)]
        {
            // 子窗口由主线程创建,须回主线程销毁;popup 在渲染线程销毁。
            let Backend::Native {
                renderer,
                hwnd,
                popup,
                ..
            } = self;
            drop(renderer);
            if popup {
                unsafe { DestroyWindow(hwnd as HWND) };
            } else {
                native::destroy_child_on_main(app, hwnd);
            }
        }
        #[cfg(target_os = "linux")]
        {
            let Backend::Native { renderer, x11, .. } = self;
            drop(renderer);
            x11.destroy();
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let Backend::Native { renderer, .. } = self;
            drop(renderer);
        }
    }
}

struct Session {
    app: AppHandle,
    beatmap_path: String,
    /// 谱面音频路径(open 时即解析,音频开关切换时懒加载)。
    audio_path: Option<std::path::PathBuf>,
    /// ffmpeg 可执行路径(open 时按 设置手动路径 > PATH > danser 解析
    /// 一次;DT/HT 预览的变调预处理用,缺 None = 回退 kira 变调)。
    ffmpeg: Option<std::path::PathBuf>,
    /// 当前是否带背景图(与 SetOptions 的 bg 比较决定是否重建图集)。
    has_bg: bool,
    /// 当前皮肤(None = 内置 Argon-Pro;与 SetOptions 比较决定热重建)。
    skin: skin::ResolvedSkin,
    skin_path: Option<String>,
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
    /// BGM 解码数据(开启音频时懒加载;Kira StaticSoundData)。
    bgm_data: Option<kira::sound::static_sound::StaticSoundData>,
    /// 当前 BGM 播放句柄(播放/暂停中;None = 未起播)。
    bgm_handle: Option<kira::sound::static_sound::StaticSoundHandle>,
    /// 回放在播但 BGM 刻意未起播:时间轴还在 audio_offset 之前(前奏
    /// 段,判定引擎从「首物件 − max(抢先量, 2000ms)」起算,首物件
    /// 2 秒内进场的谱面 t0 为负)。Kira 播放位置非负,起播即从 0 开
    /// 始播——前奏整段被快进,BGM 永久领先 |t0|·rate ms(音效按时间
    /// 轴触发,听感上全晚)。必须等时间轴跨过 offset 再起,见
    /// audio_play 与渲染循环的跨界检测。
    bgm_pending: bool,
    audio_offset: f64,
    /// 音效(一次性事件表,按时间排序;循环音不导出——ArgonPro 静音)。
    hs_events: Vec<hitsound::HitsoundEvent>,
    /// 下一个待触发事件的下标(播放推进/seek 时移动)。
    hs_cursor: usize,
    /// (bank, name) → Kira 采样数据(播放时 Clone),开启音效时懒加载。
    hs_sounds: HashMap<(&'static str, &'static str), kira::sound::static_sound::StaticSoundData>,
    /// 音效开关(LiveOptions.hitsounds 的当前值)。
    hitsounds: bool,
}

impl Session {
    /// 触发一个音效事件(音量 = 谱面音量(下限 5%)× Effect 0.6,
    /// 声像按物件 X;与离线导出的音效总线同语义)。
    fn fire_hitsound(&self, event: &hitsound::HitsoundEvent) {
        let Some(data) = self.hs_sounds.get(&(event.bank, event.name)) else {
            return;
        };
        let volume = event.volume.max(5) as f64 / 100.0 * EFFECT_VOLUME;
        let mut data = data.clone();
        data.settings = kira::sound::static_sound::StaticSoundSettings::new()
            .volume(amplitude_to_decibels(volume))
            .panning(kira::Panning(kira_panning(event.pan_x)));
        if let Some(manager) = KIRA.lock().unwrap().as_mut() {
            let _ = manager.play(data);
        }
    }

    /// 开启音效:构建事件表并预加载用到的采样(已加载则跳过)。
    fn ensure_hitsounds(&mut self) {
        if !self.hs_sounds.is_empty() || self.hs_events.is_empty() {
            return;
        }
        let mut sounds = HashMap::new();
        for event in &self.hs_events {
            let key = (event.bank, event.name);
            if sounds.contains_key(&key) {
                continue;
            }
            if let Some(handle) = load_kira_sound(event.bank, event.name) {
                sounds.insert(key, handle);
            }
        }
        self.hs_sounds = sounds;
    }
}

impl Session {
    /// 热替换图集纹理。
    fn set_atlas(&mut self, atlas: &draw::Atlas) {
        let Backend::Native { renderer, .. } = &mut self.backend;
        renderer.set_atlas(atlas);
    }

    /// BGM 文件位置(秒,按 audio_offset 换算)。仅当时间轴已跨过
    /// audio_offset 时才有意义;负位置钳 0 只是防御——前奏段的起播
    /// 必须走 bgm_pending 延迟(见 audio_play),不能拿钳 0 的位置
    /// 立即播。
    fn bgm_position(&self) -> f64 {
        ((self.t - self.audio_offset).max(0.0)) / 1000.0
    }

    /// 从当前 t 对应的音频位置重新起播(首次播放/重新开启音频)。
    /// BGM 以 game.rate 变速(速度+音调同变,游戏语义)。
    fn audio_restart(&mut self) {
        let Some(data) = self.bgm_data.clone() else {
            return;
        };
        ensure_kira_manager();
        let mut kira = KIRA.lock().unwrap();
        let Some(manager) = kira.as_mut() else {
            return;
        };
        if let Some(mut old) = self.bgm_handle.take() {
            old.stop(kira::Tween::default());
        }
        let mut data = data;
        data.settings = kira::sound::static_sound::StaticSoundSettings::new()
            .start_position(kira::sound::PlaybackPosition::Seconds(self.bgm_position()))
            .playback_rate(kira::PlaybackRate(self.game.rate))
            .volume(amplitude_to_decibels(MUSIC_VOLUME));
        match manager.play(data) {
            Ok(handle) => self.bgm_handle = Some(handle),
            Err(_) => eprintln!("live_render: BGM 起播失败"),
        }
    }

    /// 继续播放:暂停中的句柄直接 resume(避免整段缓冲重新克隆),
    /// 结束/未起播则从当前 t 重新起播。时间轴还在 audio_offset 之前
    /// (前奏段)时不起播:Kira 位置非负,立即起播只能钳 0 开播,前
    /// 奏被整段快进、BGM 永久领先;置 bgm_pending,由渲染循环在跨
    /// 过 offset 的那一拍起播。
    fn audio_play(&mut self) {
        if self.t < self.audio_offset {
            self.audio_stop();
            self.bgm_pending = true;
            return;
        }
        self.bgm_pending = false;
        match self.bgm_handle.as_ref().map(|h| h.state()) {
            Some(kira::sound::PlaybackState::Paused)
            | Some(kira::sound::PlaybackState::Pausing) => {
                if let Some(handle) = self.bgm_handle.as_mut() {
                    handle.resume(kira::Tween::default());
                }
            }
            Some(kira::sound::PlaybackState::Playing) => {}
            _ => self.audio_restart(),
        }
    }

    /// 播放中 seek:直接把句柄跳到新位置(不重新克隆缓冲)。
    fn audio_seek(&mut self) {
        match self.bgm_handle.as_mut().map(|h| h.state()) {
            Some(kira::sound::PlaybackState::Playing)
            | Some(kira::sound::PlaybackState::Paused) => {
                let position = self.bgm_position();
                self.bgm_handle.as_mut().unwrap().seek_to(position);
            }
            _ => {}
        }
    }

    fn audio_pause(&mut self) {
        if let Some(handle) = self.bgm_handle.as_mut() {
            handle.pause(kira::Tween::default());
        }
    }

    fn audio_stop(&mut self) {
        if let Some(mut handle) = self.bgm_handle.take() {
            handle.stop(kira::Tween::default());
        }
    }

    /// popup 模式:把期望位置(物理 px rect,相对主窗口客户区)换算
    /// 成屏幕坐标并贴到窗口上。主窗口被拖动/缩放时由渲染循环重复调用。
    #[cfg(windows)]
    fn enforce_native_position(&mut self) {
        let Backend::Native {
            hwnd,
            popup: true,
            main_hwnd,
            last_rect,
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
        let x = ox + last_rect.x.round() as i32;
        let y = oy + last_rect.y.round() as i32;
        let w = last_rect.width.round() as i32;
        let h = last_rect.height.round() as i32;
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

    /// 当前是否需要渲染(预览窗口可见)。
    fn visible_now(&self) -> bool {
        match &self.backend {
            Backend::Native { visible, .. } => *visible,
        }
    }

    /// 渲染当前 t 的一帧并 present。
    fn draw_frame(&mut self) {
        let t = self.t;
        let snap = game::snapshot_at(&self.game, t);
        let assets = scene::Assets {
            atlas: &self.atlas,
            bold: &self.bold,
            semibold: &self.semibold,
            skin: &self.skin,
        };
        self.list.clear();
        self.state
            .build_frame(&self.game, &assets, &snap, &mut self.list);
        self.list.finish();
        let Backend::Native {
            renderer, visible, ..
        } = &mut self.backend;
        if *visible {
            renderer.render(&self.list, CLEAR);
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
                        if let Some(mut s) = session.take() {
                            s.audio_stop();
                            s.backend.destroy(&s.app);
                        }
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
            // 按游戏速率推进:DT/HT 回放以真实速度预览,与变速后的
            // BGM(playback_rate)和音效事件(谱面时间轴)保持同步。
            s.t += now.duration_since(s.clock).as_secs_f64() * 1000.0 * s.game.rate;
            s.clock = now;
            // 前奏段结束:时间轴一跨过 audio_offset 立即起播 BGM(位置
            // 恰为 0,与音效同拍)。起播耗时(整段缓冲克隆 + 命令入队)
            // 之后重锚时钟,不计入时间轴——否则时间轴先跑,BGM 反向
            // 落后一小段。
            if s.bgm_pending && s.t >= s.audio_offset && s.t < s.duration {
                s.bgm_pending = false;
                s.audio_restart();
                s.clock = Instant::now();
            }
            if s.t >= s.duration {
                s.t = s.duration;
                s.playing = false;
                s.audio_pause();
            }
            s.dirty = true;
            // 音效:播放头跨过的事件即触发(事件表按时间排序)。
            if s.hitsounds && !s.hs_sounds.is_empty() {
                loop {
                    let Some(event) = s.hs_events.get(s.hs_cursor).copied() else {
                        break;
                    };
                    if event.time > s.t {
                        break;
                    }
                    s.fire_hitsound(&event);
                    s.hs_cursor += 1;
                }
            }
        }

        #[cfg(any(windows, target_os = "linux"))]
        {
            // Windows popup 模式:主窗口被拖动/缩放时轮询贴回屏幕位置。
            #[cfg(windows)]
            s.enforce_native_position();
            // 周期性压回兄弟栈顶:WebView(WebView2/GdkWindow)激活/重排
            // 时会把自己抬到我们之上。
            if s.last_top_assert.elapsed() >= Duration::from_millis(400) {
                #[cfg(windows)]
                if let Backend::Native {
                    hwnd,
                    visible: true,
                    ..
                } = &s.backend
                {
                    native::bring_to_top(*hwnd);
                }
                #[cfg(target_os = "linux")]
                if let Backend::Native {
                    x11, visible: true, ..
                } = &mut s.backend
                {
                    x11.bring_to_top();
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
            options,
            ffmpeg,
            reply,
        } => {
            if let Some(mut s) = session.take() {
                // Kira 的声音归混音器所有:丢弃句柄不会停播,必须显式
                // stop,否则旧会话的 BGM 一直响(与新会话叠音)。
                s.audio_stop();
                s.backend.destroy(&s.app);
            }
            // catch_unwind:初始化失败(如驱动/wgpu panic)不能拖死整个
            // 渲染线程,否则后续所有命令都会"渲染线程无响应"。
            let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                open_session(&app, &beatmap_path, &replay_path, &options, rect, ffmpeg)
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
                    *session = Some(s);
                    let _ = reply.send(Ok(LiveOpenInfo {
                        duration_ms: duration,
                    }));
                }
                Err(e) => {
                    let _ = reply.send(Err(e));
                }
            }
        }
        Cmd::Move { rect } => {
            #[cfg(any(windows, target_os = "linux"))]
            if let Some(s) = session.as_mut() {
                let show = rect.width > 0.0 && rect.height > 0.0 && !rect.suppressed;
                let (x, y) = (rect.x.round() as i32, rect.y.round() as i32);
                let (w, h) = (rect.width.round() as i32, rect.height.round() as i32);
                #[cfg(windows)]
                {
                    let Backend::Native {
                        hwnd,
                        popup,
                        last_rect,
                        visible,
                        ..
                    } = &mut s.backend;
                    if !*popup {
                        // 子窗口:父窗口客户区坐标,立即生效。
                        native::place(*hwnd, x, y, w, h, show);
                    }
                    // popup:只记录期望位置,渲染循环换算屏幕坐标并跟踪。
                    *last_rect = rect;
                    *visible = show;
                }
                #[cfg(target_os = "linux")]
                {
                    let Backend::Native { x11, visible, .. } = &mut s.backend;
                    // X11 子窗口:父窗口局部坐标,立即生效。
                    x11.place(x, y, w, h, show);
                    *visible = show;
                }
                {
                    let Backend::Native { renderer, .. } = &mut s.backend;
                    if show {
                        renderer.resize(w.max(1) as u32, h.max(1) as u32);
                    } else {
                        renderer.resize(0, 0);
                    }
                    s.dirty = true;
                }
                // 面板被切走(容器 display:none → rect 归零)时暂停
                // 播放;仅 suppressed(应用内弹窗临时遮挡)不打断。
                if rect.width <= 0.0 || rect.height <= 0.0 {
                    if s.playing {
                        s.playing = false;
                        s.audio_pause();
                    }
                }
            }
            #[cfg(not(any(windows, target_os = "linux")))]
            {
                let _ = rect;
            }
        }
        Cmd::Seek(t) => {
            // 音效游标重置:跳过 seek 点之前的所有事件。
            if let Some(s) = session.as_mut() {
                s.hs_cursor = s.hs_events.partition_point(|e| e.time <= t);
            }
            if let Some(s) = session.as_mut() {
                s.t = t.clamp(s.t0, s.duration);
                s.clock = Instant::now();
                s.dirty = true;
                if s.t < s.audio_offset {
                    // 拖回前奏段:停播等时间轴跨过 offset 再起(位置
                    // 钳 0 续播 = 前奏被快进,BGM 永久领先)。
                    s.audio_stop();
                    s.bgm_pending = s.playing;
                } else {
                    // 播放中和暂停中都把句柄跳到新位置:暂停中不跳的
                    // 话,恢复播放时 BGM 从旧位置续播,与时间轴脱钩。
                    s.bgm_pending = false;
                    if s.playing || s.bgm_handle.is_some() {
                        s.audio_seek();
                    }
                }
            }
        }
        Cmd::Play => {
            if let Some(s) = session.as_mut() {
                if s.t >= s.duration {
                    s.t = s.t0;
                    s.hs_cursor = 0;
                }
                s.playing = true;
                s.audio_play();
                // 时钟锚点放在音频起播之后:restart 路径要克隆整段解码
                // 缓冲(数十 MB memcpy)再入队,这些耗时不计入回放时
                // 间轴,否则时间轴先跑 δ、BGM 落后 δ,音效相对偏早。
                s.clock = Instant::now();
                let _ = s.app.emit(
                    "live-render-time",
                    LiveRenderState {
                        active: true,
                        playing: true,
                        time_ms: s.t,
                        duration_ms: s.duration,
                    },
                );
            }
        }
        Cmd::Pause => {
            if let Some(s) = session.as_mut() {
                s.playing = false;
                s.audio_pause();
                let _ = s.app.emit(
                    "live-render-time",
                    LiveRenderState {
                        active: true,
                        playing: false,
                        time_ms: s.t,
                        duration_ms: s.duration,
                    },
                );
            }
        }
        Cmd::SetOptions(options) => {
            let Some(s) = session.as_mut() else { return };
            // ---- 即时字段:下一帧生效 ----
            s.state.hud.ur_bar = options.ur_bar;
            s.state.hud.key_overlay = options.key_overlay;
            s.state.follow_points = options.follow_points;
            s.state.cursor_size = options.cursor_size;
            let offset_changed = s.audio_offset != options.audio_offset;
            s.audio_offset = options.audio_offset;

            // ---- 皮肤:目录变更 → 重载 + 重建图集 + 场景整体重建 ----
            // (legacy 精灵缓存/动画状态是皮肤的派生物,随皮肤作废;
            // 皮肤分支自身会带上当前 bg,故 bg 分支在皮肤未变时才走。)
            if options.skin_path != s.skin_path {
                let loaded =
                    skin::load_skin(options.skin_path.as_deref().map(std::path::Path::new));
                match loaded {
                    Ok(mut new_skin) => {
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
                        let with_bg = bg_image.is_some();
                        let (atlas, bold, semibold) = build_atlas(bg_image, &mut new_skin);
                        s.set_atlas(&atlas);
                        s.atlas = atlas;
                        s.bold = bold;
                        s.semibold = semibold;
                        s.skin = new_skin;
                        s.skin_path = options.skin_path.clone();
                        s.has_bg = with_bg;
                        let mut state = scene::SceneState::new(&s.game, RENDER_W, RENDER_H);
                        state.pro_skin = s.skin_path.is_none();
                        state.hud.ur_bar = options.ur_bar;
                        state.hud.key_overlay = options.key_overlay;
                        state.follow_points = options.follow_points;
                        state.cursor_size = options.cursor_size;
                        s.state = state;
                    }
                    Err(e) => {
                        eprintln!("live_render: 皮肤加载失败({e}),保留当前皮肤");
                        let _ = s.app.emit(
                            "live-render-skin-error",
                            format!("皮肤加载失败:{e}(已保留当前皮肤)"),
                        );
                    }
                }
            }

            // ---- 背景图:重建图集(字体 rect 补丁依赖图集)+ 热替换纹理 ----
            if options.bg != s.has_bg && options.skin_path == s.skin_path {
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
                // 皮肤纹理住在图集里,重建图集必须带上(按当前路径重载,
                // ResolvedSkin 不可 Clone;assign_regions 回填新区域)。
                match skin::load_skin(s.skin_path.as_deref().map(std::path::Path::new)) {
                    Ok(mut skin) => {
                        let (atlas, bold, semibold) = build_atlas(bg_image, &mut skin);
                        s.set_atlas(&atlas);
                        s.atlas = atlas;
                        s.bold = bold;
                        s.semibold = semibold;
                        s.skin = skin;
                        s.has_bg = options.bg;
                    }
                    Err(e) => eprintln!("live_render: 重建图集时皮肤加载失败({e})"),
                }
            }
            s.state.bg_opacity = if s.has_bg {
                Some(options.bg_opacity.clamp(0.0, 1.0))
            } else {
                None
            };

            // ---- 音频:开→懒解码(一次性);关→停播 ----
            if options.audio {
                let was_unloaded = s.bgm_data.is_none() && s.audio_path.is_some();
                if was_unloaded {
                    s.bgm_data = load_bgm(
                        s.audio_path.as_ref().unwrap(),
                        s.game.rate,
                        s.game.nightcore,
                        s.ffmpeg.as_deref(),
                    );
                }
                // 起播/重锚:解码刚就绪,或偏移热改后按新映射重新对位
                // (改回前奏段则停播待跨界)。已在播且偏移未变则不动。
                if s.bgm_data.is_some() && s.playing && (was_unloaded || offset_changed) {
                    if s.t < s.audio_offset {
                        s.audio_stop();
                        s.bgm_pending = true;
                    } else {
                        s.bgm_pending = false;
                        if was_unloaded || s.bgm_handle.is_none() {
                            s.audio_restart();
                        } else {
                            s.audio_seek();
                        }
                    }
                }
            } else {
                s.audio_stop();
                s.bgm_pending = false;
            }

            // ---- 音效:开→懒加载采样(事件表 open 时已建);关→停触发 ----
            s.hitsounds = options.hitsounds;
            if s.hitsounds {
                s.ensure_hitsounds();
            }
            s.dirty = true;
        }
        Cmd::Close => {
            if let Some(mut s) = session.take() {
                // Kira 的声音归混音器所有:丢弃句柄不会停播(切走页面/
                // 关闭预览后 BGM 一直响),必须显式 stop。
                s.audio_stop();
                s.backend.destroy(&s.app);
            }
        }
    }
}

fn open_session(
    app: &AppHandle,
    beatmap_path: &str,
    replay_path: &str,
    options: &LiveOptions,
    rect: PreviewRect,
    ffmpeg: Option<std::path::PathBuf>,
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
    // 皮肤:用户皮肤目录走 legacy 渲染(缺件回退 argon);None = 内置
    // Argon-Pro(无判定文字、滑条身体透明度 0.92)。
    let mut skin = skin::load_skin(options.skin_path.as_deref().map(std::path::Path::new))
        .map_err(|e| format!("皮肤加载失败: {e}"))?;
    let (atlas, bold, semibold) = build_atlas(bg_image, &mut skin);

    let t0 = game.snapshots.first().map(|s| s.time).unwrap_or(0.0);
    let duration = game.snapshots.last().map(|s| s.time).unwrap_or(0.0);
    let mut state = scene::SceneState::new(&game, RENDER_W, RENDER_H);
    state.pro_skin = options.skin_path.is_none();
    state.cursor_size = options.cursor_size;
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
    let bgm_data = if options.audio {
        audio_path
            .as_deref()
            .and_then(|p| load_bgm(p, game.rate, game.nightcore, ffmpeg.as_deref()))
    } else {
        None
    };
    if options.audio && bgm_data.is_none() {
        eprintln!("live_render: BGM 不可用(文件缺失或解码失败),静音播放");
    }

    // 音效事件表(lazer 语义:命中判定触发、谱面音量/bank、combobreak)。
    let hs_events = std::fs::read_to_string(beatmap_path)
        .ok()
        .map(|content| hitsound::collect_events(&game, &content))
        .unwrap_or_default();

    // ---- 原生直渲后端 --------------------------------------------------------
    // Windows:首选 WS_CHILD 子窗口(随父窗口移动,零跟踪,主线程创建),
    // 失败改用有属主的顶层弹出窗口(无线程限制,渲染线程轮询跟踪)。
    // Linux:X11 子窗口(主窗口 XID 之下,同样零跟踪)。失败直接报错。
    #[cfg(windows)]
    let backend = (|| -> Result<Backend, String> {
        let main_hwnd = app
            .get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as isize)
            .ok_or("无法获取主窗口句柄")?;
        let (x, y) = (rect.x.round() as i32, rect.y.round() as i32);
        let (w, h) = (rect.width.round() as i32, rect.height.round() as i32);
        let visible = rect.width > 0.0 && rect.height > 0.0;

        let (hwnd, popup) = match native::create_child_on_main(app, main_hwnd, x, y, w, h) {
            Ok(hwnd) => (hwnd, false),
            Err(e) => {
                eprintln!("live_render: 子窗口创建失败({e}),改用属主弹出窗口");
                let (ox, oy) = native::client_origin_on_screen(main_hwnd);
                let hwnd = native::create_popup(ox + x, oy + y, w, h)
                    .map_err(|e| format!("原生直渲不可用:无法创建预览窗口({e})"))?;
                native::set_owner(hwnd, main_hwnd);
                (hwnd, true)
            }
        };

        let mut renderer = surface::SurfaceRenderer::new(
            RENDER_W,
            RENDER_H,
            &atlas,
            osu_replay_render::raw_window_handle::RawDisplayHandle::Windows(
                osu_replay_render::raw_window_handle::WindowsDisplayHandle::new(),
            ),
            osu_replay_render::raw_window_handle::RawWindowHandle::Win32(
                osu_replay_render::raw_window_handle::Win32WindowHandle::new(
                    std::num::NonZeroIsize::new(hwnd).ok_or("无效的窗口句柄")?,
                ),
            ),
        )
        .map_err(|e| {
            if popup {
                unsafe { DestroyWindow(hwnd as HWND) };
            } else {
                native::destroy_child_on_main(app, hwnd);
            }
            format!("初始化渲染器失败: {e}")
        })?;
        if visible {
            renderer.resize(w.max(1) as u32, h.max(1) as u32);
        } else {
            renderer.resize(0, 0);
        }
        Ok(Backend::Native {
            hwnd,
            popup,
            main_hwnd,
            last_rect: rect,
            renderer,
            visible,
        })
    })();
    #[cfg(target_os = "linux")]
    let backend = (|| -> Result<Backend, String> {
        let main_xid = x11::Window::main_window_xid_on_main(app)?;
        let (x, y) = (rect.x.round() as i32, rect.y.round() as i32);
        let (w, h) = (rect.width.round() as i32, rect.height.round() as i32);
        let visible = rect.width > 0.0 && rect.height > 0.0;

        let window = x11::Window::new_child(main_xid, x, y, w, h)
            .map_err(|e| format!("无法创建 X11 预览子窗口: {e}"))?;
        let (raw_display, raw_window) = window.raw_handles();
        let mut renderer = match surface::SurfaceRenderer::new(
            RENDER_W,
            RENDER_H,
            &atlas,
            raw_display,
            raw_window,
        ) {
            Ok(r) => r,
            Err(e) => {
                window.destroy();
                return Err(format!("初始化渲染器失败: {e}"));
            }
        };
        if visible {
            renderer.resize(w.max(1) as u32, h.max(1) as u32);
        } else {
            renderer.resize(0, 0);
        }
        Ok(Backend::Native {
            renderer,
            visible,
            x11: window,
        })
    })();
    #[cfg(not(any(windows, target_os = "linux")))]
    let backend: Result<Backend, String> = Err("实时预览仅支持 Windows / Linux(X11)".into());
    let backend = backend?;

    let mut session = Session {
        app: app.clone(),
        beatmap_path: beatmap_path.to_string(),
        audio_path,
        ffmpeg,
        has_bg,
        skin,
        skin_path: options.skin_path.clone(),
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
        bgm_data,
        bgm_handle: None,
        bgm_pending: false,
        audio_offset: options.audio_offset,
        hs_events,
        hs_cursor: 0,
        hs_sounds: HashMap::new(),
        hitsounds: options.hitsounds,
    };
    if session.hitsounds {
        session.ensure_hitsounds();
    }
    Ok(session)
}

// ---- Tauri commands ----------------------------------------------------------

#[tauri::command(async)]
pub fn live_render_open(
    app: AppHandle,
    state: tauri::State<'_, crate::app::state::AppState>,
    beatmap_path: String,
    replay_path: String,
    rect: PreviewRect,
    options: LiveOptions,
) -> CommandResult<LiveOpenInfo> {
    // 皮肤引用（lazer:<resource_id>）在会话实际创建时物化。
    let options = resolve_lazer_skin(&state, options)?;
    // rect 已是物理 px(前端 × devicePixelRatio):WebKitGTK 的 X11 小数
    // 缩放(dpr 1.25)后端拿不到,见 PreviewRect 注释。
    // ffmpeg 路径在此解析一次存入会话(设置手动路径 > PATH > danser,
    // 与导出共用;缺省 = DT/HT 预览回退 kira 变调,见 load_bgm)。
    let ffmpeg = ffmpeg_path(&state);
    let (tx, rx) = channel();
    send(Cmd::Open {
        app,
        beatmap_path,
        replay_path,
        rect,
        options,
        ffmpeg,
        reply: tx,
    });
    rx.recv()
        .map_err(|_| CommandError::new("LIVE_RENDER", "渲染线程无响应"))?
        .map_err(|e| CommandError::new("LIVE_RENDER", e))
}

/// 预览区域位置(物理 px = CSS px × devicePixelRatio)。原生窗口跟随
/// 该 rect;无会话时 no-op。
#[tauri::command]
pub fn live_render_move(rect: PreviewRect) -> CommandResult<()> {
    send(Cmd::Move { rect });
    Ok(())
}

/// 渲染参数原地生效(零重载):即时字段改 SceneState;bg 重建图集并
/// 热替换纹理;audio 懒加载/静音。不触碰 wgpu 设备/窗口/判定数据。
#[tauri::command]
pub fn live_render_set_options(
    app: AppHandle,
    state: tauri::State<'_, crate::app::state::AppState>,
    options: LiveOptions,
) {
    // lazer 皮肤热切换 = 实际消费,此时物化;失败时回退内置皮肤并沿用
    // 皮肤错误事件通道提示,其余参数照常生效。
    let options = match resolve_lazer_skin(&state, options.clone()) {
        Ok(options) => options,
        Err(error) => {
            let mut fallback = options;
            fallback.skin_path = None;
            let _ = app.emit("live-render-skin-error", error.message);
            fallback
        }
    };
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

/// 一个可选的用户皮肤(客户端 Skins 目录下的子目录)。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSkinEntry {
    pub name: String,
    pub path: String,
}

/// 枚举客户端已安装的皮肤。stable 读 Skins 目录；lazer 的皮肤登记在
/// Realm 中，从本地索引列出并以 "lazer:<resource_id>" 作为引用路径，
/// 实际选用时才物化成目录（见 resolve_lazer_skin）。安装目录与数据
/// 目录下的 Skins/ 子目录会合并列出。
#[tauri::command]
pub fn live_render_list_skins(
    client: crate::local_analysis::LocalClient,
    state: tauri::State<'_, crate::app::state::AppState>,
) -> CommandResult<Vec<LiveSkinEntry>> {
    let status = state.local_analysis.source_status(client)?;
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for base in [status.install_root.as_deref(), status.data_root.as_deref()].into_iter().flatten() {
        for name in ["Skins", "skins"] {
            let p = std::path::Path::new(base).join(name);
            if p.is_dir() && !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(LiveSkinEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: path.display().to_string(),
                });
            }
        }
    }
    // Lazer：Realm 登记的皮肤（内容寻址存储，没有对应目录）。
    if client == crate::local_analysis::LocalClient::Lazer
        && let Ok(page) = state.local_analysis.query_skins(crate::local_analysis::SkinQuery {
            client,
            search: String::new(),
            sort: crate::local_analysis::SkinSort::Name,
            direction: crate::local_analysis::SortDirection::Asc,
            offset: 0,
            limit: 200,
        })
    {
        for skin in page.items {
            out.push(LiveSkinEntry {
                name: skin.name,
                path: format!("lazer:{}", skin.resource.resource_id),
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// 把 LiveOptions.skin_path 中的 lazer 引用（"lazer:<resource_id>"）解析
/// 为物化的皮肤目录——仅在皮肤真正被使用（打开预览 / 热切换 / 导出）
/// 时执行，列表阶段不产生任何文件复制。
fn resolve_lazer_skin(
    state: &crate::app::state::AppState,
    mut options: LiveOptions,
) -> CommandResult<LiveOptions> {
    let Some(reference) = options
        .skin_path
        .as_deref()
        .and_then(|path| path.strip_prefix("lazer:"))
    else {
        return Ok(options);
    };
    let directory = state.local_analysis.materialize_lazer_skin(reference)?;
    options.skin_path = Some(directory.display().to_string());
    Ok(options)
}
// NULTEST-MARKER-X

#[cfg(test)]
mod repro_tests {
    use super::*;

    /// 复现 "attempt to multiply with overflow":遍历本地 Replays × Songs
    /// (MD5 匹配),跑 open_session 的全部初始化段(判定/图集/场景/
    /// wgpu/音效事件),定位 panic 位置。BEATMAPS/REPLAYS 环境变量可覆盖目录。
    #[test]
    fn repro_init_overflow() {
        use md5::{Digest, Md5};
        let replay_dir = std::env::var("REPLAYS")
            .unwrap_or_else(|_| "/home/xiaobing/osu-test/osu!/Replays".into());
        let songs_dir =
            std::env::var("SONGS").unwrap_or_else(|_| "/home/xiaobing/osu-test/osu!/Songs".into());

        // .osr 头里的谱面 MD5(第 2 个字符串字段)。
        struct Osr<'a> {
            d: &'a [u8],
            off: usize,
        }
        impl Osr<'_> {
            fn u8(&mut self) -> u8 {
                let v = self.d[self.off];
                self.off += 1;
                v
            }
            fn u32(&mut self) -> u32 {
                let arr: [u8; 4] = self.d[self.off..self.off + 4].try_into().unwrap();
                self.off += 4;
                u32::from_le_bytes(arr)
            }
            fn s(&mut self) -> Option<String> {
                if self.u8() == 0 {
                    return Some(String::new());
                }
                let mut len = 0usize;
                let mut shift = 0;
                loop {
                    let b = self.d[self.off];
                    self.off += 1;
                    len |= ((b & 0x7f) as usize) << shift;
                    if b & 0x80 == 0 {
                        break;
                    }
                    shift += 7;
                }
                let v = self.d.get(self.off..self.off + len)?.to_vec();
                self.off += len;
                Some(String::from_utf8_lossy(&v).into_owned())
            }
        }
        let beatmap_hash = |data: &[u8]| -> Option<String> {
            let mut p = Osr { d: data, off: 0 };
            let _ = p.u8();
            let _ = p.u32();
            p.s()
        };

        let mut maps = HashMap::new();
        for entry in std::fs::read_dir(&songs_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            let dir = entry.path();
            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().and_then(|x| x.to_str()) == Some("osu") {
                    if let Ok(bytes) = std::fs::read(&p) {
                        let mut h = Md5::new();
                        h.update(&bytes);
                        maps.insert(format!("{:x}", h.finalize()), p);
                    }
                }
            }
        }

        let replays: Vec<_> = std::fs::read_dir(&replay_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("osr"))
            .collect();
        eprintln!(
            "replays: {}, md5-mapped beatmaps: {}",
            replays.len(),
            maps.len()
        );

        let mut ran = 0usize;
        for replay in replays {
            let Ok(bytes) = std::fs::read(&replay) else {
                continue;
            };
            let Some(hash) = beatmap_hash(&bytes) else {
                continue;
            };
            let Some(beatmap) = maps.get(&hash) else {
                continue;
            };
            ran += 1;
            eprintln!("[{ran}] {}", replay.display());

            // ---- open_session 各段(与 open_session 顺序一致) ----
            let game = game::load(beatmap.to_str().unwrap(), replay.to_str().unwrap()).unwrap();
            let mut skin = skin::load_skin(None).unwrap();
            let (atlas, _bold, _semibold) = build_atlas(None, &mut skin);
            let mut state = scene::SceneState::new(&game, 1280, 720);
            state.pro_skin = true;
            let content = std::fs::read_to_string(beatmap).unwrap();
            let _events = hitsound::collect_events(&game, &content);
            let mut renderer = Renderer::new(1280, 720, &atlas);
            let mut list = draw::DrawList::new();
            let t = game.snapshots.first().map(|s| s.time).unwrap_or(0.0);
            let snap = game::snapshot_at(&game, t);
            state.build_frame(
                &game,
                &scene::Assets {
                    atlas: &atlas,
                    bold: &_bold,
                    semibold: &_semibold,
                    skin: &skin,
                },
                &snap,
                &mut list,
            );
            list.finish();
            let _ = renderer.render(&list, CLEAR);
            // BGM 解码(kira/symphonia)也是 open 的一部分(audio 默认开)。
            if let Some(name) =
                osu_replay_render::osu_general_value(beatmap.to_str().unwrap(), "AudioFilename")
            {
                let audio = beatmap.parent().unwrap().join(name);
                if audio.exists() {
                    if let Some(bytes) = std::fs::read(&audio).ok() {
                        let _ = kira::sound::static_sound::StaticSoundData::from_cursor(
                            std::io::Cursor::new(bytes),
                        );
                    }
                }
            }
        }
        assert!(ran > 0, "没有匹配到任何回放");
    }
}

// ---- 视频导出(离屏渲染 → ffmpeg 管道 → mp4) --------------------------------

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportParams {
    pub out_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// "x264" | "x265" | "nvenc" | "hevc_nvenc"。
    pub encoder: String,
    /// crf(软件)/ cq(nvenc),默认 18。
    pub quality: u32,
    /// 混入 BGM(第二遍 ffmpeg:视频流拷贝 + AAC)。
    pub audio: bool,
    /// 混入音效轨(离线合成 ArgonPro 音效,与 BGM amix 求和)。
    #[serde(default = "default_true")]
    pub hitsounds: bool,
    /// 导出专用 BGM 偏移 ms(与实时预览的偏移互相独立:预览偏移含
    /// 输出设备延迟补偿,导出走文件混流不需要;默认 0)。
    #[serde(default)]
    pub audio_offset: f64,
}

fn default_true() -> bool {
    true
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

/// 检测 NVENC 硬件编码可用性(h264_nvenc / hevc_nvenc 各一次微型
/// 测试编码);返回 [h264 可用, hevc 可用]。无 FFmpeg = 均不可用。
#[tauri::command]
pub fn live_render_check_nvenc(
    state: tauri::State<'_, crate::app::state::AppState>,
) -> CommandResult<[bool; 2]> {
    let Some(path) = ffmpeg_path(&state) else {
        return Ok([false, false]);
    };
    let probe = |codec: &str| {
        std::process::Command::new(&path)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=256x256:d=0.04",
                "-c:v",
                codec,
                "-f",
                "null",
                "-",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|st| st.success())
            .unwrap_or(false)
    };
    Ok([probe("h264_nvenc"), probe("hevc_nvenc")])
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
    // 导出是实际消费点:lazer 皮肤引用在这里物化。
    let options = resolve_lazer_skin(&state, options)?;
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

/// 编码器选择 → (rawvideo 输入像素格式, ffmpeg 参数)。
/// NVENC 系列不接受带 alpha 的输入,统一喂 bgr0;x264/x265 走 bgra。
/// 未知编码器名回退 libx264(与旧版行为一致)。
fn ffmpeg_encoder_args(encoder: &str, quality: u32) -> (&'static str, Vec<String>) {
    let quality = quality.to_string();
    let mut args = match encoder {
        "nvenc" | "hevc_nvenc" => {
            let codec = if encoder == "hevc_nvenc" {
                "hevc_nvenc"
            } else {
                "h264_nvenc"
            };
            vec![
                "-c:v",
                codec,
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-rc",
                "vbr",
                "-cq",
                quality.as_str(),
                "-b:v",
                "0",
            ]
        }
        "x265" => {
            vec![
                "-c:v",
                "libx265",
                "-preset",
                "medium",
                "-crf",
                quality.as_str(),
                "-pix_fmt",
                "yuv420p",
            ]
        }
        _ => {
            vec![
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                quality.as_str(),
                "-pix_fmt",
                "yuv420p",
            ]
        }
    };
    if encoder == "hevc_nvenc" {
        // mp4 里 HEVC 默认 hev1 tag 在 QuickTime/部分系统播放器解不出,写 hvc1。
        args.push("-tag:v".into());
        args.push("hvc1".into());
    }
    args.push("-movflags".into());
    args.push("+faststart".into());
    let pix_fmt = if encoder == "nvenc" || encoder == "hevc_nvenc" {
        "bgr0"
    } else {
        "bgra"
    };
    let args = args.into_iter().map(|s| s.to_string()).collect();
    (pix_fmt, args)
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
    let mut skin = skin::load_skin(options.skin_path.as_deref().map(std::path::Path::new))
        .map_err(|e| format!("皮肤加载失败: {e}"))?;
    let (atlas, bold, semibold) = build_atlas(bg_image, &mut skin);

    let mut renderer = Renderer::new(params.width, params.height, &atlas);
    let mut state = scene::SceneState::new(&game, params.width, params.height);
    state.pro_skin = options.skin_path.is_none();
    state.cursor_size = options.cursor_size;
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
    let (in_pix_fmt, encode_args) = ffmpeg_encoder_args(&params.encoder, params.quality);
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
        skin: &skin,
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
    // 音量按 osu! 默认值(Music/Effect/Master 各 0.6,
    // `OsuGame.GetFrameworkConfigDefaults`),与 CLI 导出语义一致:
    // 通道 0.6 × 主音量 0.6。BGM 链处理 rate mod(DT/HT atempo 变速
    // 不变调、NC 保留 asetrate 变调)与负偏移静音,音效轨由
    // render_track_wav 在墙钟时间轴上离线合成(采样恒原速),直接 amix。
    let bgm_path = if params.audio {
        osu_replay_render::osu_general_value(beatmap_path, "AudioFilename")
            .map(|name| map_dir.join(name))
            .filter(|p| p.exists())
    } else {
        None
    };
    let hits_path = if params.hitsounds {
        std::fs::read_to_string(beatmap_path)
            .ok()
            .and_then(|content| {
                let wall_secs = frame_times.len() as f64 / params.fps as f64;
                let wav =
                    hitsound::render_track_wav(&game, &content, t0, wall_secs, game.rate, 0.6);
                let p = format!("{}.hits.wav", params.out_path);
                std::fs::write(&p, wav).ok().map(|_| p)
            })
    } else {
        None
    };

    if bgm_path.is_some() || hits_path.is_some() {
        emit("mux", exported, total, "混入音频…".into());
        let rate = game.rate;
        let seek_ms = t0 - params.audio_offset * rate;
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args(["-y", "-v", "error"]).arg("-i").arg(&tmp);
        let mut bgm_filters: Vec<String> = vec!["volume=0.6000".into()];
        if let Some(bgm) = &bgm_path {
            if seek_ms >= 0.0 {
                cmd.arg("-ss").arg(format!("{:.3}", seek_ms / 1000.0));
            } else {
                // 负文件位置:前置静音补齐。delay 取文件(谱面)时间轴毫秒、
                // 且必须位于 rate 滤镜之前——rate 滤波会把它压缩成
                // -seek/rate 墙钟毫秒;atempo 放在 adelay 之前会在
                // ffmpeg 9 的 -shortest 下卡死图同步、音频只剩毫秒级。
                bgm_filters.push(format!("adelay={}:all=1", (-seek_ms).round() as i64));
            }
            if (rate - 1.0).abs() > 1e-9 {
                if game.nightcore {
                    // NC 保留游戏内升调(asetrate 变速变调,nightcore 的
                    // 本体),与 CLI 导出语义一致。
                    let sr = probe_sample_rate(ffmpeg, bgm);
                    bgm_filters.push(format!(
                        "asetrate={sr},aresample={sr}",
                        sr = (sr as f64 * rate).round() as i64
                    ));
                } else {
                    // DT/HT 变速不变调(atempo 只压时长、保音调)。
                    bgm_filters.push(format!("atempo={:.6}", rate));
                }
            }
            cmd.arg("-i").arg(bgm);
        }
        match (&bgm_path, &hits_path) {
            (Some(_), Some(hits)) => {
                cmd.arg("-i").arg(hits);
                cmd.arg("-filter_complex").arg(format!(
                    "[1:a]{}[bgm];[bgm][2:a]amix=inputs=2:normalize=0,volume=0.6000[aout]",
                    bgm_filters.join(",")
                ));
                cmd.args(["-map", "0:v", "-map", "[aout]"]);
            }
            (Some(_), None) => {
                cmd.arg("-af")
                    .arg(format!("{},volume=0.6000", bgm_filters.join(",")));
                cmd.args(["-map", "0:v", "-map", "1:a"]);
            }
            (None, Some(hits)) => {
                cmd.arg("-i").arg(hits);
                cmd.arg("-af").arg("volume=0.6000");
                cmd.args(["-map", "0:v", "-map", "1:a"]);
            }
            (None, None) => unreachable!(),
        }
        let status = cmd
            .args([
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
        if let Some(hits) = &hits_path {
            let _ = std::fs::remove_file(hits);
        }
        if !status.success() {
            return Err("音频混流失败".into());
        }
    } else {
        std::fs::rename(&tmp, &params.out_path).map_err(|e| format!("无法写出文件: {e}"))?;
    }

    emit("done", exported, total, params.out_path.clone());
    Ok(params.out_path.clone())
}

/// 首个音频流的采样率(仅 NC 分支的 asetrate 变调用)。优先取 ffmpeg
/// 同目录的 ffprobe(自定义路径/danser 包),否则 PATH;失败回退 44100。
fn probe_sample_rate(ffmpeg: &std::path::Path, media: &std::path::Path) -> u32 {
    let sibling = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let probe = if sibling.is_file() {
        sibling
    } else {
        std::path::PathBuf::from("ffprobe")
    };
    std::process::Command::new(&probe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(media)
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(44_100)
}

#[cfg(test)]
mod tests {
    use super::ffmpeg_encoder_args;

    fn joined(args: &[String]) -> String {
        args.join(" ")
    }

    #[test]
    fn software_encoders_use_crf_and_bgra() {
        let (pix_fmt, args) = ffmpeg_encoder_args("x264", 18);
        assert_eq!(pix_fmt, "bgra");
        assert_eq!(
            joined(&args),
            "-c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p -movflags +faststart"
        );
        let (pix_fmt, args) = ffmpeg_encoder_args("x265", 22);
        assert_eq!(pix_fmt, "bgra");
        assert!(joined(&args).starts_with("-c:v libx265 -preset medium -crf 22"));
    }

    #[test]
    fn nvenc_h264_uses_cq_and_bgr0() {
        let (pix_fmt, args) = ffmpeg_encoder_args("nvenc", 19);
        assert_eq!(pix_fmt, "bgr0");
        assert_eq!(
            joined(&args),
            "-c:v h264_nvenc -preset p5 -tune hq -rc vbr -cq 19 -b:v 0 -movflags +faststart"
        );
    }

    #[test]
    fn nvenc_he264_hardware_maps_to_hevc_nvenc_with_hvc1_tag() {
        let (pix_fmt, args) = ffmpeg_encoder_args("hevc_nvenc", 20);
        assert_eq!(pix_fmt, "bgr0");
        assert_eq!(
            joined(&args),
            "-c:v hevc_nvenc -preset p5 -tune hq -rc vbr -cq 20 -b:v 0 -tag:v hvc1 -movflags +faststart"
        );
    }

    #[test]
    fn unknown_encoder_falls_back_to_libx264() {
        let (pix_fmt, args) = ffmpeg_encoder_args("nope", 18);
        assert_eq!(pix_fmt, "bgra");
        assert!(joined(&args).contains("libx264"));
    }
}
