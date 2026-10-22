//! 视野训练的前端契约：生成请求与谱面时间线摘要。
use crate::features::local_analysis::{LocalClient, StrainSeries};
use serde::{Deserialize, Serialize};

/// 训练谱面生成请求，前端以 camelCase 提交。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewTrainerRequest {
    /// 谱面来源（Stable/lazer）与对应的本地资源 ID。
    pub client: LocalClient,
    pub resource_id: String,
    /// 倍速，有效范围 0.75 到 2.0。
    pub rate: f64,
    /// 为 `true` 时按“目标 BPM / 源主 BPM”覆盖 `rate`。
    #[serde(default)]
    pub bpm_locked: bool,
    #[serde(default)]
    pub target_bpm: Option<f64>,
    /// 目标难度值；未锁定时会按源谱面与倍速重新计算。
    pub ar: f32,
    pub od: f32,
    pub cs: f32,
    pub hp: f32,
    /// 随倍速缩放 AR，taiko 与 mania 模式不缩放。
    #[serde(default)]
    pub scale_ar: bool,
    /// 随倍速缩放 OD。
    #[serde(default)]
    pub scale_od: bool,
    // lock_* 系列：为 true 时保留请求值；否则 AR/OD 可被缩放覆盖，
    // CS/HP 直接回填源谱面值。
    #[serde(default)]
    pub lock_ar: bool,
    #[serde(default)]
    pub lock_od: bool,
    #[serde(default)]
    pub lock_cs: bool,
    #[serde(default)]
    pub lock_hp: bool,
    /// 生成时移除转盘。
    #[serde(default)]
    pub no_spinners: bool,
    /// 音频变调开关，原样传给生成器。
    #[serde(default)]
    pub change_pitch: bool,
    /// 仅生成快速预览（默认 true），暂存目录不含完整素材。
    #[serde(default = "default_preview_only")]
    pub preview_only: bool,
    // BPM 过滤与时间段截取，语义由旧版 trainer 生成流程定义。
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub start_time_ms: Option<f64>,
    pub end_time_ms: Option<f64>,
}

fn default_preview_only() -> bool {
    true
}

/// 谱面时间线摘要，供前端绘制预览与参数面板。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    /// 最后一个物件的命中时间，作为谱面时长。
    pub duration_ms: f64,
    /// 非继承计时点列表，元素为 (起点毫秒, BPM)。
    pub bpm_segments: Vec<(f64, f64)>,
    /// 源谱面难度参数。
    pub ar: f32,
    pub od: f32,
    pub cs: f32,
    pub hp: f32,
    /// 物件总数。
    pub object_count: usize,
    /// 谱面模式：0=osu、1=taiko、2=fruits、3=mania。
    pub mode: u8,
    /// 首个非继承计时点的 BPM，作为主 BPM。
    pub primary_bpm: Option<f64>,
    /// 难度应变曲线，物件过多时为空。
    pub strain_series: Vec<StrainSeries>,
    /// 应变曲线首段的起始时间（毫秒）。
    pub strain_section_start_time_ms: f64,
    /// 应变曲线单个分段的长度（毫秒）。
    pub strain_section_length_ms: f64,
}
