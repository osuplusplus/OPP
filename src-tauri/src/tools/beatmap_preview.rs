//! 谱面预览工具：借助 `osu_beatmap_preview` 库把在线谱面渲染成 GIF 片段或
//! PNG 快照。`.osu` 文件按 Beatmap ID 缓存在系统临时目录，重复预览不再下载；
//! 输出图片只允许从本工具的输出目录读取（防路径穿越）。std 模式输出区间
//! GIF（1–30 秒），其余模式输出整图 PNG。

use std::{
    fs,
    path::{Path, PathBuf},
};

use osu_beatmap_preview::{PreviewOptions, generate_preview, parse_time_point};
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;

use crate::{
    domain::Ruleset,
    error::{CommandError, CommandResult},
    features::local_analysis::{
        LocalClient, StrainAnalysis,
        parser::{calculate_strains, parse_beatmap},
    },
    infrastructure::http::{HttpClientSpec, http_client},
};

/// GIF 片段时长下限（单位：秒），与 `MAX_GIF_SECONDS` 共同限定预览区间。
const MIN_GIF_SECONDS: f64 = 1.0;
/// GIF 片段时长上限（单位：秒）：GIF 体积随时长快速膨胀，超限直接拒绝。
const MAX_GIF_SECONDS: f64 = 30.0;

/// 谱面元数据检查结果：标题、作者、模式与总时长；std 模式附带难度走势，
/// 供前端选择预览区间时参考。
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
    /// 谱面总时长（单位：毫秒）。
    pub length_ms: f64,
    /// std 模式的难度走势；其余模式用不到且序列化时省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strains: Option<StrainAnalysis>,
}

/// 前端预览请求：`start_seconds`/`end_seconds` 单位为秒，仅 std 模式使用
/// （缺少时报错）；`mods` 缩写原样传给预览库影响渲染。
#[derive(Debug, Clone, Deserialize)]
pub struct BeatmapPreviewRequest {
    pub bid: u32,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
    #[serde(default)]
    pub mods: Vec<String>,
}

/// 预览产物描述：输出文件路径、文件名与 MIME 类型，供前端展示与保存。
#[derive(Debug, Clone, Serialize)]
pub struct BeatmapPreviewResult {
    pub output_path: String,
    pub file_name: String,
    pub mime_type: String,
}

/// 预览工具根目录：系统临时目录下的 `osu-beatmap-preview`。
fn preview_root() -> PathBuf {
    std::env::temp_dir().join("osu-beatmap-preview")
}

/// `.osu` 缓存路径：`osu-download-cache/<bid>.osu`。
fn beatmap_cache_path(bid: u32) -> PathBuf {
    preview_root()
        .join("osu-download-cache")
        .join(format!("{bid}.osu"))
}

/// 预览图片输出目录（`outputs`）；读取/保存仅放行该目录下的 gif/png 文件。
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

/// 取得谱面字节：优先读本地缓存，未命中时从 `https://osu.ppy.sh/osu/<bid>`
/// 下载（404 映射为 `BEATMAP_NOT_FOUND`）并写回缓存供下次复用。
async fn ensure_beatmap_cached(bid: u32) -> CommandResult<Vec<u8>> {
    validate_bid(bid)?;
    let path = beatmap_cache_path(bid);
    // 空文件视为缓存未命中，触发重新下载。
    if let Ok(bytes) = tokio::fs::read(&path).await
        && !bytes.is_empty()
    {
        return Ok(bytes);
    }

    let response = http_client(HttpClientSpec {
        timeout: Some(std::time::Duration::from_secs(20)),
        ua_note: " beatmap-preview",
        connect_timeout: None,
    })?
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

/// 解析谱面字节得到元数据检查结果；仅 std 模式额外计算难度走势
/// （`calculate_strains` 只支持 std）。
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

/// 组装预览库参数：std 模式必须给出 `[start, end]` 区间（秒），校验通过后
/// 输出区间 GIF；其余模式不设时间点，输出整图 PNG。
fn preview_options(
    request: &BeatmapPreviewRequest,
    ruleset: Ruleset,
    length_seconds: f64,
) -> CommandResult<PreviewOptions> {
    validate_bid(request.bid)?;
    let mut options = PreviewOptions::new(request.bid.to_string());
    options.mods = request.mods.clone();
    if ruleset == Ruleset::Osu {
        let start = request.start_seconds.ok_or_else(|| {
            CommandError::new("PREVIEW_RANGE_REQUIRED", "std 预览需要选择开始时间")
        })?;
        let end = request.end_seconds.ok_or_else(|| {
            CommandError::new("PREVIEW_RANGE_REQUIRED", "std 预览需要选择结束时间")
        })?;
        // 终点允许 1 毫秒容差抵消浮点误差，但不得超出谱面末尾。
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
        options.time_points = vec![
            parse_time_point(&format!("{start:.3}"))
                .map_err(|error| CommandError::new("INVALID_PREVIEW_RANGE", error.to_string()))?,
        ];
        options.duration_time = Some(duration);
    } else {
        options.format = Some("png".into());
    }
    Ok(options)
}

/// 校验并规范化输出路径：必须位于输出目录内、是普通文件、扩展名为 gif/png。
/// 双方先 canonicalize 再做前缀比对，阻止 `..` 或绝对路径穿越到任意文件。
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

/// 从预览库返回的 JSON（`preview-img` 字段为输出路径）提取并校验结果。
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

/// 检查在线谱面元数据：确保 `.osu` 已缓存后解析标题、模式、时长与难度
/// 走势；解析放到后台线程执行，避免阻塞命令线程。
#[tauri::command]
/// 供前端调用的 Tauri 命令：检查资源的元数据或可用性。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn inspect_beatmap_preview(bid: u32) -> CommandResult<BeatmapPreviewInspection> {
    let bytes = ensure_beatmap_cached(bid).await?;
    crate::infrastructure::tasks::background("tools", move || inspect_bytes(bid, &bytes))
        .await
        .map_err(|error| CommandError::new("PREVIEW_INSPECTION_TASK_FAILED", error.to_string()))?
}

