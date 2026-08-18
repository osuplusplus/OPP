use std::{
    fs,
    path::{Path, PathBuf},
};

use osu_beatmap_preview::{PreviewOptions, generate_preview};
use serde::{Deserialize, Serialize};
use tauri::{async_runtime, ipc::Response};

use crate::{
    error::{CommandError, CommandResult},
    local_analysis::{
        LocalClient, StrainAnalysis,
        parser::{calculate_strains, parse_beatmap},
    },
    app::models::Ruleset,
};

const MIN_GIF_SECONDS: f64 = 1.0;
const MAX_GIF_SECONDS: f64 = 30.0;

#[derive(Debug, Clone, Serialize)]
pub struct BeatmapPreviewInspection {
    pub bid: u32,
    pub title: String,
    pub title_unicode: String,
    pub artist: String,
    pub artist_unicode: String,
    pub creator: String,
    pub difficulty_name: String,
    pub ruleset: Ruleset,
    pub length_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strains: Option<StrainAnalysis>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BeatmapPreviewRequest {
    pub bid: u32,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BeatmapPreviewResult {
    pub output_path: String,
    pub file_name: String,
    pub mime_type: String,
}

fn preview_root() -> PathBuf {
    std::env::temp_dir().join("osu-beatmap-preview")
}

fn beatmap_cache_path(bid: u32) -> PathBuf {
    preview_root()
        .join("osu-download-cache")
        .join(format!("{bid}.osu"))
}

fn output_root() -> PathBuf {
    preview_root().join("outputs")
}

fn validate_bid(bid: u32) -> CommandResult<()> {
    if bid == 0 {
        return Err(CommandError::new(
            "INVALID_BEATMAP_ID",
            "Beatmap ID 必须是正整数",
        ));
    }
    Ok(())
}

async fn ensure_beatmap_cached(bid: u32) -> CommandResult<Vec<u8>> {
    validate_bid(bid)?;
    let path = beatmap_cache_path(bid);
    if let Ok(bytes) = tokio::fs::read(&path).await
        && !bytes.is_empty()
    {
        return Ok(bytes);
    }

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("OPP/beatmap-preview")
        .build()
        .map_err(|error| CommandError::network(error.to_string()))?
        .get(format!("https://osu.ppy.sh/osu/{bid}"))
        .send()
        .await
        .map_err(|error| CommandError::network(format!("下载谱面失败：{error}")))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CommandError::new(
            "BEATMAP_NOT_FOUND",
            format!("未找到 Beatmap ID {bid}"),
        ));
    }
    if !response.status().is_success() {
        return Err(CommandError::network(format!(
            "下载谱面失败：HTTP {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| CommandError::network(format!("读取谱面失败：{error}")))?
        .to_vec();
    if bytes.is_empty() {
        return Err(CommandError::new("INVALID_BEATMAP", "下载到的谱面为空"));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, &bytes).await?;
    Ok(bytes)
}

fn inspect_bytes(bid: u32, bytes: &[u8]) -> CommandResult<BeatmapPreviewInspection> {
    let parsed = parse_beatmap(
        LocalClient::Stable,
        bytes,
        &format!("preview/{bid}.osu"),
        None,
        None,
    )
    .map_err(|message| CommandError::new("INVALID_BEATMAP", message))?;
    let summary = parsed.summary;
    let strains = if summary.ruleset == Ruleset::Osu {
        Some(
            calculate_strains(bytes)
                .map_err(|message| CommandError::new("BEATMAP_STRAIN_ERROR", message))?,
        )
    } else {
        None
    };
    Ok(BeatmapPreviewInspection {
        bid,
        title: summary.title,
        title_unicode: summary.title_unicode,
        artist: summary.artist,
        artist_unicode: summary.artist_unicode,
        creator: summary.creator,
        difficulty_name: summary.difficulty_name,
        ruleset: summary.ruleset,
        length_ms: summary.length_ms,
        strains,
    })
}

fn preview_options(
    request: &BeatmapPreviewRequest,
    ruleset: Ruleset,
    length_seconds: f64,
) -> CommandResult<PreviewOptions> {
    validate_bid(request.bid)?;
    let mut options = PreviewOptions::new(request.bid.to_string());
    if ruleset == Ruleset::Osu {
        let start = request.start_seconds.ok_or_else(|| {
            CommandError::new("PREVIEW_RANGE_REQUIRED", "std 预览需要选择开始时间")
        })?;
        let end = request.end_seconds.ok_or_else(|| {
            CommandError::new("PREVIEW_RANGE_REQUIRED", "std 预览需要选择结束时间")
        })?;
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end > length_seconds + 0.001 {
            return Err(CommandError::new(
                "INVALID_PREVIEW_RANGE",
                "预览区间超出谱面可玩范围",
            ));
        }
        let duration = end - start;
        if !(MIN_GIF_SECONDS..=MAX_GIF_SECONDS).contains(&duration) {
            return Err(CommandError::new(
                "INVALID_PREVIEW_RANGE",
                format!("GIF 区间必须在 {MIN_GIF_SECONDS:.0} 到 {MAX_GIF_SECONDS:.0} 秒之间"),
            ));
        }
        options.format = Some("gif".into());
        options.times = Some(format!("{start:.3}+{end:.3}"));
        options.gif_clip_label = true;
    } else {
        options.format = Some("png".into());
    }
    Ok(options)
}

fn validated_output(path: &str) -> CommandResult<PathBuf> {
    let candidate = PathBuf::from(path).canonicalize().map_err(|error| {
        CommandError::new(
            "PREVIEW_OUTPUT_NOT_FOUND",
            format!("预览文件不存在：{error}"),
        )
    })?;
    let root = output_root().canonicalize().map_err(|error| {
        CommandError::new(
            "PREVIEW_OUTPUT_NOT_FOUND",
            format!("预览目录不存在：{error}"),
        )
    })?;
    if !candidate.is_file() || !candidate.starts_with(&root) {
        return Err(CommandError::new(
            "INVALID_PREVIEW_OUTPUT",
            "只能访问本次预览工具生成的文件",
        ));
    }
    match candidate.extension().and_then(|value| value.to_str()) {
        Some(extension)
            if extension.eq_ignore_ascii_case("gif") || extension.eq_ignore_ascii_case("png") =>
        {
            Ok(candidate)
        }
        _ => Err(CommandError::new(
            "INVALID_PREVIEW_OUTPUT",
            "预览文件格式不受支持",
        )),
    }
}

fn result_from_value(value: serde_json::Value) -> CommandResult<BeatmapPreviewResult> {
    let output_path = value
        .get("preview-img")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CommandError::new("PREVIEW_RESULT_INVALID", "预览库没有返回输出文件"))?;
    let output = validated_output(output_path)?;
    let file_name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("beatmap-preview")
        .to_string();
    let mime_type = if output
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("gif"))
    {
        "image/gif"
    } else {
        "image/png"
    };
    Ok(BeatmapPreviewResult {
        output_path: output.to_string_lossy().into_owned(),
        file_name,
        mime_type: mime_type.into(),
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：检查资源的元数据或可用性。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn inspect_beatmap_preview(bid: u32) -> CommandResult<BeatmapPreviewInspection> {
    let bytes = ensure_beatmap_cached(bid).await?;
    async_runtime::spawn_blocking(move || inspect_bytes(bid, &bytes))
        .await
        .map_err(|error| CommandError::new("PREVIEW_INSPECTION_TASK_FAILED", error.to_string()))?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：生成派生资源或分析结果。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn generate_beatmap_preview(
    request: BeatmapPreviewRequest,
) -> CommandResult<BeatmapPreviewResult> {
    let bytes = ensure_beatmap_cached(request.bid).await?;
    let inspection = inspect_bytes(request.bid, &bytes)?;
    let options = preview_options(&request, inspection.ruleset, inspection.length_ms / 1_000.0)?;
    async_runtime::spawn_blocking(move || {
        generate_preview(options)
            .map_err(|error| CommandError::new("PREVIEW_GENERATION_FAILED", error.to_string()))
            .and_then(result_from_value)
    })
    .await
    .map_err(|error| CommandError::new("PREVIEW_GENERATION_TASK_FAILED", error.to_string()))?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取已生成或本地保存的内容。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn read_beatmap_preview_output(path: String) -> CommandResult<Response> {
    let output = validated_output(&path)?;
    Ok(Response::new(fs::read(output)?))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：校验并持久化用户配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn save_beatmap_preview_output(source: String, destination: String) -> CommandResult<String> {
    let source = validated_output(&source)?;
    let destination = PathBuf::from(destination);
    let source_extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let destination_extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !source_extension.eq_ignore_ascii_case(destination_extension) {
        return Err(CommandError::new(
            "PREVIEW_EXTENSION_MISMATCH",
            format!("请保存为 .{source_extension} 文件"),
        ));
    }
    if source != destination {
        fs::copy(&source, &destination).map_err(|error| {
            CommandError::new("PREVIEW_SAVE_FAILED", format!("保存预览失败：{error}"))
        })?;
    }
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn open_beatmap_preview_output(path: String) -> CommandResult<()> {
    let output = validated_output(&path)?;
    crate::platform::reveal_path(Path::new(&output)).map_err(|error| {
        CommandError::new(
            "PREVIEW_OPEN_FAILED",
            format!("无法打开预览所在文件夹：{error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OSU: &str = "osu file format v14\n\n[General]\nMode:0\n\n[Metadata]\nTitle:Test\nArtist:Artist\nCreator:Mapper\nVersion:Hard\nBeatmapID:123\nBeatmapSetID:12\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:8\nApproachRate:9\nSliderMultiplier:1.4\nSliderTickRate:1\n\n[TimingPoints]\n0,500,4,2,1,100,1,0\n\n[HitObjects]\n256,192,0,1,0,0:0:0:0:\n256,192,12000,1,0,0:0:0:0:\n";

    #[test]
    fn inspects_standard_mode_and_strains() {
        let inspected = inspect_bytes(123, OSU.as_bytes()).expect("inspection");
        assert_eq!(inspected.ruleset, Ruleset::Osu);
        assert_eq!(inspected.length_ms, 12_000.0);
        assert!(inspected.strains.is_some());
    }

    #[test]
    fn maps_standard_to_labeled_gif_clip() {
        let options = preview_options(
            &BeatmapPreviewRequest {
                bid: 123,
                start_seconds: Some(2.0),
                end_seconds: Some(12.0),
            },
            Ruleset::Osu,
            20.0,
        )
        .expect("options");
        assert_eq!(options.format.as_deref(), Some("gif"));
        assert_eq!(options.times.as_deref(), Some("2.000+12.000"));
        assert!(options.gif_clip_label);
    }

    #[test]
    fn maps_other_modes_to_full_png() {
        let options = preview_options(
            &BeatmapPreviewRequest {
                bid: 123,
                start_seconds: None,
                end_seconds: None,
            },
            Ruleset::Mania,
            200.0,
        )
        .expect("options");
        assert_eq!(options.format.as_deref(), Some("png"));
        assert!(options.times.is_none());
    }

    #[test]
    fn rejects_gif_ranges_over_thirty_seconds() {
        let error = preview_options(
            &BeatmapPreviewRequest {
                bid: 123,
                start_seconds: Some(0.0),
                end_seconds: Some(31.0),
            },
            Ruleset::Osu,
            60.0,
        )
        .expect_err("range must fail");
        assert_eq!(error.code, "INVALID_PREVIEW_RANGE");
    }
}
