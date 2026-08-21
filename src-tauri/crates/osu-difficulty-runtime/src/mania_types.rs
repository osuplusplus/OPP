use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const MANIA_DIFFICULTY_DIMENSIONS: usize = 8;
pub const MANIA_PATTERN_DIMENSIONS: usize = 6;
pub const MANIA_STRUCTURE_DIMENSIONS: usize = 10;
pub const MANIA_STYLE_DIMENSIONS: usize = MANIA_PATTERN_DIMENSIONS + MANIA_STRUCTURE_DIMENSIONS;
pub const MANIA_VECTOR_DIMENSIONS: usize = MANIA_DIFFICULTY_DIMENSIONS + MANIA_STYLE_DIMENSIONS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct ManiaDifficultyVector {
    pub speed: f32,
    pub hand_stream: f32,
    pub jack: f32,
    pub chordjack: f32,
    pub technical: f32,
    pub stamina: f32,
    pub long_note: f32,
    pub course: f32,
}

impl ManiaDifficultyVector {
    pub const fn as_array(self) -> [f32; MANIA_DIFFICULTY_DIMENSIONS] {
        [
            self.speed,
            self.hand_stream,
            self.jack,
            self.chordjack,
            self.technical,
            self.stamina,
            self.long_note,
            self.course,
        ]
    }