/// 生成谱面预览图：元数据检查 → 参数校验 → 调用预览库渲染，全部在后台
/// 线程完成；输出路径经 `validated_output` 校验后才返回给前端。
#[tauri::command]
/// 供前端调用的 Tauri 命令：生成派生资源或分析结果。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn generate_beatmap_preview(
    request: BeatmapPreviewRequest,
) -> CommandResult<BeatmapPreviewResult> {
    let bytes = ensure_beatmap_cached(request.bid).await?;
    crate::infrastructure::tasks::background("tools", move || {
        let inspection = inspect_bytes(request.bid, &bytes)?;
        let options =
            preview_options(&request, inspection.ruleset, inspection.length_ms / 1_000.0)?;
        generate_preview(options)
            .map_err(|error| CommandError::new("PREVIEW_GENERATION_FAILED", error.to_string()))
            .and_then(result_from_value)
    })
    .await
    .map_err(|error| CommandError::new("PREVIEW_GENERATION_TASK_FAILED", error.to_string()))?
}

/// 以二进制 `tauri::ipc::Response` 回传预览图片内容，文件系统读取走阻塞
/// IO 线程池；路径限定在预览输出目录内。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取已生成或本地保存的内容。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn read_beatmap_preview_output(path: String) -> CommandResult<Response> {
    crate::infrastructure::tasks::blocking_io("read_beatmap_preview_output", move || {
        let output = validated_output(&path)?;
        Ok(Response::new(fs::read(output)?))
    })
    .await?
}

/// 把预览图复制到用户选择的位置：目标扩展名必须与源一致（防止把 GIF 存成
/// `.png` 误导用户），源路径仍限定在输出目录内；源等于目标时跳过复制。
#[tauri::command]
/// 供前端调用的 Tauri 命令：校验并持久化用户配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn save_beatmap_preview_output(
    source: String,
    destination: String,
) -> CommandResult<String> {
    crate::infrastructure::tasks::blocking_io("save_beatmap_preview_output", move || {
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
    })
    .await?
}

/// 在系统资源管理器中定位预览文件（复用平台层 `reveal_path`，Windows 上等
/// 价于选中文件后打开所在文件夹）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn open_beatmap_preview_output(path: String) -> CommandResult<()> {
    crate::infrastructure::tasks::blocking_io("open_beatmap_preview_output", move || {
        let output = validated_output(&path)?;
        crate::infrastructure::platform::reveal_path(Path::new(&output)).map_err(|error| {
            CommandError::new(
                "PREVIEW_OPEN_FAILED",
                format!("无法打开预览所在文件夹：{error}"),
            )
        })
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use osu_beatmap_preview::TimePoint;

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
                mods: Vec::new(),
            },
            Ruleset::Osu,
            20.0,
        )
        .expect("options");
        assert_eq!(options.format.as_deref(), Some("gif"));
        // GIF 剪辑语义 = 起点时间点 + 时长(旧字段 times/gif_clip_label 已被
        // 上游 PreviewOptions 的 time_points/duration_time 取代)。
        assert_eq!(options.time_points, vec![TimePoint::Seconds(2.0)]);
        assert_eq!(options.duration_time, Some(10.0));
    }

    #[test]
    fn maps_other_modes_to_full_png() {
        let options = preview_options(
            &BeatmapPreviewRequest {
                bid: 123,
                start_seconds: None,
                end_seconds: None,
                mods: Vec::new(),
            },
            Ruleset::Mania,
            200.0,
        )
        .expect("options");
        assert_eq!(options.format.as_deref(), Some("png"));
        assert!(options.time_points.is_empty());
    }

    #[test]
    fn rejects_gif_ranges_over_thirty_seconds() {
        let error = preview_options(
            &BeatmapPreviewRequest {
                bid: 123,
                start_seconds: Some(0.0),
                end_seconds: Some(31.0),
                mods: Vec::new(),
            },
            Ruleset::Osu,
            60.0,
        )
        .expect_err("range must fail");
        assert_eq!(error.code, "INVALID_PREVIEW_RANGE");
    }
}
