use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    local_analysis::LocalClient,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct TrainerRequest {
    pub client: LocalClient,
    pub resource_id: String,
    pub rate: f64,
    pub ar: f32,
    pub od: f32,
    pub cs: f32,
    pub hp: f32,
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
    for (name, value) in [
        ("AR", request.ar),
        ("OD", request.od),
        ("CS", request.cs),
        ("HP", request.hp),
    ] {
        if !(0.0..=10.0).contains(&value) {
            return Err(CommandError::new(
                "INVALID_DIFFICULTY",
                format!("{name} 必须在 0 到 10 之间"),
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
    let transforms_audio =
        (request.rate - 1.0).abs() > f64::EPSILON || start > 0.0 || end.is_some();
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
            if trimmed.starts_with("BeatmapID:") || trimmed.starts_with("BeatmapSetID:") {
                output.push(
                    raw.split_once(':')
                        .map(|(key, _)| format!("{key}:0"))
                        .unwrap_or_else(|| raw.to_string()),
                );
                continue;
            }
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
    use crate::local_analysis::LocalClient;

    fn request(rate: f64, cs: f32) -> TrainerRequest {
        TrainerRequest {
            client: LocalClient::Stable,
            resource_id: "test".into(),
            rate,
            ar: 8.0,
            od: 8.0,
            cs,
            hp: 5.0,
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
}

fn prepare_audio(source: &Path, destination: &Path, request: &TrainerRequest) -> CommandResult<()> {
    // 音频变速需要外部工具；未请求变速时直接复制以避免无损资源被重复编码。
    let requires_transform = (request.rate - 1.0).abs() > f64::EPSILON
        || request.start_time_ms.unwrap_or(0.0) > 0.0
        || request.end_time_ms.is_some();
    if !requires_transform {
        return fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| CommandError::new("TRAINER_AUDIO_COPY_FAILED", error.to_string()));
    }
    let mut command = Command::new("ffmpeg");
    command.arg("-y");
    if let Some(start) = request.start_time_ms {
        command.args(["-ss", &(start / 1000.0).to_string()]);
    }
    command.arg("-i").arg(source);
    if let Some(end) = request.end_time_ms {
        let duration = (end - request.start_time_ms.unwrap_or(0.0)).max(0.0) / 1000.0;
        command.args(["-t", &duration.to_string()]);
    }
    command
        .args([
            "-filter:a",
            &format!("atempo={}", request.rate),
            "-vn",
            "-q:a",
            "2",
        ])
        .arg(destination);
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(CommandError::new(
            "TRAINER_AUDIO_FAILED",
            "ffmpeg 未能处理音频；请确认源音频可读",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(CommandError::new(
            "FFMPEG_REQUIRED",
            "变速或截取训练需要 ffmpeg；请安装 ffmpeg 并加入 PATH 后重试",
        )),
        Err(error) => Err(CommandError::new("TRAINER_AUDIO_FAILED", error.to_string())),
    }
}

#[tauri::command(async)]
pub fn generate_trainer_beatmap(
    request: TrainerRequest,
    state: tauri::State<'_, AppState>,
) -> CommandResult<TrainerResult> {
    let source_path = PathBuf::from(
        state
            .local_analysis
            .beatmap_file_path(request.client, &request.resource_id)?,
    );
    let source_text = crate::local_analysis::parser::decode_text(
        &fs::read(&source_path)
            .map_err(|error| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", error.to_string()))?,
    );
    let (beatmap, included_objects) = transform_beatmap(&source_text, &request)?;
    let source_dir = source_path
        .parent()
        .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "谱面目录不可用"))?;
    let songs_dir = source_dir.parent().unwrap_or(source_dir);
    let title = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Beatmap");
    let target_dir = songs_dir.join(format!(
        "OPP Trainer - {} - {}",
        safe_name(title),
        &Uuid::new_v4().simple().to_string()[..8]
    ));
    fs::create_dir_all(&target_dir)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
    for entry in fs::read_dir(source_dir)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?
    {
        let entry =
            entry.map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_none_or(|extension| !extension.eq_ignore_ascii_case("osu"))
        {
            let target = target_dir.join(entry.file_name());
            let _ = fs::copy(path, target);
        }
    }
    let audio = source_dir.join(
        source_text
            .lines()
            .find_map(|line| line.strip_prefix("AudioFilename:"))
            .map(str::trim)
            .unwrap_or_default(),
    );
    if !audio.is_file() {
        return Err(CommandError::new(
            "TRAINER_AUDIO_NOT_FOUND",
            "未找到谱面的音频文件",
        ));
    }
    prepare_audio(&audio, &target_dir.join("opp-trainer-audio.mp3"), &request)?;
    let beatmap_path = target_dir.join("OPP Trainer.osu");
    fs::write(&beatmap_path, beatmap)
        .map_err(|error| CommandError::new("TRAINER_OUTPUT_FAILED", error.to_string()))?;
    Ok(TrainerResult {
        directory: target_dir.to_string_lossy().into_owned(),
        beatmap_path: beatmap_path.to_string_lossy().into_owned(),
        included_objects,
    })
}
