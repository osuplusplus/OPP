//! osu_replay_render: offscreen osu! replay renderer (wgpu + Argon skin).
//!
//! Usage:
//!   osu_replay_render <beatmap.osu> [replay.osr] [options]
//!
//! Options:
//!   --autoplay             Generate the replay from the beatmap (lazer
//!                          OsuAutoGenerator port) - no .osr needed
//!   --hud [on|off]         Gameplay HUD visibility, default on; `off` hides
//!                          score/accuracy/combo/health/UR bar/key overlay/
//!                          PP counter (`--hud` alone = on)
//!   --out <file.mp4>       Pipe frames to ffmpeg and encode (default if given)
//!   --png-dir <dir>        Write PNG frames to a directory instead
//!   --size <WxH>           Output size (default 1920x1080)
//!   --fps <n>              Output fps (default 60; frames are sampled from
//!                          the 60fps game-frame snapshots)
//!   --start <ms>           Start time in replay ms (default: beginning)
//!   --end <ms>             End time in replay ms (default: end)
//!   --score classic        Show classic (stable-style) score
//!   --skin <dir|argon|argon-pro>
//!                          Render with a user skin: an unpacked .osk
//!                          directory (skin.ini + sprites; @2x, animation
//!                          frames and number fonts resolved like lazer,
//!                          missing elements fall back to the built-in
//!                          argon visuals)
//!   --audio-offset <ms>    BGM alignment offset (default 0)
//!   --hitsounds            Synthesize the hitsound track (user skin
//!                          samples mixed with the built-in ArgonPro set,
//!                          lazer gameplay-audio parity) and mix it into
//!                          the export, amix-summed with --audio when
//!                          present
//!   --skin-colours         Force the user skin's combo colours over the
//!                          beatmap's [Colours] (stable behaviour, lazer
//!                          "Beatmap skins" off). Default: the beatmap's
//!                          colours win; the skin's only apply when the
//!                          beatmap ships none
//!   --results <secs>       Seconds of the (static, expanded) results
//!                          screen appended after gameplay (default 4;
//!                          --no-results disables)
//!   --results-only         Render ONLY the results screen (poster mode:
//!                          no gameplay frames; pair with --png-dir for a
//!                          single preview image, e.g. --results-only
//!                          --png-dir out --fps 1 --results 1)
//!   --limit <n>            Render at most n frames (testing)

use osu_replay_render::{build_atlas, decode_image_file, draw, draw::Image, game, hitsound, osu_background_file, osu_general_value, render::Renderer, scene, skin};

use scene::{Assets, SceneState};
use std::io::Write;

/// Hidden mod visual override.
enum HdMode {
    /// Follow the replay's own mods (default).
    Auto,
    /// `--hd`: force HD visuals on regardless of the replay's mods.
    On,
    /// `--no-hd`: force HD visuals off even when the replay has the mod.
    Off,
}

struct Options {
    out: Option<String>,
    png_dir: Option<String>,
    width: u32,
    height: u32,
    fps: f64,
    start: Option<f64>,
    end: Option<f64>,
    classic_score: bool,
    skin: String,
    /// User skin directory (`--skin <dir>`): legacy skin rendering.
    skin_dir: Option<std::path::PathBuf>,
    /// auto (default: probe NVENC, fall back to x264) | x264 | x265 | nvenc.
    encoder: String,
    /// crf; 18 by default.
    quality: u32,
    /// When set: render a single frame at this time and dump geometry JSON.
    probe: Option<f64>,
    limit: Option<usize>,
    ffmpeg_extra: Vec<String>,
    /// Whether the UR bar's window guide lines (colour axis) render.
    /// Default on; `--no-guides` disables them.
    guides: bool,
    /// Whether the gameplay HUD renders (score/accuracy/combo/health/UR
    /// bar/key overlay/PP counter). Default on; `--hud off` hides it all
    /// (independent of `--autoplay`, which no longer touches the HUD).
    hud: bool,
    /// Whether the live PP counter renders. Default on; `--no-pp` hides it.
    pp: bool,
    /// Optional BGM muxed into the output (`--audio [file]`; without a
    /// value the beatmap's own audio is used).
    audio: Option<String>,
    /// Draw the beatmap background image (`--bg`).
    bg: bool,
    /// Background opacity 0..1 (lazer: 1 - DimLevel, default DimLevel 0.7).
    bg_opacity: f32,
    /// Cursor size multiplier 0.1..=2 (lazer `GameplayCursorSize`,
    /// default 1). Scales the cursor and trail for every skin path.
    cursor_size: f32,
    /// Optional manual audio offset in ms (audio file position =
    /// replay_time - offset). Default 0; `--audio-offset` overrides.
    audio_offset: Option<f64>,
    /// Autoplay mod: generate the replay from the beatmap itself (lazer
    /// `OsuAutoGenerator` port) instead of reading an .osr — beatmap
    /// preview without a replay file.
    autoplay: bool,
    /// Force the Hidden mod visuals (`--hd`), overriding the replay's own
    /// mods: objects fade out before their hit time, approach circles are
    /// hidden (except the first object's). Visual only — judgement/score
    /// stay computed from the replay's actual mods (HD changes no
    /// judgement anyway).
    hd: HdMode,
    /// Synthesize the hitsound track (ArgonPro skin samples, lazer
    /// gameplay-audio parity: hit-only judgements, beatmap volumes/banks,
    /// slider loops, samples at their natural rate — no DT/HT pitch
    /// shift) and mix it into the export (`--hitsounds`).
    hitsounds: bool,
    /// Hitsound bus gain 0..1 (`--hitsounds-volume`, default 0.6 =
    /// `VolumeEffect`, see `OsuGame.GetFrameworkConfigDefaults`).
    hitsounds_volume: f32,
    /// BGM gain 0..1 (`--bgm-volume`, default 0.6 = `VolumeMusic`).
    bgm_volume: f32,
    /// Master gain applied to the final mixed audio 0..1
    /// (`--master-volume`, default 0.6 = `VolumeUniversal`). The game's
    /// effective bus gain is channel x master (0.6 x 0.6), so at defaults
    /// music + hitsounds peak at ~0.72 and essentially never clip.
    master_volume: f32,
    /// Force the user skin's combo colours over the beatmap's `[Colours]`
    /// (`--skin-colours`; stable behaviour, = lazer's "Beatmap skins"
    /// setting off). Default: the beatmap's colours win and the skin's
    /// only apply when the beatmap ships none.
    skin_colours: bool,
    /// Keep the Argon HUD even with a user skin (`--argon-hud`); by
    /// default the skin's own score/accuracy/combo/health/key pieces are
    /// used when it provides them.
    argon_hud: bool,
    /// Seconds of the (static, expanded) results screen appended after
    /// gameplay (`--results <secs>`; default 4, `--no-results` disables).
    results: f64,
    /// Render ONLY the results screen, no gameplay frames
    /// (`--results-only`; duration still set by `--results`).
    results_only: bool,
    /// Custom results-screen avatar image (`--avatar <image>` / config
    /// `avatar`): cover-cropped square with rounded corners.
    avatar: Option<String>,
}

