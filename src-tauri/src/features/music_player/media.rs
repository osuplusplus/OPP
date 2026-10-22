//! 系统媒体控制集成：借助 `souvlaki` 把曲目信息与播放状态同步到系统面板
//! （Windows 为 SMTC，Linux 为 MPRIS），并把媒体键/面板事件
//! 转换回 `Control` 指令送入播放服务。
use super::models::{Control, PlayerState, TrackInfo};
use souvlaki::{
    MediaControlEvent as Event, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
    PlatformConfig, SeekDirection,
};
use std::time::Duration;
use tauri::Manager;

/// 注册系统媒体控制并返回句柄；注册失败仅记日志并返回 `None`，
/// 没有系统集成时播放器照常工作。
///
/// `hwnd` 是 Windows SMTC 要求的宿主窗口句柄（其他平台忽略）；
/// `Raise` 事件唤起当前活动窗口，`Quit` 事件直接退出应用。
pub(crate) fn attach(
    app: tauri::AppHandle,
    hwnd: Option<usize>,
    send: impl Fn(Control) + Send + 'static,
) -> Option<MediaControls> {
    let result = (|| {
        // dbus_name/display_name 仅在 Linux MPRIS 上生效。
        let mut controls = MediaControls::new(PlatformConfig {
            dbus_name: "opp",
            display_name: "OPP 本地音乐",
            hwnd: hwnd.map(|h| h as *mut std::ffi::c_void),
        })?;
        controls.attach(move |event| {
            let position = || {
                app.state::<crate::state::AppState>()
                    .music
                    .snapshot()
                    .map(|s| s.position)
                    .unwrap_or(0.0)
            };
            let action = match event {
                Event::Play => Control::Play,
                Event::Pause | Event::Stop => Control::Pause,
                Event::Toggle => Control::Toggle,
                Event::Next => Control::Next,
                Event::Previous => Control::Previous,
                Event::SetPosition(position) => Control::Seek {
                    seconds: position.0.as_secs_f64(),
                },
                Event::SetVolume(value) => Control::Volume {
                    value: value as f32,
                },
                // 相对快进/快退按固定 5 秒换算为绝对跳转。
                Event::Seek(direction) => Control::Seek {
                    seconds: (position()
                        + if direction == SeekDirection::Forward {
                            5.0
                        } else {
                            -5.0
                        })
                    .max(0.0),
                },
                Event::SeekBy(direction, delta) => Control::Seek {
                    seconds: (position()
                        + if direction == SeekDirection::Forward {
                            delta.as_secs_f64()
                        } else {
                            -delta.as_secs_f64()
                        })
                    .max(0.0),
                },
                Event::Raise => {
                    super::windows::show_active(&app);
                    return;
                }
                Event::Quit => {
                    app.exit(0);
                    return;
                }
                _ => return,
            };
            send(action);
        })?;
        Ok::<_, souvlaki::Error>(controls)
    })();
    match result {
        Ok(controls) => Some(controls),
        Err(error) => {
            crate::log_warn!("music_player", "系统媒体控制不可用：{}", error);
            None
        }
    }
}
/// 更新系统面板中的曲目元数据；时长未知（0）时不向系统提供。
pub(crate) fn metadata(controls: &mut Option<MediaControls>, track: &TrackInfo, duration: f64) {
    if let Some(controls) = controls
        && let Err(error) = controls.set_metadata(MediaMetadata {
            title: Some(&track.title),
            artist: Some(&track.artist),
            album: Some("OPP 本地谱库"),
            duration: (duration > 0.0).then(|| Duration::from_secs_f64(duration)),
            ..Default::default()
        })
    {
        crate::log_warn!("music_player", "更新媒体信息失败：{}", error);
    }
}
/// 同步播放/暂停/停止状态与进度到系统面板；无当前曲目时报告停止。
pub(crate) fn playback(controls: &mut Option<MediaControls>, state: &PlayerState) {
    if let Some(controls) = controls {
        let progress = Some(MediaPosition(Duration::from_secs_f64(
            state.position.max(0.0),
        )));
        let playback = if state.current.is_none() {
            MediaPlayback::Stopped
        } else if state.playing {
            MediaPlayback::Playing { progress }
        } else {
            MediaPlayback::Paused { progress }
        };
        if let Err(error) = controls.set_playback(playback) {
            crate::log_warn!("music_player", "更新媒体状态失败：{}", error);
        }
        // 仅 Linux MPRIS 支持把音量同步到系统面板。
        #[cfg(target_os = "linux")]
        if let Err(error) = controls.set_volume(f64::from(state.volume)) {
            crate::log_warn!("music_player", "更新系统音量失败：{}", error);
        }
    }
}
