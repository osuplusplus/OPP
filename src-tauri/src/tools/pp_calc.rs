//! 在线谱面 PP 计算：按「官方 osu! API → Catboy → Nerinyan」顺序下载
//! `.osu`（全部失败才报错并附各自原因），再用 rosu-pp 4.0.1（ppy-osu
//! rulesets 口径）在同一份难度属性上计算星数、指定成绩假设下的 PP 与
//! 满分 PP。

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

#[derive(Debug, Clone, Deserialize)]
/// 前端提交的在线谱面 PP 计算参数；未填写的表现字段由 rosu-pp 使用默认值。
pub struct BeatmapCalculationRequest {
    pub beatmap_id: u64,
    /// Mod 缩写列表（如 `HD`、`DT`，大小写不敏感，`NM`/空表表示无 Mod）。
    #[serde(default)]
    pub mods: Vec<String>,
    /// 目标准确率（单位：百分比，越界值会被夹取到 0–100）。
    pub accuracy: Option<f64>,
    pub misses: Option<u32>,
    pub combo: Option<u32>,
    pub n300: Option<u32>,
    pub n100: Option<u32>,
    pub n50: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
/// PP 计算结果及其来源、算法版本，便于界面解释与问题追踪。
pub struct BeatmapCalculationResult {
    pub beatmap_id: u64,
    pub mods: Vec<String>,
    pub mode: String,
    /// 星数（难度值，已包含 Mod 的影响）。
    pub stars: f64,
    /// 按请求中的成绩假设（准确率/miss/combo/判定数）计算出的 PP。
    pub pp: f64,
    /// 同一 Mod 组合下 SS 满分的 PP，供前端展示达成度对比。
    pub max_pp: f64,
    /// 该谱面（含 Mod）的最大连击数。
    pub max_combo: u32,
    pub calculation_engine: String,
    pub calculated_at: String,
    /// `.osu` 文件的下载来源标识，由成功的 provider 填写。
    pub source: String,
    pub star_algorithm: String,
    pub star_algorithm_date: String,
    pub performance_algorithm: String,
    pub performance_algorithm_date: String,
}

#[tauri::command]
/// 下载谱面并计算指定 Mod 与成绩假设下的星数、PP 和最大 PP。
///
/// 优先读取官方 .osu，失败后按 Catboy、Nerinyan 回退。
pub async fn calculate_beatmap_pp(
    request: BeatmapCalculationRequest,
    state: State<'_, AppState>,
) -> CommandResult<BeatmapCalculationResult> {
    let download = match state.providers.official_osu(request.beatmap_id).await {
        Ok(download) => download,
        Err(official) => match state.providers.catboy_osu(request.beatmap_id).await {
            Ok(download) => download,
            Err(catboy) => state
                .providers
                .nerinyan_osu(request.beatmap_id)
                .await
                .map_err(|nerinyan| {
                    CommandError::new(
                        "BEATMAP_CALCULATION_SOURCE_FAILED",
                        format!(
                            "osu!: {}; Catboy: {}; Nerinyan: {}",
                            official.message, catboy.message, nerinyan.message
                        ),
                    )
                })?,
        },
    };
    crate::infrastructure::tasks::interactive("calculate_beatmap_pp", move || {
        let bytes = download.bytes;
        let started = Instant::now();
        let map = rosu_pp::Beatmap::from_bytes(&bytes)
            .map_err(|error| CommandError::new("BEATMAP_PARSE_FAILED", error.to_string()))?;
        // 模式字符串（"osu"/"taiko"/"catch"/"mania"）用于 Mod 合法性校验与算法标签。
        let mode = format!("{:?}", map.mode).to_lowercase();
        let bits = mod_bits(&request.mods, &mode)?;
        // 难度属性（星数）只计算一次：满 PP 与成绩 PP 都复用同一份属性。
        let difficulty = rosu_pp::Difficulty::new().mods(bits).calculate(&map);
        let max_pp = difficulty.clone().performance().calculate().pp();
        let mut performance = difficulty.performance().mods(bits);
        if let Some(value) = request.accuracy {
            performance = performance.accuracy(value.clamp(0.0, 100.0));
        }
        if let Some(value) = request.misses {
            performance = performance.misses(value);
        }
        if let Some(value) = request.combo {
            performance = performance.combo(value);
        }
        if let Some(value) = request.n300 {
            performance = performance.n300(value);
        }
        if let Some(value) = request.n100 {
            performance = performance.n100(value);
        }
        if let Some(value) = request.n50 {
            performance = performance.n50(value);
        }
        let attributes = performance.calculate();
        let _elapsed = started.elapsed();
        Ok(BeatmapCalculationResult {
            beatmap_id: request.beatmap_id,
            mods: request.mods,
            mode: mode.clone(),
            stars: attributes.stars(),
            pp: attributes.pp(),
            max_pp,
            max_combo: attributes.max_combo(),
            calculation_engine: "rosu-pp 4.0.1 / ppy-osu rulesets".into(),
            calculated_at: chrono::Utc::now().to_rfc3339(),
            source: download.source,
            star_algorithm: format!("rosu-pp 4.0.1 · {mode} star"),
            star_algorithm_date: "2025-10-16".into(),
            performance_algorithm: format!("rosu-pp 4.0.1 · {mode} performance"),
            performance_algorithm_date: "2025-10-16".into(),
        })
    })
    .await?
}

/// 将前端传入的 Mod 缩写合并为 rosu-pp 位掩码，并校验模式专属 Mod。
fn mod_bits(mods: &[String], mode: &str) -> CommandResult<u32> {
    let mut bits = 0;
    for value in mods {
        let acronym = value.trim().to_ascii_uppercase();
        let bit = match acronym.as_str() {
            "NM" | "NOMOD" => 0,
            "NF" => 1,
            "EZ" => 2,
            "TD" => 4,
            "HD" => 8,
            "HR" => 16,
            "SD" => 32,
            "DT" => 64,
            "RX" => 128,
            "HT" => 256,
            "NC" => 512,
            "FL" => 1024,
            "SO" => 4096,
            // FI/RD/MR 是 mania 专属 Mod，映射到高位比特；其他模式下直接拒绝。
            "FI" if mode == "mania" => 1 << 20,
            "RD" if mode == "mania" => 1 << 21,
            "MR" if mode == "mania" => 1 << 30,
            "FI" | "RD" | "MR" => {
                return Err(CommandError::new(
                    "MOD_NOT_AVAILABLE",
                    format!("{acronym} is not available for {mode}"),
                ));
            }
            _ => {
                return Err(CommandError::new(
                    "INVALID_MOD",
                    format!("不支持的 Mod：{value}"),
                ));
            }
        };
        bits |= bit;
    }
    Ok(bits)
}
