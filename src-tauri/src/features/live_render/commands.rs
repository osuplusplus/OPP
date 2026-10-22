//! Tauri 命令层：从 mod.rs 拆分，命令名与序列化契约不变。

use super::*;
use crate::error::{CommandError, CommandResult};
use tauri::AppHandle;

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
    state: tauri::State<'_, crate::state::AppState>,
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

/// 跳转预览时间(ms,渲染线程钳制到有效范围;BGM/音效游标随动)。
#[tauri::command]
pub fn live_render_seek(time_ms: f64) {
    send(Cmd::Seek(time_ms));
}

/// 从当前位置继续播放(播到结尾后再播放则回到开头重播)。
#[tauri::command]
pub fn live_render_play() {
    send(Cmd::Play);
}

/// 暂停播放(BGM 与循环音效一并暂停)。
#[tauri::command]
pub fn live_render_pause() {
    send(Cmd::Pause);
}

/// 关闭预览会话;同步等待渲染线程完成清理(停播 + 销毁原生窗口)。
#[tauri::command]
pub fn live_render_close() {
    let (tx, rx) = channel();
    send(Cmd::Close { reply: tx });
    let _ = rx.recv();
}

/// 枚举客户端已安装的皮肤。stable 读 Skins 目录；lazer 的皮肤登记在
/// Realm 中，从本地索引列出并以 "lazer:<resource_id>" 作为引用路径，
/// 实际选用时才物化成目录（见 resolve_lazer_skin）。安装目录与数据
/// 目录下的 Skins/ 子目录会合并列出。
#[tauri::command]
pub async fn live_render_list_skins(
    client: crate::features::local_analysis::LocalClient,
    app: AppHandle,
) -> CommandResult<Vec<LiveSkinEntry>> {
    crate::infrastructure::tasks::blocking_io("live_render_list_skins", move || {
        live_render_list_skins_blocking(client, app.state())
    })
    .await?
}

/// 当前生效的 FFmpeg 信息(路径 + 版本首行 + 解析来源),设置页展示用。
#[tauri::command]
pub async fn live_render_get_ffmpeg_status(app: AppHandle) -> CommandResult<FfmpegStatus> {
    crate::infrastructure::tasks::blocking_io("live_render_get_ffmpeg_status", move || {
        live_render_get_ffmpeg_status_blocking(app.state())
    })
    .await
}

/// 检测 NVENC 硬件编码可用性(h264_nvenc / hevc_nvenc 各一次微型
/// 测试编码);返回 [h264 可用, hevc 可用]。无 FFmpeg = 均不可用。
#[tauri::command]
pub async fn live_render_check_nvenc(app: AppHandle) -> CommandResult<[bool; 2]> {
    crate::infrastructure::tasks::blocking_io("live_render_check_nvenc", move || {
        live_render_check_nvenc_blocking(app.state())
    })
    .await?
}

/// 检测用户 FFmpeg(PATH 自动检测优先,danser 发行包兜底);
/// 返回版本首行(None = 未找到)。
#[tauri::command]
pub async fn live_render_check_ffmpeg(app: AppHandle) -> CommandResult<Option<String>> {
    crate::infrastructure::tasks::blocking_io("live_render_check_ffmpeg", move || {
        live_render_check_ffmpeg_blocking(app.state())
    })
    .await?
}

/// 请求取消进行中的导出(导出线程逐帧轮询标志,下一帧生效)。
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
    crate::infrastructure::platform::reveal_path(&path)
        .map_err(|e| CommandError::new("LIVE_RENDER", format!("无法打开所在文件夹: {e}")))
}