/// `--config <file.json>`: every key mirrors the matching CLI flag
/// (snake_case). Explicit CLI flags win over the config regardless of
/// the order they appear in.
#[derive(serde::Deserialize)]
struct ConfigJson {
    out: Option<String>,
    png_dir: Option<String>,
    size: Option<String>,
    fps: Option<f64>,
    start: Option<f64>,
    end: Option<f64>,
    score: Option<String>,
    skin: Option<String>,
    encoder: Option<String>,
    quality: Option<u32>,
    limit: Option<usize>,
    no_guides: Option<bool>,
    hud: Option<bool>,
    no_pp: Option<bool>,
    audio: Option<String>,
    bg: Option<bool>,
    bg_opacity: Option<f32>,
    cursor_size: Option<f32>,
    audio_offset: Option<f64>,
    autoplay: Option<bool>,
    hd: Option<String>,
    hitsounds: Option<bool>,
    hitsounds_volume: Option<f32>,
    bgm_volume: Option<f32>,
    master_volume: Option<f32>,
    skin_colours: Option<bool>,
    argon_hud: Option<bool>,
    results: Option<f64>,
    results_only: Option<bool>,
    avatar: Option<String>,
    ffmpeg_extra: Option<Vec<String>>,
}

/// `--hud <value>`: `on`/`off`, with `true`/`false`/`1`/`0` aliases
/// (config mirrors it as the JSON boolean `"hud"`).
fn parse_hud_value(v: &str) -> Result<bool, String> {
    match v {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        other => Err(format!("--hud must be on or off (got {})", other)),
    }
}

/// `--skin` value shared by the CLI flag and the config's `skin` key.
fn set_skin(opts: &mut Options, skin: &str) -> Result<(), String> {
    if skin == "argon" || skin == "argon-pro" {
        opts.skin = skin.to_string();
        opts.skin_dir = None;
    } else {
        // A user skin directory (unpacked .osk): legacy skin rendering
        // with per-element argon fallbacks.
        let p = std::path::Path::new(skin);
        if !p.is_dir() {
            return Err(format!("skin: not a directory: {} (or argon|argon-pro)", skin));
        }
        opts.skin_dir = Some(p.to_path_buf());
        opts.skin = "argon".to_string();
    }
    Ok(())
}

/// Applies the config over the current options (config wins over the
/// defaults; CLI wins over the config since it is applied afterwards).
fn apply_config(opts: &mut Options, c: ConfigJson) -> Result<(), String> {
    if c.out.is_some() { opts.out = c.out; }
    if c.png_dir.is_some() { opts.png_dir = c.png_dir; }
    if let Some(s) = c.size {
        let mut it = s.split('x');
        opts.width = it.next().and_then(|v| v.parse().ok()).ok_or("config size: bad WxH")?;
        opts.height = it.next().and_then(|v| v.parse().ok()).ok_or("config size: bad WxH")?;
    }
    if let Some(v) = c.fps { opts.fps = v; }
    if c.start.is_some() { opts.start = c.start; }
    if c.end.is_some() { opts.end = c.end; }
    if let Some(v) = c.score {
        if v == "classic" {
            opts.classic_score = true;
        }
    }
    if let Some(v) = c.skin {
        set_skin(opts, &v)?;
    }
    if let Some(v) = c.encoder {
        if !matches!(v.as_str(), "auto" | "x264" | "x265" | "nvenc") {
            return Err("config encoder must be auto, x264, x265 or nvenc".into());
        }
        opts.encoder = v;
    }
    if let Some(v) = c.quality { opts.quality = v; }
    if let Some(v) = c.limit { opts.limit = Some(v); }
    if let Some(true) = c.no_guides { opts.guides = false; }
    if let Some(v) = c.hud { opts.hud = v; }
    if let Some(true) = c.no_pp { opts.pp = false; }
    if c.audio.is_some() { opts.audio = c.audio; }
    if let Some(true) = c.bg { opts.bg = true; }
    if let Some(v) = c.bg_opacity { opts.bg_opacity = v; }
    if let Some(v) = c.cursor_size { opts.cursor_size = v; }
    if c.audio_offset.is_some() { opts.audio_offset = c.audio_offset; }
    if let Some(true) = c.autoplay { opts.autoplay = true; }
    if let Some(v) = c.hd {
        opts.hd = match v.as_str() {
            "on" => HdMode::On,
            "off" => HdMode::Off,
            "auto" => HdMode::Auto,
            other => return Err(format!("config hd must be auto, on or off (got {})", other)),
        };
    }
    if let Some(true) = c.hitsounds { opts.hitsounds = true; }
    if let Some(v) = c.hitsounds_volume { opts.hitsounds_volume = v; }
    if let Some(v) = c.bgm_volume { opts.bgm_volume = v; }
    if let Some(v) = c.master_volume { opts.master_volume = v; }
    if let Some(true) = c.skin_colours { opts.skin_colours = true; }
    if let Some(true) = c.argon_hud { opts.argon_hud = true; }
    if let Some(v) = c.results { opts.results = v; }
    if let Some(true) = c.results_only {
        opts.results_only = true;
        if opts.results <= 0.0 {
            opts.results = 4.0;
        }
    }
    if c.avatar.is_some() { opts.avatar = c.avatar; }
    if let Some(v) = c.ffmpeg_extra { opts.ffmpeg_extra = v; }
    Ok(())
}

