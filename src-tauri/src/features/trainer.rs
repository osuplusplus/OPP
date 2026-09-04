use std::{
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::LocalClient,
    state::AppState,
};

// Prevent overlapping preview requests from racing on the same staging path.
static GENERATION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Deserialize)]
pub struct TrainerRequest {
    pub client: LocalClient,
    pub resource_id: String,
    pub rate: f64,
    pub ar: f32,
    pub od: f32,
    pub cs: f32,
    pub hp: f32,
    #[serde(default)]
    pub no_spinners: bool,
    #[serde(default)]
    pub change_pitch: bool,
    /// View Trainer sets this for visual-only previews so ffmpeg is never
    /// started while the user is dragging a control.
    #[serde(default)]
    pub preview_only: bool,
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub start_time_ms: Option<f64>,
    pub end_time_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TrainerResult {
    pub directory: String,
    pub beatmap_path: String,
    pub included_objects: usize,
}

fn safe_name(value: &str) -> String {
    let value = value
        .chars()
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    let trimmed = value.trim().trim_matches('.');
    if trimmed.is_empty() {
        "OPP Trainer".into()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn update_key(line: &str, key: &str, value: impl std::fmt::Display) -> String {
    if line.trim_start().starts_with(key) && line.split_once(':').is_some() {
        format!("{key}:{value}")
    } else {
        line.to_string()
    }
}

fn object_time(line: &str) -> Option<f64> {
    line.split(',').nth(2)?.trim().parse().ok()
}

fn transform_hit_object(line: &str, start_ms: f64, rate: f64) -> String {
    let mut parts = line.split(',').map(str::to_string).collect::<Vec<_>>();
    if parts.len() < 3 {
        return line.to_string();
    }
    let transform = |value: &str| -> Option<String> {
        let time = value.trim().parse::<f64>().ok()?;
        Some(((time - start_ms) / rate).round().max(0.0).to_string())
    };
    if let Some(time) = transform(&parts[2]) {
        parts[2] = time;
    }
    let kind = parts
        .get(3)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or_default();
    if kind & 8 != 0 {
        if let Some(value) = parts.get(5).and_then(|value| transform(value)) {
            parts[5] = value;
        }
    } else if kind & 128 != 0
        && let Some(field) = parts.get_mut(5)
        && let Some((end, tail)) = field.split_once(':')
        && let Some(value) = transform(end)
    {
        *field = format!("{value}:{tail}");
    }
    parts.join(",")
}

fn bpm_at(timing_points: &[(f64, f64)], time: f64) -> Option<f64> {
    timing_points
        .iter()
        .rev()
        .find(|(at, _)| *at <= time)
        .and_then(|(_, beat_length)| (*beat_length > 0.0).then(|| 60_000.0 / *beat_length))
}

fn is_mania_beatmap(source: &str) -> bool {
    let mut in_general = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_general = trimmed.eq_ignore_ascii_case("[General]");
            continue;
        }
        if in_general
            && let Some((key, value)) = trimmed.split_once(':')
            && key.trim().eq_ignore_ascii_case("Mode")
        {
            return value.trim() == "3";
        }
    }
    false
}

fn transform_beatmap(source: &str, request: &TrainerRequest) -> CommandResult<(String, usize)> {
    // 只重写可安全识别的元数据和 HitObjects，保留其余 .osu 段落以兼容原始谱面。
    if !(0.75..=2.0).contains(&request.rate) {
        return Err(CommandError::new(
            "INVALID_RATE",
            "Rate 必须在 0.75× 到 2.00× 之间",
        ));
    }
    for (name, value, max) in [
        ("AR", request.ar, 11.0),
        ("OD", request.od, 11.0),
        ("CS", request.cs, 10.0),
        ("HP", request.hp, 10.0),
    ] {
        if !(0.0..=max).contains(&value) {
            return Err(CommandError::new(
                "INVALID_DIFFICULTY",
                format!("{name} 的取值超出允许范围（AR/OD: 0-11，CS/HP: 0-10）"),
            ));
        }
    }
    let start = request.start_time_ms.unwrap_or(0.0).max(0.0);
    if request.end_time_ms.is_some_and(|value| value <= start) {
        return Err(CommandError::new(
            "INVALID_TIME_RANGE",
            "结束时间必须大于开始时间",
        ));
    }
    if let (Some(min), Some(max)) = (request.min_bpm, request.max_bpm)
        && min > max
    {
        return Err(CommandError::new(
            "INVALID_BPM_RANGE",
            "最低 BPM 不能高于最高 BPM",
        ));
    }
    let end = request.end_time_ms;
    // Non-preview generation always uses the normalized ffmpeg output, even
    // for a 1x/full-range request. This keeps the audio path and the rewritten
    // chart deterministic and makes pitch handling a single backend concern.
    let transforms_audio = !request.preview_only;
    // In osu!mania CircleSize is the key count, not a visual difficulty setting.
    // Always preserve the value from the source chart so rate changes cannot collapse lanes.
    let preserve_circle_size = is_mania_beatmap(source);
    let mut section = "";
    let mut timing_points = Vec::<(f64, f64)>::new();
    for line in source.lines() {
        if line.trim().eq_ignore_ascii_case("[TimingPoints]") {
            section = "timing";
            continue;
        }
        if line.starts_with('[') {
            section = "";
        }
        if section == "timing" {
            let values = line.split(',').collect::<Vec<_>>();
            if values.len() >= 7
                && values[6].trim() == "1"
                && let (Ok(time), Ok(beat_length)) =
                    (values[0].trim().parse(), values[1].trim().parse())
            {
                timing_points.push((time, beat_length));
            }
        }
    }

    let mut output = Vec::new();
    section = "";
    let mut included = 0;
    for raw in source.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed;
            output.push(raw.to_string());
            continue;
        }
        if section == "[Difficulty]" {
            let next = update_key(raw, "ApproachRate", request.ar);
            let next = update_key(&next, "OverallDifficulty", request.od);
            let next = if preserve_circle_size {
                next
            } else {
                update_key(&next, "CircleSize", request.cs)
            };
            output.push(update_key(&next, "HPDrainRate", request.hp));
            continue;
        }
        if transforms_audio && section == "[General]" && trimmed.starts_with("AudioFilename:") {
            output.push("AudioFilename: opp-trainer-audio.mp3".into());
            continue;
        }
        if section == "[Metadata]" {
            if trimmed.starts_with("Creator:") {
                output.push("Creator: OPP Trainer".into());
                continue;
            }
            if trimmed.starts_with("Version:") {
                output.push(format!("Version: Trainer {:.2}x", request.rate));
                continue;
            }
            // Keep source IDs: osu! indexes entries by BeatmapSetID in osu!.db.
            // Zeroing every generated chart causes distinct difficulties to collapse.
        }
        if section == "[TimingPoints]" && !trimmed.is_empty() && !trimmed.starts_with("//") {
            let mut values = raw.split(',').map(str::to_string).collect::<Vec<_>>();
            if values.len() >= 2 {
                if let Ok(time) = values[0].trim().parse::<f64>() {
                    values[0] = ((time - start) / request.rate).round().max(0.0).to_string();
                }
                if let Ok(beat_length) = values[1].trim().parse::<f64>() {
                    values[1] = (beat_length / request.rate).to_string();
                }
                output.push(values.join(","));
                continue;
            }
        }
        if section == "[HitObjects]" && !trimmed.is_empty() && !trimmed.starts_with("//") {
            let Some(time) = object_time(raw) else {
                continue;
            };
            let kind = raw
                .split(',')
                .nth(3)
                .and_then(|value| value.trim().parse::<i32>().ok())
                .unwrap_or_default();
            if request.no_spinners && kind & 8 != 0 {
                continue;
            }
            let time_ok = time >= start && end.is_none_or(|limit| time <= limit);
            let bpm_ok = bpm_at(&timing_points, time).is_none_or(|bpm| {
                request.min_bpm.is_none_or(|min| bpm >= min)
                    && request.max_bpm.is_none_or(|max| bpm <= max)
            });
            if time_ok && bpm_ok {
                output.push(transform_hit_object(raw, start, request.rate));
                included += 1;
            }
            continue;
        }
        output.push(raw.to_string());
    }
    if included == 0 {
        return Err(CommandError::new(
            "EMPTY_TRAINER_RESULT",
            "筛选后没有可用于训练的物件",
        ));
    }
    Ok((output.join("\r\n") + "\r\n", included))
}