    pub const fn from_array(value: [f32; MANIA_DIFFICULTY_DIMENSIONS]) -> Self {
        Self {
            speed: value[0],
            hand_stream: value[1],
            jack: value[2],
            chordjack: value[3],
            technical: value[4],
            stamina: value[5],
            long_note: value[6],
            course: value[7],
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct ManiaStyleVector {
    pub stream: f32,
    pub chordstream: f32,
    pub jacks: f32,
    pub coordination: f32,
    pub density: f32,
    pub wildcard: f32,
    pub chord_rate: f32,
    pub large_chord_rate: f32,
    pub rotation_rate: f32,
    pub anchor_rate: f32,
    pub rhythm_entropy: f32,
    pub transition_entropy: f32,
    pub ln_note_ratio: f32,
    pub hold_occupancy: f32,
    pub hybrid_row_ratio: f32,
    pub peak_to_sustain_gap: f32,
}

impl ManiaStyleVector {
    pub const fn pattern_array(self) -> [f32; MANIA_PATTERN_DIMENSIONS] {
        [
            self.stream,
            self.chordstream,
            self.jacks,
            self.coordination,
            self.density,
            self.wildcard,
        ]
    }

    pub const fn structure_array(self) -> [f32; MANIA_STRUCTURE_DIMENSIONS] {
        [
            self.chord_rate,
            self.large_chord_rate,
            self.rotation_rate,
            self.anchor_rate,
            self.rhythm_entropy,
            self.transition_entropy,
            self.ln_note_ratio,
            self.hold_occupancy,
            self.hybrid_row_ratio,
            self.peak_to_sustain_gap,
        ]
    }

    pub const fn as_array(self) -> [f32; MANIA_STYLE_DIMENSIONS] {
        let patterns = self.pattern_array();
        let structure = self.structure_array();
        [
            patterns[0],
            patterns[1],
            patterns[2],
            patterns[3],
            patterns[4],
            patterns[5],
            structure[0],
            structure[1],
            structure[2],
            structure[3],
            structure[4],
            structure[5],
            structure[6],
            structure[7],
            structure[8],
            structure[9],
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct ManiaBaseFeatures {
    pub bpm: f32,
    pub length_seconds: f32,
    pub active_length_seconds: f32,
    pub note_count: f32,
    pub row_count: f32,
    pub avg_nps: f32,
    pub peak_nps: f32,
    pub break_density: f32,
    pub sv_change_rate: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ManiaModeFamily {
    #[default]
    Rc,
    Hb,
    Mix,
    Ln,
}

impl ManiaModeFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rc => "RC",
            Self::Hb => "HB",
            Self::Mix => "Mix",
            Self::Ln => "LN",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum ManiaPattern {
    #[default]
    Stream,
    Chordstream,
    Jacks,
    Coordination,
    Density,
    Wildcard,
}

impl ManiaPattern {
    pub const ALL: [Self; MANIA_PATTERN_DIMENSIONS] = [
        Self::Stream,
        Self::Chordstream,
        Self::Jacks,
        Self::Coordination,
        Self::Density,
        Self::Wildcard,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stream => "Stream",
            Self::Chordstream => "Chordstream",
            Self::Jacks => "Jacks",
            Self::Coordination => "Coordination",
            Self::Density => "Density",
            Self::Wildcard => "Wildcard",
        }
    }

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManiaBeatmapMetadata {
    pub beatmap_id: u64,
    pub beatmapset_id: u64,
    pub checksum: String,
    pub artist: String,
    pub title: String,
    pub version: String,
    pub creator: String,
    pub online_url: String,
    pub key_count: u8,
    pub mode_family: ManiaModeFamily,
    pub dominant_pattern: ManiaPattern,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct ManiaRawFeatureRecord {
    pub beatmap_id: u64,
    pub beatmapset_id: u64,
    pub difficulty: ManiaDifficultyVector,
    pub style: ManiaStyleVector,
    pub base: ManiaBaseFeatures,
    pub key_count: u8,
    pub mode_family: ManiaModeFamily,
    pub dominant_pattern: ManiaPattern,
    pub analyzer_version: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct ManiaFeatureRecord {
    pub beatmap_id: u64,
    pub beatmapset_id: u64,
    pub difficulty: ManiaDifficultyVector,
    pub style: ManiaStyleVector,
    pub base: ManiaBaseFeatures,
    pub difficulty_percentile: f32,
    pub difficulty_band: u8,
    pub key_count: u8,
    pub mode_family: ManiaModeFamily,
    pub dominant_pattern: ManiaPattern,
    pub analyzer_version: u32,
    pub normalization_version: u32,
}

impl ManiaFeatureRecord {
    pub fn searchable_vector(self) -> [f32; MANIA_VECTOR_DIMENSIONS] {
        let difficulty = self.difficulty.as_array();
        let style = self.style.as_array();
        let mut vector = [0.0; MANIA_VECTOR_DIMENSIONS];
        vector[..MANIA_DIFFICULTY_DIMENSIONS].copy_from_slice(&difficulty);
        vector[MANIA_DIFFICULTY_DIMENSIONS..].copy_from_slice(&style);
        vector
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManiaSimilarityQuery {
    pub result_limit: usize,
    pub include_same_set: bool,
}

impl Default for ManiaSimilarityQuery {
    fn default() -> Self {
        Self {
            result_limit: 20,
            include_same_set: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ManiaDistanceComponents {
    pub skill: f32,
    pub pattern: f32,
    pub structure: f32,
    pub difficulty: f32,
    pub context: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManiaSimilarityResult {
    pub beatmap_id: u64,
    pub beatmapset_id: u64,
    pub artist: String,
    pub title: String,
    pub version: String,
    pub key_count: u8,
    pub mode_family: ManiaModeFamily,
    pub dominant_pattern: ManiaPattern,
    pub difficulty_percentile: f32,
    pub difficulty_band: u8,
    pub final_distance: f32,
    pub components: ManiaDistanceComponents,
}

/// Runtime query options. This alias intentionally preserves the exact v1
/// upstream query contract while using the standard runtime naming scheme.
pub type ManiaQueryOptions = ManiaSimilarityQuery;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManiaQueryTarget {
    pub metadata: ManiaBeatmapMetadata,
    pub record: ManiaFeatureRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManiaQueryResult {
    pub metadata: ManiaBeatmapMetadata,
    pub record: ManiaFeatureRecord,
    pub final_distance: f32,
    pub components: ManiaDistanceComponents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManiaDatasetInfo {
    pub record_count: usize,
    pub records_by_key_count: BTreeMap<u8, usize>,
    pub analyzer_version: u32,
    pub normalization_version: u32,
    pub algorithm_id: String,
    /// Unix seconds of the newest metadata record included in the index.
    pub data_cutoff_at: Option<i64>,
    pub supports_dynamic_weighting: bool,
}
