//! X11 原生子窗口(Linux 直渲宿主)

use osu_replay_render::raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, XlibDisplayHandle, XlibWindowHandle,
};
use std::ptr::NonNull;
use x11_dl::xlib;

pub struct Window {
    xlib: xlib::Xlib,
    display: *mut xlib::Display,
    xid: xlib::Window,
}

// display 指针只在渲染线程(创建/使用/销毁均同一线程)触碰。
unsafe impl Send for Window {}

impl Window {
    /// 主线程上取主窗口 XID(tao 的 window_handle 读 GTK 对象,按 GTK
    /// 线程规则须在主线程调用)。
    pub fn main_window_xid_on_main(app: &tauri::AppHandle) -> Result<xlib::Window, String> {
        use osu_replay_render::raw_window_handle::HasWindowHandle;
        use tauri::Manager;
        let (tx, rx) = std::sync::mpsc::channel();
        let app_in = app.clone();
        app.run_on_main_thread(move || {
            // 只回传 XID(c_ulong):RawWindowHandle 部分 variant 含
            // NonNull,不满足 Send。
            let res = match app_in.get_webview_window("main") {
                None => Err("找不到主窗口".to_string()),
                Some(w) => match w.window_handle() {
                    Err(_) => Err("无法获取主窗口原生句柄".to_string()),
                    Ok(h) => match h.as_raw() {
                        RawWindowHandle::Xlib(h) => Ok(h.window),
                        RawWindowHandle::Xcb(h) => Ok(h.window.get() as xlib::Window),
                        other => Err(format!(
                            "主窗口不是 X11 窗口({other:?}):原生直渲需要 X11,请以 GDK_BACKEND=x11 启动"
                        )),
                    },
                },
            };
            let _ = tx.send(res);
        })
        .map_err(|e| format!("无法调度到主线程: {e}"))?;
        rx.recv().map_err(|_| "主线程无响应".to_string())?
    }

    /// 在 `parent` 下创建子窗口(渲染线程调用)。
    pub fn new_child(
        parent: xlib::Window,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Result<Window, String> {
        let xlib = xlib::Xlib::open().map_err(|e| format!("无法加载 libX11: {e}"))?;
        unsafe {
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return Err("XOpenDisplay 失败(无 X11/XWayland 会话?)".into());
            }
            let screen = (xlib.XDefaultScreen)(display);
            let background = (xlib.XBlackPixel)(display, screen);
            let xid = (xlib.XCreateSimpleWindow)(
                display,
                parent,
                x,
                y,
                w.max(1) as u32,
                h.max(1) as u32,
                0,
                0,
                background,
            );
            if xid == 0 {
                (xlib.XCloseDisplay)(display);
                return Err("XCreateSimpleWindow 失败".into());
            }
            // 不选任何事件:输入穿透 + 后续无需读事件队列。
            (xlib.XSelectInput)(display, xid, 0);
            (xlib.XMapWindow)(display, xid);
            // XSync(而非 XFlush):必须等服务器真正建好/定位本窗口,
            // wgpu 的 EGL 走另一条 X 连接,之后创建 surface 时要读它的
            // 尺寸——跨连接无隐式顺序保证,提前创建会拿到旧尺寸,
            // 呈现的画面就在窗口里错位/比例不稳。
            (xlib.XSync)(display, 0);
            Ok(Window { xlib, display, xid })
        }
    }

    /// 定位/显隐。父窗口局部坐标。XSync:确保服务器已应用新几何,
    /// 渲染线程随后重配置 EGL surface(另一条连接)才不会拿到旧尺寸。
    pub fn place(&mut self, x: i32, y: i32, w: i32, h: i32, visible: bool) {
        unsafe {
            if visible {
                (self.xlib.XMoveResizeWindow)(
                    self.display,
                    self.xid,
                    x,
                    y,
                    w.max(1) as u32,
                    h.max(1) as u32,
                );
                (self.xlib.XMapWindow)(self.display, self.xid);
            } else {
                (self.xlib.XUnmapWindow)(self.display, self.xid);
            }
            (self.xlib.XSync)(self.display, 0);
        }
    }

    /// 压回兄弟栈顶(WebView 的 GdkWindow 之上),对抗 GDK 重排。
    pub fn bring_to_top(&mut self) {
        unsafe {
            (self.xlib.XRaiseWindow)(self.display, self.xid);
            (self.xlib.XFlush)(self.display);
        }
    }

    /// wgpu surface 所需的 raw handle(display + window)。
    pub fn raw_handles(&self) -> (RawDisplayHandle, RawWindowHandle) {
        let screen = unsafe { (self.xlib.XDefaultScreen)(self.display) };
        let display = XlibDisplayHandle::new(NonNull::new(self.display.cast()), screen);
        let window = XlibWindowHandle::new(self.xid);
        (
            RawDisplayHandle::Xlib(display),
            RawWindowHandle::Xlib(window),
        )
    }

    pub fn destroy(self) {
        unsafe {
            (self.xlib.XDestroyWindow)(self.display, self.xid);
            // XCloseDisplay 自带 flush,关闭后不得再触碰 display。
            (self.xlib.XCloseDisplay)(self.display);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 冒烟:自有父窗口 → 子窗口 → wgpu surface → blit 渲染一帧。
    /// 无 DISPLAY 时跳过(headless CI)。
    #[test]
    fn child_window_wgpu_surface_smoke() {
        if std::env::var_os("DISPLAY").is_none() {
            eprintln!("skip: no DISPLAY");
            return;
        }
        let xlib = match xlib::Xlib::open() {
            Ok(x) => x,
            Err(_) => {
                eprintln!("skip: no libX11");
                return;
            }
        };
        unsafe {
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                eprintln!("skip: XOpenDisplay failed");
                return;
            }
            let screen = (xlib.XDefaultScreen)(display);
            let root = (xlib.XDefaultRootWindow)(display);
            let white = (xlib.XWhitePixel)(display, screen);
            let parent = (xlib.XCreateSimpleWindow)(display, root, 0, 0, 320, 180, 0, 0, white);
            (xlib.XMapWindow)(display, parent);
            // XSync(而非 XFlush):等服务器真正建好父窗口,另一条连接
            // 才能以它为 parent 建子窗口(跨连接无隐式顺序保证)。
            (xlib.XSync)(display, xlib::False);
            let child = Window::new_child(parent, 10, 10, 300, 160).expect("child window");
            (xlib.XFlush)(display);

            let mut skin = osu_replay_render::skin::load_skin(None).unwrap();
            let (atlas, _fonts) =
                osu_replay_render::build_atlas(None, None, None, &mut skin, 8192, None);
            let (rd, rw) = child.raw_handles();
            let mut renderer =
                osu_replay_render::surface::SurfaceRenderer::new(1280, 720, &atlas, rd, rw)
                    .expect("surface renderer");
            renderer.resize(300, 160);
            let mut list = osu_replay_render::draw::DrawList::new();
            list.finish();
            assert!(renderer.render(&list, [0.0, 0.0, 0.0, 1.0]));

            drop(renderer); // surface 释放后才能销毁窗口
            child.destroy();
            (xlib.XDestroyWindow)(display, parent);
            (xlib.XCloseDisplay)(display);
        }
    }
}