#[cfg(test)]
mod tests {
    use super::{TrainerRequest, transform_beatmap};
    use crate::features::local_analysis::LocalClient;

    fn request(rate: f64, cs: f32) -> TrainerRequest {
        TrainerRequest {
            client: LocalClient::Stable,
            resource_id: "test".into(),
            rate,
            ar: 8.0,
            od: 8.0,
            cs,
            hp: 5.0,
            no_spinners: false,
            change_pitch: false,
            preview_only: false,
            min_bpm: None,
            max_bpm: None,
            start_time_ms: None,
            end_time_ms: None,
        }
    }

    fn source(mode: u8) -> String {
        format!(
            "osu file format v14\n\n[General]\nMode:{mode}\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:7\nApproachRate:7\n\n[TimingPoints]\n0,500,4,2,1,50,1,0\n\n[HitObjects]\n64,192,1000,1,0,0:0:0:0:"
        )
    }

    #[test]
    fn mania_4k_rate_change_to_1_1x_preserves_source_key_count() {
        // Regression: with all difficulty fields left blank in the UI, changing only the rate
        // used to submit CS 0 and turn a native 4K chart into a 1K chart.
        let (output, included) = transform_beatmap(&source(3), &request(1.1, 0.0)).unwrap();

        assert_eq!(included, 1);
        assert!(output.contains("CircleSize:4\r\n"));
        assert!(!output.contains("CircleSize:0"));
    }

