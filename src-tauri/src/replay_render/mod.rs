use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use rosu_render::{
    OrdrClient, OrdrWebsocket,
    model::{Event, RenderOptions, RenderResolution, RenderSkinOption, Verification},
};

use crate::{
    error::{CommandError, CommandResult},
    game_session::{load_game_replay_file, parse_replay_metadata},
    local_analysis::LocalClient,
    models::Ruleset,
    state::AppState,
};

mod events;
use events::ReplayRenderProgress;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderSkinKind {
    Official,
    Custom,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderDeveloperMode {
    Success,
    ApiFailure,
    WebsocketFailure,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReplayRenderOptions {
    pub resolution: String,
    pub global_volume: u8,
    pub music_volume: u8,
    pub hitsound_volume: u8,
    pub show_hit_error_meter: bool,
    pub show_unstable_rate: bool,
    pub show_score: bool,
    pub show_hp_bar: bool,
    pub show_combo_counter: bool,
    pub show_pp_counter: bool,
    pub show_scoreboard: bool,
    pub show_borders: bool,
    pub show_mods: bool,
    pub show_result_screen: bool,
    pub show_hit_counter: bool,
    pub show_key_overlay: bool,
    pub show_avatars_on_scoreboard: bool,
    pub show_aim_error_meter: bool,
    pub show_strain_graph: bool,
    pub show_slider_breaks: bool,
    pub use_skin_cursor: bool,
    pub use_skin_colors: bool,
    pub use_skin_hitsounds: bool,
    pub use_beatmap_colors: bool,
    pub cursor_rainbow: bool,
    pub cursor_trail: bool,
    pub cursor_trail_glow: bool,
    pub cursor_ripples: bool,
    pub cursor_size: f32,
    pub draw_follow_points: bool,
    pub draw_combo_numbers: bool,
    pub slider_snaking_in: bool,
    pub slider_snaking_out: bool,
    pub slider_merge: bool,
    pub objects_rainbow: bool,
    pub flash_objects: bool,
    pub use_slider_hitcircle_color: bool,
    pub beat_scaling: bool,
    pub seizure_warning: bool,
    pub load_storyboard: bool,
    pub load_video: bool,
    pub intro_bg_dim: u8,
    pub ingame_bg_dim: u8,
    pub break_bg_dim: u8,
    pub bg_parallax: bool,
    pub show_danser_logo: bool,
    pub skip_intro: bool,
    pub play_nightcore_samples: bool,
    pub ignore_fail: bool,
}

impl Default for ReplayRenderOptions {
    fn default() -> Self {
        let o = RenderOptions::default();
        Self {
            resolution: o.resolution.as_str().into(),
            global_volume: o.global_volume,
            music_volume: o.music_volume,
            hitsound_volume: o.hitsound_volume,
            show_hit_error_meter: o.show_hit_error_meter,
            show_unstable_rate: o.show_unstable_rate,
            show_score: o.show_score,
            show_hp_bar: o.show_hp_bar,
            show_combo_counter: o.show_combo_counter,
            show_pp_counter: o.show_pp_counter,
            show_scoreboard: o.show_scoreboard,
            show_borders: o.show_borders,
            show_mods: o.show_mods,
            show_result_screen: o.show_result_screen,
            show_hit_counter: o.show_hit_counter,
            show_key_overlay: o.show_key_overlay,
            show_avatars_on_scoreboard: o.show_avatars_on_scoreboard,
            show_aim_error_meter: o.show_aim_error_meter,
            show_strain_graph: o.show_strain_graph,
            show_slider_breaks: o.show_slider_breaks,
            use_skin_cursor: o.use_skin_cursor,
            use_skin_colors: o.use_skin_colors,
            use_skin_hitsounds: o.use_skin_hitsounds,
            use_beatmap_colors: o.use_beatmap_colors,
            cursor_rainbow: o.cursor_rainbow,
            cursor_trail: o.cursor_trail,
            cursor_trail_glow: o.cursor_trail_glow,
            cursor_ripples: o.cursor_ripples,
            cursor_size: o.cursor_size,
            draw_follow_points: o.draw_follow_points,
            draw_combo_numbers: o.draw_combo_numbers,
            slider_snaking_in: o.slider_snaking_in,
            slider_snaking_out: o.slider_snaking_out,
            slider_merge: o.slider_merge,
            objects_rainbow: o.objects_rainbow,
            flash_objects: o.flash_objects,
            use_slider_hitcircle_color: o.use_slider_hitcircle_color,
            beat_scaling: o.beat_scaling,
            seizure_warning: o.seizure_warning,
            load_storyboard: o.load_storyboard,
            load_video: o.load_video,
            intro_bg_dim: o.intro_bg_dim,
            ingame_bg_dim: o.ingame_bg_dim,
            break_bg_dim: o.break_bg_dim,
            bg_parallax: o.bg_parallax,
            show_danser_logo: o.show_danser_logo,
            skip_intro: o.skip_intro,
            play_nightcore_samples: o.play_nightcore_samples,
            ignore_fail: o.ignore_fail,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplayRenderRequest {
    pub client: LocalClient,
    pub replay_path: String,
    pub username: String,
    #[serde(default)]
    pub options: ReplayRenderOptions,
    #[serde(default = "default_skin_kind")]
    pub skin_kind: RenderSkinKind,
    #[serde(default = "default_skin")]
    pub skin: String,
    pub verification_key: Option<String>,
    pub developer_mode: Option<RenderDeveloperMode>,
}

fn default_skin_kind() -> RenderSkinKind {
    RenderSkinKind::Official
}
fn default_skin() -> String {
    "default".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayRenderJob {
    pub render_id: u32,
    pub status: String,
    pub description: String,
}

fn options_from(input: ReplayRenderOptions) -> CommandResult<RenderOptions> {
    let resolution = match input.resolution.as_str() {
        "720x480" => RenderResolution::SD480,
        "960x540" => RenderResolution::SD960,
        "1280x720" => RenderResolution::HD720,
        "1920x1080" => RenderResolution::HD1080,
        _ => {
            return Err(CommandError::new(
                "INVALID_RENDER_RESOLUTION",
                "不支持的视频分辨率",
            ));
        }
    };
    if !(0.5..=2.0).contains(&input.cursor_size) {
        return Err(CommandError::new(
            "INVALID_CURSOR_SIZE",
            "光标大小必须在 0.5 到 2.0 之间",
        ));
    }
    for value in [
        input.global_volume,
        input.music_volume,
        input.hitsound_volume,
        input.intro_bg_dim,
        input.ingame_bg_dim,
        input.break_bg_dim,
    ] {
        if value > 100 {
            return Err(CommandError::new(
                "INVALID_RENDER_OPTION",
                "音量和背景暗度必须在 0 到 100 之间",
            ));
        }
    }
    Ok(RenderOptions {
        resolution,
        global_volume: input.global_volume,
        music_volume: input.music_volume,
        hitsound_volume: input.hitsound_volume,
        show_hit_error_meter: input.show_hit_error_meter,
        show_unstable_rate: input.show_unstable_rate,
        show_score: input.show_score,
        show_hp_bar: input.show_hp_bar,
        show_combo_counter: input.show_combo_counter,
        show_pp_counter: input.show_pp_counter,
        show_scoreboard: input.show_scoreboard,
        show_borders: input.show_borders,
        show_mods: input.show_mods,
        show_result_screen: input.show_result_screen,
        show_hit_counter: input.show_hit_counter,
        show_key_overlay: input.show_key_overlay,
        show_avatars_on_scoreboard: input.show_avatars_on_scoreboard,
        show_aim_error_meter: input.show_aim_error_meter,
        show_strain_graph: input.show_strain_graph,
        show_slider_breaks: input.show_slider_breaks,
        use_skin_cursor: input.use_skin_cursor,
        use_skin_colors: input.use_skin_colors,
        use_skin_hitsounds: input.use_skin_hitsounds,
        use_beatmap_colors: input.use_beatmap_colors,
        cursor_scale_to_cs: false,
        cursor_rainbow: input.cursor_rainbow,
        cursor_trail_glow: input.cursor_trail_glow,
        draw_follow_points: input.draw_follow_points,
        draw_combo_numbers: input.draw_combo_numbers,
        cursor_size: input.cursor_size,
        cursor_trail: input.cursor_trail,
        beat_scaling: input.beat_scaling,
        slider_merge: input.slider_merge,
        objects_rainbow: input.objects_rainbow,
        flash_objects: input.flash_objects,
        use_slider_hitcircle_color: input.use_slider_hitcircle_color,
        seizure_warning: input.seizure_warning,
        load_storyboard: input.load_storyboard,
        load_video: input.load_video,
        intro_bg_dim: input.intro_bg_dim,
        ingame_bg_dim: input.ingame_bg_dim,
        break_bg_dim: input.break_bg_dim,
        bg_parallax: input.bg_parallax,
        show_danser_logo: input.show_danser_logo,
        skip_intro: input.skip_intro,
        cursor_ripples: input.cursor_ripples,
        slider_snaking_in: input.slider_snaking_in,
        slider_snaking_out: input.slider_snaking_out,
        play_nightcore_samples: input.play_nightcore_samples,
        ignore_fail: input.ignore_fail,
        discord_user_id: None,
    })
}

fn verification(request: &ReplayRenderRequest) -> CommandResult<Option<Verification>> {
    if request.developer_mode.is_some()
        && request
            .verification_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
    {
        return Err(CommandError::new(
            "INVALID_RENDER_AUTH",
            "验证密钥与开发者模拟模式不能同时使用",
        ));
    }
    Ok(match request.developer_mode {
        Some(RenderDeveloperMode::Success) => Some(Verification::DevModeSuccess),
        Some(RenderDeveloperMode::ApiFailure) => Some(Verification::DevModeFail),
        Some(RenderDeveloperMode::WebsocketFailure) => Some(Verification::DevModeWsFail),
        None => request
            .verification_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(|key| Verification::Key(key.into())),
    })
}

fn emit(app: &AppHandle, event: ReplayRenderProgress) {
    let _ = app.emit("ordr-render-progress", event);
}

fn monitor_render(app: AppHandle, render_id: u32) {
    tauri::async_runtime::spawn(async move {
        let mut socket = match OrdrWebsocket::connect().await {
            Ok(socket) => socket,
            Err(error) => {
                emit(
                    &app,
                    ReplayRenderProgress {
                        render_id,
                        status: "monitor_error".into(),
                        description: format!("无法连接 o!rdr 进度通道：{error}"),
                        video_url: None,
                    },
                );
                return;
            }
        };
        loop {
            let raw = match socket.next_event().await {
                Ok(event) => event,
                Err(error) => {
                    emit(
                        &app,
                        ReplayRenderProgress {
                            render_id,
                            status: "monitor_error".into(),
                            description: format!("进度通道中断：{error}"),
                            video_url: None,
                        },
                    );
                    return;
                }
            };
            let Ok(event) = raw.deserialize() else {
                continue;
            };
            match event {
                Event::RenderProgress(value) if value.render_id == render_id => emit(
                    &app,
                    ReplayRenderProgress {
                        render_id,
                        status: "rendering".into(),
                        description: format!("{} · {}", value.progress, value.description),
                        video_url: None,
                    },
                ),
                Event::RenderDone(value) if value.render_id == render_id => {
                    emit(
                        &app,
                        ReplayRenderProgress {
                            render_id,
                            status: "completed".into(),
                            description: "视频已生成，可打开或复制链接".into(),
                            video_url: Some(value.video_url.into()),
                        },
                    );
                    let _ = socket.disconnect().await;
                    return;
                }
                Event::RenderFailed(value) if value.render_id == render_id => {
                    emit(
                        &app,
                        ReplayRenderProgress {
                            render_id,
                            status: "failed".into(),
                            description: value.error_message.into(),
                            video_url: None,
                        },
                    );
                    let _ = socket.disconnect().await;
                    return;
                }
                _ => {}
            }
        }
    });
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：提交异步渲染任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn submit_replay_render(
    request: ReplayRenderRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ReplayRenderJob> {
    let username = request.username.trim();
    if username.is_empty() || username.chars().count() > 32 {
        return Err(CommandError::new(
            "INVALID_RENDER_USERNAME",
            "渲染署名需为 1 至 32 个字符",
        ));
    }
    let replay = load_game_replay_file(request.client, &request.replay_path, &state)?;
    let (beatmap_hash, _) = parse_replay_metadata(&replay)?;
    let beatmap = state
        .local_analysis
        .find_beatmap_by_md5(request.client, &beatmap_hash)?
        .ok_or_else(|| {
            CommandError::new(
                "REPLAY_BEATMAP_NOT_INDEXED",
                "未在本地谱面索引中找到该回放对应的谱面，请先扫描本地谱面",
            )
        })?;
    if beatmap.ruleset != Ruleset::Osu {
        return Err(CommandError::new(
            "RENDER_RULESET_UNSUPPORTED",
            "o!rdr 当前仅支持 osu!standard 回放",
        ));
    }
    if beatmap.beatmap_id.is_none() {
        return Err(CommandError::new(
            "RENDER_BEATMAP_UNAVAILABLE",
            "所选本地谱面尚未提交到 osu!；o!rdr 无法下载它用于渲染",
        ));
    }
    let verification = verification(&request)?;
    let options = options_from(request.options)?;
    let skin = match request.skin_kind {
        RenderSkinKind::Official => {
            let name = request.skin.trim();
            if name.is_empty() {
                return Err(CommandError::new(
                    "INVALID_RENDER_SKIN",
                    "请输入 o!rdr 官方皮肤名称",
                ));
            }
            RenderSkinOption::from(name)
        }
        RenderSkinKind::Custom => {
            RenderSkinOption::from(request.skin.trim().parse::<u32>().map_err(|_| {
                CommandError::new("INVALID_RENDER_SKIN", "自定义皮肤 ID 必须是数字")
            })?)
        }
    };
    let mut builder = OrdrClient::builder();
    if let Some(value) = verification {
        builder = builder.verification(value);
    }
    let added = builder
        .build()
        .render_with_replay_file(&replay, username, &skin)
        .options(&options)
        .await
        .map_err(|error| CommandError::new("ORDR_SUBMIT_FAILED", error.to_string()))?;
    let job = ReplayRenderJob {
        render_id: added.render_id,
        status: "queued".into(),
        description: "已加入 o!rdr 队列，正在连接进度通道".into(),
    };
    emit(
        &app,
        ReplayRenderProgress {
            render_id: job.render_id,
            status: job.status.clone(),
            description: job.description.clone(),
            video_url: None,
        },
    );
    monitor_render(app, job.render_id);
    Ok(job)
}