fn parse_args() -> Result<(Options, String, Option<String>), String> {
    let args: Vec<String> = std::env::args().collect();
    // `--autoplay` (Autoplay mod, beatmap preview) makes the replay file
    // optional; positionals stay "map first, replay second, then flags".
    let autoplay = args.iter().any(|a| a == "--autoplay");
    let min_args = if autoplay { 2 } else { 3 };
    if args.len() < min_args {
        return Err(format!("usage: {} <beatmap.osu> [replay.osr] [--autoplay] [--hud on|off] [--hd] [--no-hd] [--out file.mp4] [--png-dir dir] [--size WxH] [--fps n] [--start ms] [--end ms] [--score classic] [--skin argon|argon-pro|dir] [--argon-hud] [--no-guides] [--no-pp] [--audio [file.mp3]] [--audio-offset ms] [--bg] [--bg-opacity 0..1] [--cursor-size 0.1..=2] [--hitsounds] [--skin-colours] [--results secs] [--results-only] [--avatar image] [--config file.json] [--limit n]", args.get(0).map(|s| s.as_str()).unwrap_or("osu_replay_render")));
    }
    let map_path = args[1].clone();
    let replay_path = if autoplay { None } else { Some(args[2].clone()) };
    let mut opts = Options {
        out: None,
        png_dir: None,
        width: 1920,
        height: 1080,
        fps: 60.0,
        start: None,
        end: None,
        classic_score: false,
        skin: "argon-pro".to_string(),
        skin_dir: None,
        encoder: "auto".to_string(),
        quality: 18,
        probe: None,
        limit: None,
        ffmpeg_extra: Vec::new(),
        guides: true,
        hud: true,
        pp: true,
        audio: None,
        bg: false,
        bg_opacity: 0.3,
        cursor_size: 1.0,
        audio_offset: None,
        autoplay,
        hd: HdMode::Auto,
        hitsounds: false,
        hitsounds_volume: 0.6,
        bgm_volume: 0.6,
        master_volume: 0.6,
        skin_colours: false,
        argon_hud: false,
        results: 4.0,
        results_only: false,
        avatar: None,
    };
    // `--config <file.json>` pre-pass: the JSON provides base values;
    // explicit CLI flags win over it regardless of order. Relative
    // `avatar` paths resolve against the config file's directory.
    let mut i = min_args;
    while i < args.len() {
        if args[i] == "--config" {
            i += 1;
            let path = args.get(i).ok_or("--config needs a JSON file")?;
            let text = std::fs::read_to_string(path).map_err(|e| format!("--config: cannot read {}: {}", path, e))?;
            let cfg: ConfigJson =
                serde_json::from_str(&text).map_err(|e| format!("--config: bad JSON in {}: {}", path, e))?;
            let mut cfg = cfg;
            if let Some(a) = &cfg.avatar {
                if std::path::Path::new(a).is_relative() {
                    let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new("."));
                    let joined = dir.join(a);
                    if joined.exists() {
                        cfg.avatar = Some(joined.to_string_lossy().into_owned());
                    }
                }
            }
            apply_config(&mut opts, cfg)?;
        }
        i += 1;
    }
    let mut i = min_args;
    let mut i = min_args;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                opts.out = args.get(i).cloned();
            }
            "--png-dir" => {
                i += 1;
                opts.png_dir = args.get(i).cloned();
            }
            "--fps" => {
                i += 1;
                opts.fps = args.get(i).and_then(|v| v.parse().ok()).ok_or("bad --fps")?;
                if opts.fps < 1.0 || opts.fps > 480.0 {
                    return Err("--fps must be within 1..480".into());
                }
            }
            "--size" => {
                i += 1;
                let s = args.get(i).ok_or("--size needs WxH")?;
                let mut it = s.split('x');
                opts.width = it.next().and_then(|v| v.parse().ok()).ok_or("bad --size")?;
                opts.height = it.next().and_then(|v| v.parse().ok()).ok_or("bad --size")?;
            }
            "--start" => {
                i += 1;
                opts.start = args.get(i).and_then(|v| v.parse().ok());
            }
            "--end" => {
                i += 1;
                opts.end = args.get(i).and_then(|v| v.parse().ok());
            }
            "--score" => {
                i += 1;
                if args.get(i).map(|s| s.as_str()) == Some("classic") {
                    opts.classic_score = true;
                }
            }
            "--skin" => {
                i += 1;
                let skin = args.get(i).cloned().ok_or("--skin needs a value")?;
                set_skin(&mut opts, &skin)?;
            }
            "--encoder" => {
                i += 1;
                let enc = args.get(i).cloned().ok_or("--encoder needs a value")?;
                if !matches!(enc.as_str(), "auto" | "x264" | "x265" | "nvenc") {
                    return Err("--encoder must be auto, x264, x265 or nvenc".into());
                }
                opts.encoder = enc;
            }
            "--quality" => {
                i += 1;
                opts.quality = args.get(i).and_then(|v| v.parse().ok()).ok_or("bad --quality")?;
            }
            "--probe" => {
                i += 1;
                opts.probe = args.get(i).and_then(|v| v.parse().ok());
            }
            "--limit" => {
                i += 1;
                opts.limit = args.get(i).and_then(|v| v.parse().ok());
            }
            "--no-guides" => {
                opts.guides = false;
            }
            "--hud" => {
                // Optional value: bare `--hud` means on (the default);
                // `--hud on|off` (true/false aliases) overrides.
                opts.hud = match args.get(i + 1) {
                    Some(v) if !v.starts_with("--") => {
                        i += 1;
                        parse_hud_value(v)?
                    }
                    _ => true,
                };
            }
            "--no-pp" => {
                opts.pp = false;
            }
            "--audio" => {
                // Optional value: an explicit file, else the beatmap's own
                // audio (resolved later once the map path is known).
                opts.audio = match args.get(i + 1) {
                    Some(v) if !v.starts_with("--") => {
                        i += 1;
                        Some(v.clone())
                    }
                    Some(_) | None => Some(String::new()),
                };
            }
            "--bg" => {
                opts.bg = true;
            }
            "--bg-opacity" => {
                i += 1;
                opts.bg_opacity = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad --bg-opacity (expected 0..1)")?;
                if !(0.0..=1.0).contains(&opts.bg_opacity) {
                    return Err("--bg-opacity must be within 0..1".into());
                }
            }
            "--cursor-size" => {
                i += 1;
                opts.cursor_size = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad --cursor-size (expected 0.1..=2)")?;
                if !(0.1..=2.0).contains(&opts.cursor_size) {
                    return Err("--cursor-size must be within 0.1..=2 (lazer GameplayCursorSize)".into());
                }
            }
            "--audio-offset" => {
                i += 1;
                opts.audio_offset = Some(
                    args.get(i)
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad --audio-offset (expected milliseconds)")?,
                );
            }
            "--autoplay" => {
                opts.autoplay = true;
            }
            "--hd" => {
                opts.hd = HdMode::On;
            }
            "--no-hd" => {
                opts.hd = HdMode::Off;
            }
            "--hitsounds" => {
                opts.hitsounds = true;
            }
            "--skin-colours" => {
                opts.skin_colours = true;
            }
            "--argon-hud" => {
                opts.argon_hud = true;
            }
            "--results" => {
                i += 1;
                opts.results = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad --results (expected seconds)")?;
                if opts.results < 0.0 || opts.results > 300.0 {
                    return Err("--results must be within 0..300 seconds".into());
                }
            }
            "--no-results" => {
                opts.results = 0.0;
            }
            "--results-only" => {
                opts.results_only = true;
                if opts.results <= 0.0 {
                    opts.results = 4.0;
                }
            }
            "--avatar" => {
                i += 1;
                opts.avatar = Some(args.get(i).cloned().ok_or("--avatar needs an image path")?);
            }
            "--config" => {
                // Consumed by the pre-pass; skip its value here.
                i += 1;
            }
            "--master-volume" => {
                i += 1;
                opts.master_volume = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad --master-volume (expected 0..1)")?;
                if !(0.0..=1.0).contains(&opts.master_volume) {
                    return Err("--master-volume must be within 0..1".into());
                }
            }
            "--hitsounds-volume" => {
                i += 1;
                opts.hitsounds_volume = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad --hitsounds-volume (expected 0..1)")?;
                if !(0.0..=1.0).contains(&opts.hitsounds_volume) {
                    return Err("--hitsounds-volume must be within 0..1".into());
                }
            }
            "--bgm-volume" => {
                i += 1;
                opts.bgm_volume = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad --bgm-volume (expected 0..1)")?;
                if !(0.0..=1.0).contains(&opts.bgm_volume) {
                    return Err("--bgm-volume must be within 0..1".into());
                }
            }
            other => return Err(format!("unknown argument: {}", other)),
        }
        i += 1;
    }
    Ok((opts, map_path, replay_path))
}