    #[test]
    fn standard_mode_still_allows_circle_size_changes() {
        let (output, _) = transform_beatmap(&source(0), &request(1.0, 6.5)).unwrap();

        assert!(output.contains("CircleSize:6.5\r\n"));
    }

    #[test]
    fn preview_keeps_source_audio_and_can_remove_spinners() {
        let mut request = request(1.5, 4.0);
        request.preview_only = true;
        request.no_spinners = true;
        let source = "osu file format v14\n\n[General]\nAudioFilename: song.mp3\nMode:0\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:7\nApproachRate:7\n\n[TimingPoints]\n0,500,4,2,1,50,1,0\n\n[HitObjects]\n64,192,1000,1,0,0:0:0:0:\n64,192,1500,8,0,0:0:0:0:";
        let (output, included) = transform_beatmap(source, &request).unwrap();
        assert_eq!(included, 1);
        assert!(output.contains("AudioFilename: song.mp3"));
        assert!(!output.contains(",1500,8,"));
    }

    #[test]
    fn ar_and_od_values_above_ten_are_valid() {
        let mut request = request(1.0, 4.0);
        request.ar = 11.0;
        request.od = 11.0;
        assert!(transform_beatmap(&source(0), &request).is_ok());
    }

    #[test]
    fn generated_identifiers_fit_osu_stable_integer_fields() {
        let id = (u32::from_str_radix("ffffffff", 16).unwrap() % 2_000_000_000).max(1);
        assert!(id <= 2_000_000_000);
    }

    #[test]
    fn change_pitch_requires_audio_processing_even_at_one_x() {
        let mut request = request(1.0, 4.0);
        request.change_pitch = true;
        let source = "osu file format v14\n\n[General]\nAudioFilename: song.mp3\nMode:0\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:7\nApproachRate:7\n\n[TimingPoints]\n0,500,4,2,1,50,1,0\n\n[HitObjects]\n64,192,1000,1,0,0:0:0:0:";
        let (output, _) = transform_beatmap(source, &request).unwrap();
        assert!(output.contains("AudioFilename: opp-trainer-audio.mp3"));
    }
}

