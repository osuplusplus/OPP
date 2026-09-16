use super::models::{Control, PlayerState, TrackInfo};
use souvlaki::{
    MediaControlEvent as Event, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
    PlatformConfig, SeekDirection,
};
use std::time::Duration;
use tauri::Manager;

pub(crate) fn attach(
    app: tauri::AppHandle,
    hwnd: Option<usize>,
    send: impl Fn(Control) + Send + 'static,
) -> Option<MediaControls> {
    let result = (|| {
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
        #[cfg(target_os = "linux")]
        if let Err(error) = controls.set_volume(f64::from(state.volume)) {
            crate::log_warn!("music_player", "更新系统音量失败：{}", error);
        }
    }
}