enum Output {
    /// Frames are sent to a dedicated writer thread through a bounded
    /// channel (backpressure); the thread owns the ffmpeg child, writes
    /// each frame with ONE write (contiguous at typical widths, else a
    /// single repack), and joins at `finish`. Spent frame buffers are
    /// recycled back through `ret` so the render thread never reallocates
    /// the ~8MB readback payload per frame.
    Ffmpeg {
        tx: Option<std::sync::mpsc::SyncSender<Vec<u8>>>,
        ret: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
        handle: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    },
    PngDir(String),
    None,
}

impl Output {
    /// Returns a buffer to fill the next frame into: a recycled one from
    /// the writer thread when available, a fresh allocation otherwise.
    fn take_buf(&mut self, frame_bytes: usize) -> Vec<u8> {
        match self {
            Output::Ffmpeg { ret, .. } => {
                if let Some(rx) = ret {
                    if let Ok(mut buf) = rx.try_recv() {
                        buf.clear();
                        buf.reserve(frame_bytes);
                        return buf;
                    }
                }
                Vec::with_capacity(frame_bytes)
            }
            _ => Vec::with_capacity(frame_bytes),
        }
    }

    /// Takes ownership of `buf` (frame data, padded rows) and queues it to
    /// the writer thread. Cheap unless the writer is more than
    /// `WRITER_QUEUE` frames behind (natural backpressure).
    fn write_frame(&mut self, mut buf: Vec<u8>, width: u32, height: u32, stride: u32, index: usize) -> std::io::Result<()> {
        match self {
            Output::Ffmpeg { tx, .. } => {
                let tx = tx.as_ref().expect("writer channel");
                if stride != width * 4 {
                    // Repack padded rows into one contiguous buffer so the
                    // writer thread does a single write per frame.
                    let mut tight = Vec::with_capacity((width * height * 4) as usize);
                    for row in 0..height as usize {
                        let start = row * stride as usize;
                        tight.extend_from_slice(&buf[start..start + width as usize * 4]);
                    }
                    buf = tight;
                }
                let _ = index;
                tx.send(buf).map_err(|_| std::io::Error::other("ffmpeg writer exited"))
            }
            Output::PngDir(dir) => {
                let data: &[u8] = &buf;
                let path = format!("{}/frame_{:06}.png", dir, index);
                let file = std::fs::File::create(&path)?;
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                let mut writer = enc.write_header()?;
                // Convert BGRA -> RGBA rows.
                let mut rgba = vec![0u8; (width * height * 4) as usize];
                for row in 0..height {
                    let src = (row * stride) as usize;
                    for x in 0..width {
                        let s = src + (x * 4) as usize;
                        let d = ((row * width + x) * 4) as usize;
                        rgba[d] = data[s + 2];
                        rgba[d + 1] = data[s + 1];
                        rgba[d + 2] = data[s];
                        rgba[d + 3] = data[s + 3];
                    }
                }
                writer.write_image_data(&rgba)?;
                Ok(())
            }
            Output::None => Ok(()),
        }
    }