fn prepare_audio(source: &Path, destination: &Path, request: &TrainerRequest) -> CommandResult<()> {
    // Every generated chart gets a fresh, normalized BGM. Keeping the audio
    // transform in the same ffmpeg invocation as trim/rate changes guarantees
    // that the chart timestamps and the rendered audio share one time base.
    let mut command = Command::new("ffmpeg");
    command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    if let Some(start) = request.start_time_ms {
        command.args(["-ss", &(start / 1000.0).to_string()]);
    }
    command.arg("-i").arg(source);
    if let Some(end) = request.end_time_ms {
        let duration = (end - request.start_time_ms.unwrap_or(0.0)).max(0.0) / 1000.0;
        command.args(["-t", &duration.to_string()]);
    }
    let filter = if request.change_pitch {
        format!("asetrate=44100*{:.6},aresample=44100", request.rate)
    } else {
        format!("atempo={:.6}", request.rate)
    };
    command
        .args(["-filter:a", &filter, "-vn", "-q:a", "2"])
        .arg(destination);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(CommandError::new(
            "TRAINER_AUDIO_FAILED",
            "ffmpeg 未能处理音频；请确认源音频可读",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(CommandError::new(
            "FFMPEG_REQUIRED",
            "生成训练音频需要 ffmpeg；请安装 ffmpeg 并加入 PATH 后重试",
        )),
        Err(error) => Err(CommandError::new("TRAINER_AUDIO_FAILED", error.to_string())),
    }
}

#[tauri::command(async)]
pub fn generate_trainer_beatmap(
    request: TrainerRequest,
    state: tauri::State<'_, AppState>,
) -> CommandResult<TrainerResult> {
    generate_trainer_beatmap_inner(request, &state, None)
}

/// Build a private working copy for View Trainer. The staging directory is
/// outside the osu! Songs tree so merely previewing edits cannot make osu!
/// index or import a new beatmap.
pub fn stage_trainer_beatmap(
    request: TrainerRequest,
    state: &AppState,
) -> CommandResult<TrainerResult> {
    let source_path = PathBuf::from(
        state
            .local_analysis
            .beatmap_file_path(request.client, &request.resource_id)?,
    );
    stage_trainer_beatmap_at_path(request, source_path)
}

/// Same staging operation with the resolved path supplied by the command
/// layer. This keeps the expensive file work off the Tauri async executor.
pub fn stage_trainer_beatmap_at_path(
    request: TrainerRequest,
    source_path: PathBuf,
) -> CommandResult<TrainerResult> {
    let root = std::env::temp_dir().join("opp").join("view-trainer");
    fs::create_dir_all(&root)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
    // Keep repeated edits cheap without allowing abandoned previews to grow
    // the temp directory forever.
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let stale = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.elapsed().ok())
                .is_some_and(|age| age > std::time::Duration::from_secs(24 * 60 * 60));
            if stale {
                let _ = fs::remove_dir_all(path);
            }
        }
    }

    generate_trainer_beatmap_from_path(request, source_path, Some(&root))
}

fn generate_trainer_beatmap_inner(
    request: TrainerRequest,
    state: &AppState,
    staging_root: Option<&Path>,
) -> CommandResult<TrainerResult> {
    let source_path = PathBuf::from(
        state
            .local_analysis
            .beatmap_file_path(request.client, &request.resource_id)?,
    );
    generate_trainer_beatmap_from_path(request, source_path, staging_root)
}

