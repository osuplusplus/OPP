use crate::features::local_analysis::{LocalClient, StrainSeries};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewTrainerRequest {
    pub client: LocalClient,
    pub resource_id: String,
    pub rate: f64,
    #[serde(default)]
    pub bpm_locked: bool,
    #[serde(default)]
    pub target_bpm: Option<f64>,
    pub ar: f32,
    pub od: f32,
    pub cs: f32,
    pub hp: f32,
    #[serde(default)]
    pub scale_ar: bool,
    #[serde(default)]
    pub scale_od: bool,
    #[serde(default)]
    pub lock_ar: bool,
    #[serde(default)]
    pub lock_od: bool,
    #[serde(default)]
    pub lock_cs: bool,
    #[serde(default)]
    pub lock_hp: bool,
    #[serde(default)]
    pub no_spinners: bool,
    #[serde(default)]
    pub change_pitch: bool,
    #[serde(default = "default_preview_only")]
    pub preview_only: bool,
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub start_time_ms: Option<f64>,
    pub end_time_ms: Option<f64>,
}

fn default_preview_only() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub duration_ms: f64,
    pub bpm_segments: Vec<(f64, f64)>,
    pub ar: f32,
    pub od: f32,
    pub cs: f32,
    pub hp: f32,
    pub object_count: usize,
    pub mode: u8,
    pub primary_bpm: Option<f64>,
    pub strain_series: Vec<StrainSeries>,
    pub strain_section_start_time_ms: f64,
    pub strain_section_length_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub session_id: String,
    pub duration_ms: f64,
    pub included_objects: usize,
}