    /// Drops the sender (EOF for the writer thread), then joins it: the
    /// thread drains the queue, closes ffmpeg's stdin and waits for the
    /// encode to finish.
    fn finish(mut self) {
        if let Output::Ffmpeg { tx, ret, handle } = &mut self {
            drop(tx.take());
            drop(ret.take());
            match handle.take().unwrap().join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("ffmpeg writer error: {}", e),
                Err(_) => eprintln!("ffmpeg writer panicked"),
            }
        }
    }
}

/// Bound of the frame queue to the ffmpeg writer thread (frames).
const WRITER_QUEUE: usize = 3;

/// First audio stream's sample rate (Hz), for the Nightcore `asetrate`
/// pitch-up. Falls back to 44100 when ffprobe is missing or fails.
fn probe_sample_rate(path: &str) -> u32 {
    std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=sample_rate", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(44_100)
}

fn main() {
    let (opts, map_path, replay_path) = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(2);
        }
    };

    let game = match &replay_path {
        Some(rp) => {
            eprintln!("loading {} + {}", map_path, rp);
            game::load(&map_path, rp)
        }
        None => {
            eprintln!("loading {} (autoplay preview)", map_path);
            game::load_autoplay(&map_path)
        }
    };
    let mut game = match game {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    // HD override: by default (Auto) the visuals follow the replay's own
    // mods; `--hd`/`--no-hd` force them on/off (visual override only —
    // score/judgement always come from the replay's actual mods).
    match opts.hd {
        HdMode::Auto => {}
        HdMode::On => game.hidden = true,
        HdMode::Off => game.hidden = false,
    }
    eprintln!(
        "player: {} | objects: {} | snapshots: {} | final score: {} (max combo {}) | pp: {} / {} max",
        game.player,
        game.objects.len(),
        game.snapshots.len(),
        game.final_score,
        game.final_max_combo,
        if game.pp.is_nan() { "-".to_string() } else { format!("{:.2}", game.pp) },
        if game.pp_max.is_nan() { "-".to_string() } else { format!("{:.2}", game.pp_max) },
    );

    // Resolve optional BGM: explicit file, or the beatmap's own audio
    // (`AudioFilename`, relative to the map).
    let map_dir = std::path::Path::new(&map_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let audio_path: Option<String> = match &opts.audio {
        Some(explicit) if !explicit.is_empty() => {
            if !std::path::Path::new(explicit).exists() {
                eprintln!("error: audio file not found: {}", explicit);
                std::process::exit(1);
            }
            Some(explicit.clone())
        }
        Some(_) => {
            let name = osu_general_value(&map_path, "AudioFilename")
                .unwrap_or_else(|| "audio.mp3".to_string());
            let p = map_dir.join(&name);
            if p.exists() {
                eprintln!("audio: {} (from beatmap)", p.display());
                Some(p.to_string_lossy().into_owned())
            } else {
                eprintln!("warning: beatmap audio not found: {} - rendering without BGM", p.display());
                None
            }
        }
        None => None,
    };

    // Resolve the beatmap background: the `[Events]` background image,
    // decoded into the atlas (raw + pre-blurred copy). Gameplay draws it
    // full-screen at `--bg-opacity` when `--bg` is on (default
    // 1 - DimLevel 0.7, matching lazer); the results screen always draws
    // the blurred copy (lazer `ResultsScreen`).
    let bg_image: Option<Image> = match osu_background_file(&map_path) {
        Some(name) => {
            let p = map_dir.join(&name);
            match decode_image_file(&p) {
                Ok(img) => {
                    eprintln!("background: {} ({}x{})", p.display(), img.width, img.height);
                    Some(img)
                }
                Err(e) => {
                    eprintln!("warning: {} - rendering without background", e);
                    None
                }
            }
        }
        None => {
            if opts.bg {
                eprintln!("warning: beatmap has no background image - rendering without background");
            }
            None
        }
    };

    let has_bg = bg_image.is_some();

    // Skin resolution (`--skin <dir>`: user legacy skin with argon
    // fallbacks). Default combo colours: the beatmap's `[Colours]` win
    // (lazer "Beatmap skins" on); `--skin-colours` forces the skin's.
    let mut resolved_skin = match skin::load_skin(opts.skin_dir.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    game::apply_skin_combo_colours(&mut game, &resolved_skin, opts.skin_colours);

    // Custom results-screen avatar (`--avatar <image>` / config
    // `avatar`): decoded into the atlas, pre-cropped and pre-rounded.
    let avatar_image: Option<Image> = match &opts.avatar {
        Some(p) => match decode_image_file(std::path::Path::new(p)) {
            Ok(img) => {
                eprintln!("avatar: {} ({}x{})", p, img.width, img.height);
                Some(img)
            }
            Err(e) => {
                eprintln!("warning: {} - falling back to the initial placeholder", e);
                None
            }
        },
        None => None,
    };

    // 8192 is the GLES/GL-compat floor for max_texture_dimension2d:
    // capping here keeps the atlas creatable on every backend (desktop
    // Vulkan/dGPU simply packs wider instead of taller).
    let (atlas, fonts) = build_atlas(bg_image, avatar_image, &mut resolved_skin, 8192);
    eprintln!("atlas: {}x{}", atlas.width, atlas.height);

    let mut renderer = Renderer::new(opts.width, opts.height, &atlas);
    let mut state = SceneState::new(&game, opts.width, opts.height);
    state.pro_skin = opts.skin == "argon-pro";
    state.hud.ur_guides = opts.guides;
    state.hud.visible = opts.hud;
    state.hud.pp_display = opts.pp;
    state.hud.argon_hud = opts.argon_hud;
    state.bg_opacity = if has_bg && opts.bg { Some(opts.bg_opacity) } else { None };
    state.has_bg = has_bg;
    state.has_avatar = opts.avatar.is_some();
    state.cursor_size = opts.cursor_size;
    if opts.classic_score {
        state.hud.use_classic_score();
    }

    // Frame selection: sample the replay at the requested fps.
    let start = opts.start.unwrap_or(f64::NEG_INFINITY);
    let end = opts.end.unwrap_or(f64::INFINITY);
    let first_snap = game.snapshots.first().map(|s| s.time).unwrap_or(0.0);
    let last_snap = game.snapshots.last().map(|s| s.time).unwrap_or(0.0);

    let mut frame_times: Vec<f64> = Vec::new();
    if (opts.fps - 60.0).abs() < 1e-6 {
        // Exact game-frame cadence: use the engine snapshots 1:1.
        for s in &game.snapshots {
            if s.time >= start && s.time <= end {
                frame_times.push(s.time);
            }
        }
    } else {
        // Rate mods (DT/NC 1.5, HT 0.75) compress the replay's map-time
        // timeline onto the video's wall timeline: one output frame
        // (1/fps wall s) covers `rate` × 1/fps map ms — the 60fps branch
        // gets this for free from the engine's game-frame snapshot
        // cadence, and the audio side (hitsound placements / BGM atempo)
        // assumes it. Without the factor a DT export plays at the map's
        // original speed and desyncs from its audio.
        let step = 1000.0 / opts.fps * game.rate;
        let mut t = first_snap.max(start.min(last_snap));
        while t <= last_snap.min(end) {
            frame_times.push(t);
            t += step;
        }
    }
    let frame_times = match opts.limit {
        Some(l) => frame_times.into_iter().take(l).collect::<Vec<_>>(),
        None => frame_times,
    };

    if frame_times.is_empty() {
        eprintln!("error: no frames to render");
        std::process::exit(1);
    }

    // Results screen (`--results`, default 4s): appended after gameplay as
    // the static expanded end-state (lazer ResultsScreen, no entrance
    // animations). The frames are duplicates of the last gameplay time —
    // `state.results_at` switches the scene builder over. `--results-only`
    // drops the gameplay frames entirely and renders JUST the results
    // screen (poster/preview generation).
    let results_frames = (opts.results * opts.fps).round() as usize;
    let mut frame_times = frame_times;
    if opts.results_only {
        let last = *frame_times.last().unwrap();
        let n = results_frames.max(1);
        frame_times.clear();
        for _ in 0..n {
            frame_times.push(last);
        }
        state.results_at = Some(last);
        eprintln!("results-only: {} frames ({:.1}s)", n, opts.results);
    } else if results_frames > 0 {
        let last = *frame_times.last().unwrap();
        for _ in 0..results_frames {
            frame_times.push(last);
        }
        state.results_at = Some(last);
        // Sequential handover: the gameplay fades out (0.35s), then the
        // results screen appears instantly after the fade-out.
        state.results_fade_frames = (0.35 * opts.fps).round() as u32;
        state.results_fadein_frames = 0;
        eprintln!("results: +{} frames ({:.1}s, expanded panel, gameplay fade-out 0.35s)", results_frames, opts.results);
    }

    if opts.hitsounds && opts.out.is_none() {
        eprintln!("warning: --hitsounds needs --out (video export) - ignoring");
    }

    // Hitsound track (`--hitsounds`): synthesized offline from the
    // judgement timeline plus the .osu sample data (hit-only triggers,
    // beatmap banks/volumes, slider slide loops, samples at their
    // natural rate — rate mods only compress the trigger times), on the
    // export's wall timeline so it muxes 1:1 with the video. Written as a
    // temp WAV, mixed into the output in the second pass below.
    let hits_path: Option<String> = if opts.hitsounds && opts.out.is_some() {
        match std::fs::read_to_string(&map_path) {
            Ok(content) => {
                let t0 = frame_times[0];
                let wall_secs = frame_times.len() as f64 / opts.fps;
                let wav = hitsound::render_track_wav(&game, &content, t0, wall_secs, game.rate, opts.hitsounds_volume, &resolved_skin);
                let p = format!("{}.hits.wav", opts.out.as_ref().unwrap());
                std::fs::write(&p, wav).unwrap_or_else(|e| panic!("write {}: {}", p, e));
                eprintln!("hitsounds: {} ({} samples, {:.1}s)", p, if resolved_skin.is_legacy() { "user skin mixed with ArgonPro" } else { "ArgonPro" }, wall_secs);
                Some(p)
            }
            Err(e) => {
                eprintln!("warning: cannot re-read beatmap for hitsounds: {} - skipping", e);
                None
            }
        }
    } else {
        None
    };

    // auto: probe NVENC with a tiny test encode, fall back to x264.
    let encoder = if opts.encoder == "auto" {
        let probe = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "nullsrc=s=256x256:d=0.04",
                   "-c:v", "h264_nvenc", "-f", "null", "-"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if probe.map(|st| st.success()).unwrap_or(false) { "nvenc" } else { "x264" }
    } else {
        opts.encoder.as_str()
    };
    eprintln!("encoder: {}", encoder);

    // Output setup.
    let mut output = if let Some(dir) = &opts.png_dir {
        std::fs::create_dir_all(dir).expect("create png dir");
        Output::PngDir(dir.clone())
    } else if let Some(out) = &opts.out {
        // Input side: frames are piped BGRA. NVENC accepts bgr0 natively and
        // converts in hardware (fastest end-to-end: ~1.7x x264); the x264 /
        // x265 software paths go through CPU swscale to yuv420p.
        let (in_pix_fmt, encode_args): (&str, Vec<String>) = if encoder == "nvenc" {
            (
                "bgr0",
                vec![
                    "-c:v", "h264_nvenc", "-preset", "p5", "-tune", "hq",
                    "-rc", "vbr", "-cq", &opts.quality.to_string(), "-b:v", "0",
                    "-movflags", "+faststart",
                ]
                .iter().map(|s| s.to_string()).collect(),
            )
        } else {
            let codec = if encoder == "x265" { "libx265" } else { "libx264" };
            (
                "bgra",
                vec![
                    "-c:v", codec, "-preset", "medium",
                    "-crf", &opts.quality.to_string(),
                    "-pix_fmt", "yuv420p", "-movflags", "+faststart",
                ]
                .iter().map(|s| s.to_string()).collect(),
            )
        };
        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-f").arg("rawvideo")
            .arg("-pix_fmt").arg(in_pix_fmt)
            .arg("-s").arg(format!("{}x{}", opts.width, opts.height))
            .arg("-r").arg(format!("{}", opts.fps))
            .arg("-i").arg("-");
        cmd.args(&encode_args);
        // With BGM or hitsounds the audio is muxed in a SECOND ffmpeg pass
        // after rendering (see below): muxing audio directly on the raw
        // pipe can grow ffmpeg's interleave queue without bound when fed
        // fast.
        let video_tmp = if audio_path.is_some() || hits_path.is_some() {
            format!("{}.video.tmp.mp4", out)
        } else {
            out.clone()
        };
        let mut child = cmd
            .arg(&video_tmp)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create("ffmpeg_err.log").unwrap())
            .spawn()
            .expect("failed to spawn ffmpeg (is it on PATH? or use --png-dir)");
        // Dedicated writer thread: owns ffmpeg's stdin, one write per
        // frame, decoupled from the render loop by a bounded channel.
        // Buffers are recycled back to the render thread after use.
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(WRITER_QUEUE);
        let (ret_tx, ret_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let handle = std::thread::spawn(move || {
            let mut stdin = child.stdin.take().expect("ffmpeg stdin");
            for frame in rx {
                stdin.write_all(&frame)?;
                let _ = ret_tx.send(frame);
            }
            drop(stdin); // EOF -> ffmpeg finishes the encode
            let status = child.wait()?;
            if !status.success() {
                return Err(std::io::Error::other(format!("ffmpeg exited with {:?}", status.code())));
            }
            Ok(())
        });
        Output::Ffmpeg { tx: Some(tx), ret: Some(ret_rx), handle: Some(handle) }
    } else {
        eprintln!("no output specified; use --out <file.mp4> or --png-dir <dir>");
        std::process::exit(2);
    };

    let total = frame_times.len();
    let t0 = std::time::Instant::now();
    let mut list = draw::DrawList::new();
    let assets = Assets { atlas: &atlas, bold: &fonts.bold, semibold: &fonts.semibold, light: &fonts.light, venera: &fonts.venera, regular: &fonts.regular, skin: &resolved_skin };
    let stats = std::env::var("RENDER_STATS").is_ok();
    let (mut s_build, mut s_render, mut s_write) = (0.0f64, 0.0f64, 0.0f64);
    // Index of the next frame to be written out; lags behind the frame
    // being submitted while the pipeline is running ahead.
    let mut written = 0usize;

    for (n, &ft) in frame_times.iter().enumerate() {
        let ta = std::time::Instant::now();
        list.clear();
        let snap = game::snapshot_at(&game, ft);
        state.build_frame(&game, &assets, &snap, &mut list);
        list.finish();
        if let Some(pt) = opts.probe {
            if (ft - pt).abs() < 30.0 {
                state.probe_dump(&game, ft, "probe.json");
            }
        }
        let tb = std::time::Instant::now();
        // Pipelined: submit this frame WITHOUT waiting for the GPU, and
        // only read back the OLDEST in-flight frame once the pipeline has
        // reached depth 2. Until then the GPU renders ahead while the CPU
        // keeps building the next frame; reading immediately after submit
        // would serialize CPU and GPU again.
        renderer.render_deferred(&list, [0.055, 0.055, 0.075, 1.0]);
        let tc = std::time::Instant::now();
        if renderer.pending_len() >= 2 {
            let mut buf = output.take_buf((renderer.padded_row as usize) * opts.height as usize);
            renderer.read_oldest_into(&mut buf);
            if let Err(e) = output.write_frame(buf, opts.width, opts.height, renderer.padded_row, written) {
                eprintln!("error writing frame {}: {}", written, e);
                std::process::exit(1);
            }
            written += 1;
        }
        let td = std::time::Instant::now();
        s_build += tb.duration_since(ta).as_secs_f64();
        s_render += tc.duration_since(tb).as_secs_f64();
        s_write += td.duration_since(tc).as_secs_f64();
        if n % 300 == 0 || n + 1 == total {
            eprintln!(
                "frame {}/{} (t={:.0}ms) elapsed {:.1}s",
                n + 1,
                total,
                ft,
                t0.elapsed().as_secs_f32()
            );
        }
    }
    if stats {
        eprintln!(
            "stats: build {:.2}s ({:.2}ms/f) | render+readback {:.2}s ({:.2}ms/f) | write {:.2}s ({:.2}ms/f) | total {:.2}s",
            s_build, s_build * 1000.0 / total as f64,
            s_render, s_render * 1000.0 / total as f64,
            s_write, s_write * 1000.0 / total as f64,
            t0.elapsed().as_secs_f64()
        );
    }

    // Drain the pipeline's last frame(s) into the writer before finishing.
    while renderer.pending_len() > 0 {
        let mut buf = output.take_buf((renderer.padded_row as usize) * opts.height as usize);
        renderer.read_oldest_into(&mut buf);
        if let Err(e) = output.write_frame(buf, opts.width, opts.height, renderer.padded_row, written) {
            eprintln!("error writing final frame: {}", e);
            std::process::exit(1);
        }
        written += 1;
    }
        output.finish();

    // Second pass: mux the audio into the video (stream copy + AAC).
    // Audio file position = replay_time - offset*rate (offset is a
    // wall-time ms value, multiplied by the gameplay rate like lazer's
    // OffsetCorrectionClock). When that position is negative the replay
    // clock has not reached the track start yet — lazer holds the track
    // stopped and starts it once the file position crosses 0, reproduced
    // here by delaying the audio with leading silence (adelay) instead of
    // clamping the seek to 0 (the clamp started the music |t0| early and
    // silently disabled the offset for renders starting at t=0). Rate
    // mods tempo-stretch the track with atempo — the pitch stays intact,
    // a deliberate deviation from lazer's Track rate adjustment (which
    // resamples, pitch and tempo together) — except Nightcore, which
    // keeps the game's resampled pitch-up. The hitsound track is a
    // separate audio stream: with BGM present both are summed with amix
    // (normalize=0 keeps the levels as authored).
    if audio_path.is_some() || hits_path.is_some() {
        if let Some(out) = &opts.out {
            let offset = opts.audio_offset.unwrap_or(0.0);
            let rate = game.rate;
            let first = frame_times.first().map(|t| *t).unwrap_or(0.0);
            let seek_ms = first - offset * rate;
            let tmp = format!("{}.video.tmp.mp4", out);
            if let Some(audio) = &audio_path {
                eprintln!("muxing audio: {} (offset {}ms, rate {}, file start {}ms)", audio, offset, rate, seek_ms.max(0.0));
            }
            let mut cmd = std::process::Command::new("ffmpeg");
            cmd.arg("-y")
                .arg("-v").arg("error")
                .arg("-i").arg(&tmp);
            let mut bgm_filters: Vec<String> = Vec::new();
            if let Some(audio) = &audio_path {
                if (opts.bgm_volume - 1.0).abs() > 1e-6 {
                    bgm_filters.push(format!("volume={:.4}", opts.bgm_volume));
                }
                if seek_ms >= 0.0 {
                    cmd.arg("-ss").arg(format!("{:.3}", seek_ms / 1000.0));
                } else {
                    // Negative file position: pad silence so the music
                    // begins exactly when the replay clock reaches the
                    // offset. The delay is inserted on the file (map)
                    // timeline and must be pushed BEFORE atempo, which
                    // compresses it to -seek/rate wall ms; atempo ahead
                    // of adelay stalls the graph's inter-stream sync
                    // under -shortest (ffmpeg 9) and mangles the mux.
                    bgm_filters.push(format!("adelay={}:all=1", (-seek_ms).round() as i64));
                }
                if (rate - 1.0).abs() > 1e-9 {
                    if game.nightcore {
                        // Nightcore keeps the game's pitch-up: resampling,
                        // tempo and pitch scale together (rate 1.5) —
                        // nightcore without the pitch isn't nightcore.
                        let sr = probe_sample_rate(audio);
                        bgm_filters.push(format!("asetrate={},aresample={}", (sr as f64 * rate).round() as i64, sr));
                    } else {
                        // DT/HT: tempo-only stretch — atempo keeps the
                        // music's pitch, a deliberate deviation from the
                        // game, whose track rate adjustment resamples
                        // (pitch and tempo together). DT 1.5 / HT 0.75
                        // are within atempo's 0.5..100 range.
                        bgm_filters.push(format!("atempo={:.6}", rate));
                    }
                }
                cmd.arg("-i").arg(audio);
            }
            // Sum stage: plain float summation like the game's BASS mixer
            // (no limiter, no normalization), followed by the master
            // volume (`VolumeUniversal`) on the final mix — the game's
            // chain is per-channel volume x master, i.e. 0.6 x 0.6 at
            // osu! defaults. Overs clip exactly where the game's DAC
            // would.
            let master = if (opts.master_volume - 1.0).abs() > 1e-6 {
                format!(",volume={:.4}", opts.master_volume)
            } else {
                String::new()
            };
            // Results tail: the video runs past the music by the appended
            // results frames. Pad the mix with silence and pin the output
            // length to the video track (plain -shortest would cut the
            // results off as soon as the music ends).
            let results_tail = if results_frames > 0 {
                format!("apad,atrim=duration={:.3}", frame_times.len() as f64 / opts.fps)
            } else {
                String::new()
            };
            let shortest = if results_frames > 0 { "" } else { "-shortest" };
            match (&audio_path, &hits_path) {
                (Some(_), Some(hits)) => {
                    cmd.arg("-i").arg(hits);
                    let head = if bgm_filters.is_empty() {
                        "[1:a]".to_string()
                    } else {
                        format!("[1:a]{}[bgm];[bgm]", bgm_filters.join(","))
                    };
                    let pad = if results_tail.is_empty() { String::new() } else { format!(",{}", results_tail) };
                    cmd.arg("-filter_complex").arg(format!("{head}amix=inputs=2:normalize=0{master}{pad}[aout]"));
                    cmd.arg("-map").arg("0:v").arg("-map").arg("[aout]");
                }
                (Some(_), None) => {
                    let mut filters = bgm_filters.clone();
                    if !master.is_empty() {
                        filters.push(master.trim_start_matches(',').to_string());
                    }
                    if !results_tail.is_empty() {
                        filters.push(results_tail.clone());
                    }
                    if !filters.is_empty() {
                        cmd.arg("-af").arg(filters.join(","));
                    }
                    cmd.arg("-map").arg("0:v").arg("-map").arg("1:a");
                }
                (None, Some(hits)) => {
                    cmd.arg("-i").arg(hits);
                    let mut filters = Vec::new();
                    if !master.is_empty() {
                        filters.push(master.trim_start_matches(',').to_string());
                    }
                    if !results_tail.is_empty() {
                        filters.push(results_tail.clone());
                    }
                    if !filters.is_empty() {
                        cmd.arg("-af").arg(filters.join(","));
                    }
                    cmd.arg("-map").arg("0:v").arg("-map").arg("1:a");
                }
                (None, None) => unreachable!(),
            }
            cmd.arg("-c:v").arg("copy")
                // Pin the video track timescale to the fps so each frame is
                // exactly one tick and the container reports avg_frame_rate
                // 60/1; a source with microsecond timestamps (e.g. the
                // raw-h264 demuxer rounds 1/60s to 16667us) would otherwise
                // yield 1000000/16667 (~59.9988).
                .arg("-video_track_timescale").arg(opts.fps.round().max(1.0).to_string())
                .arg("-c:a").arg("aac").arg("-b:a").arg("192k");
            if !shortest.is_empty() {
                cmd.arg(shortest);
            }
            let status = cmd
                .arg("-movflags").arg("+faststart")
                .arg(out)
                .status()
                .expect("ffmpeg audio mux");
            let keep_tmp = std::env::var("HITSOUND_DEBUG").is_ok();
            if !keep_tmp {
                let _ = std::fs::remove_file(&tmp);
                if let Some(hits) = &hits_path {
                    let _ = std::fs::remove_file(hits);
                }
            }
            if !status.success() {
                eprintln!("error: audio mux failed");
                std::process::exit(1);
            }
        }
    }
    eprintln!("done: {} frames in {:.1}s", total, t0.elapsed().as_secs_f32());
}