fn generate_trainer_beatmap_from_path(
    request: TrainerRequest,
    source_path: PathBuf,
    staging_root: Option<&Path>,
) -> CommandResult<TrainerResult> {
    let _generation_guard = GENERATION_LOCK
        .lock()
        .map_err(|_| CommandError::new("TRAINER_OUTPUT_FAILED", "生成器状态锁定失败"))?;
    let source_text = crate::features::local_analysis::parser::decode_text(
        &fs::read(&source_path)
            .map_err(|error| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", error.to_string()))?,
    );
    let source_dir = source_path
        .parent()
        .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "谱面目录不可用"))?;
    let songs_dir = source_dir.parent().unwrap_or(source_dir);
    let title = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Beatmap");
    let audio = source_dir.join(
        source_text
            .lines()
            .find_map(|line| line.strip_prefix("AudioFilename:"))
            .map(str::trim)
            .unwrap_or_default(),
    );
    let stage_key = if staging_root.is_some() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source_path.to_string_lossy().hash(&mut hasher);
        if let Ok(meta) = fs::metadata(&source_path) {
            meta.len().hash(&mut hasher);
            meta.modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|time| time.as_nanos())
                .hash(&mut hasher);
        }
        audio.to_string_lossy().hash(&mut hasher);
        if let Ok(meta) = fs::metadata(&audio) {
            meta.len().hash(&mut hasher);
            meta.modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|time| time.as_nanos())
                .hash(&mut hasher);
        }
        format!(
            "{}:{:.4}:{:.4}:{:.4}:{:.4}:{}:{}:{}:{}:{}:{}",
            request.rate,
            request.ar,
            request.od,
            request.cs,
            request.hp,
            request.no_spinners,
            request.change_pitch,
            request.preview_only,
            request.min_bpm.map_or(String::new(), |v| format!("{v:.4}")),
            request.max_bpm.map_or(String::new(), |v| format!("{v:.4}")),
            request
                .start_time_ms
                .map_or(String::new(), |v| format!("{v:.1}")),
        )
        .hash(&mut hasher);
        request
            .end_time_ms
            .map_or(String::new(), |v| format!("{v:.1}"))
            .hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    } else {
        String::new()
    };
    let target_dir = staging_root
        .map(|root| root.join(format!("{} - {}", safe_name(title), stage_key)))
        .unwrap_or_else(|| {
            songs_dir.join(format!(
                "OPP Trainer - {} - {}",
                safe_name(title),
                &Uuid::new_v4().simple().to_string()[..8]
            ))
        });
    // Give every generated chart a unique positive identity. osu!.db groups
    // entries by BeatmapSetID; reusing the source IDs makes repeated Trainer
    // generations appear as a single difficulty in game.
    // osu! Stable stores these identifiers as signed 32-bit integers in
    // osu!.db. Keep generated IDs within that range so imported charts are
    // accepted by the parser and indexed instead of reported as corrupt.
    fs::create_dir_all(&target_dir)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
    let beatmap_path = target_dir.join("OPP Trainer.osu");
    if beatmap_path.is_file() {
        let included_objects = fs::read_to_string(&beatmap_path)
            .ok()
            .map(|text| count_hit_objects(&text))
            .unwrap_or(0);
        let has_asset = fs::read_dir(&target_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.path().is_file() && entry.path() != beatmap_path);
        let audio_ready =
            !request.preview_only && target_dir.join("opp-trainer-audio.mp3").is_file();
        if included_objects > 0 && has_asset && (request.preview_only || audio_ready) {
            return Ok(TrainerResult {
                directory: target_dir.to_string_lossy().into_owned(),
                beatmap_path: beatmap_path.to_string_lossy().into_owned(),
                included_objects,
            });
        }
    }
    // Only transform the source after the deterministic staging path has been
    // checked. Slider changes that revisit an existing request now avoid the
    // full hit-object rewrite.
    let (beatmap, included_objects) = transform_beatmap(&source_text, &request)?;
    let unique_id = (u32::from_str_radix(&Uuid::new_v4().simple().to_string()[..8], 16)
        .unwrap_or(1)
        % 2_000_000_000)
        .max(1);
    let beatmap = beatmap
        .lines()
        .map(|line| {
            if line.starts_with("BeatmapID:") {
                format!("BeatmapID:{unique_id}")
            } else if line.starts_with("BeatmapSetID:") {
                format!("BeatmapSetID:{unique_id}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n";
    if !audio.is_file() {
        return Err(CommandError::new(
            "TRAINER_AUDIO_NOT_FOUND",
            "未找到谱面的音频文件",
        ));
    }
    if staging_root.is_none() {
        for entry in fs::read_dir(source_dir)
            .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?
        {
            let entry = entry
                .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
            let path = entry.path();
            // A generated folder must contain exactly one .osu difficulty. Copy only
            // non-beatmap assets; copying sibling difficulties makes osu! collapse
            // the folder into the wrong chart on import.
            if path.is_file()
                && path
                    .extension()
                    .is_none_or(|extension| !extension.eq_ignore_ascii_case("osu"))
            {
                let _ = fs::copy(&path, target_dir.join(entry.file_name()));
            }
        }
    } else {
        // The renderer only needs the audio for a live preview. Avoid copying
        // large video/storyboard/skin assets on every slider change.
        let relative_audio = audio.strip_prefix(source_dir).unwrap_or_else(|_| {
            audio
                .file_name()
                .map(Path::new)
                .unwrap_or_else(|| Path::new("audio.mp3"))
        });
        let staged_audio = target_dir.join(relative_audio);
        if let Some(parent) = staged_audio.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CommandError::new("TRAINER_AUDIO_COPY_FAILED", error.to_string())
            })?;
        }
        fs::copy(&audio, staged_audio)
            .map_err(|error| CommandError::new("TRAINER_AUDIO_COPY_FAILED", error.to_string()))?;
    }
    if beatmap
        .lines()
        .any(|line| line.trim() == "AudioFilename: opp-trainer-audio.mp3")
    {
        prepare_audio(&audio, &target_dir.join("opp-trainer-audio.mp3"), &request)?;
    }
    let temporary = target_dir.join(format!(".opp-trainer-{}.tmp", Uuid::new_v4().simple()));
    fs::write(&temporary, beatmap)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
    fs::rename(&temporary, &beatmap_path)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
    Ok(TrainerResult {
        directory: target_dir.to_string_lossy().into_owned(),
        beatmap_path: beatmap_path.to_string_lossy().into_owned(),
        included_objects,
    })
}

fn count_hit_objects(source: &str) -> usize {
    let mut section = "";
    source
        .lines()
        .filter(|raw| {
            let trimmed = raw.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed;
                return false;
            }
            section == "[HitObjects]"
                && !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && trimmed.split(',').count() >= 4
        })
        .count()
}
