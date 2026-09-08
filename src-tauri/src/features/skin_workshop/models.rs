use serde::{Deserialize, Serialize};

use crate::features::local_analysis::LocalSkinSummary;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkshopAssetKind {
    Image,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinAssetVariant {
    pub asset_id: String,
    pub kind: WorkshopAssetKind,
    pub name: String,
    pub logical_path: String,
    pub extension: String,
    pub size: u64,
    pub scale: u8,
    pub frame: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinTreeNode {
    pub part_id: String,
    pub part_key: String,
    pub label: String,
    pub path_segments: Vec<String>,
    pub asset_count: usize,
    pub image_count: usize,
    pub audio_count: usize,
    pub children: Vec<SkinTreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinTree {
    pub skin_resource_id: String,
    pub roots: Vec<SkinTreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinPartPreview {
    pub skin_resource_id: String,
    pub part_key: String,
    pub assets: Vec<SkinAssetVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinAssetPayload {
    pub asset_id: String,
    pub kind: WorkshopAssetKind,
    pub mime_type: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigEntry {
    pub key: String,
    pub value: String,
    pub occurrence: usize,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigSection {
    pub name: String,
    pub entries: Vec<SkinConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigError {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinConfigDocument {
    pub source: String,
    pub sections: Vec<SkinConfigSection>,
    pub errors: Vec<SkinConfigError>,
    pub encoding: String,
    pub newline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkinWorkshopAction {
    ReplaceComponent {
        target_logical_path: String,
        replacement_path: String,
    },
    ReplacePart {
        target_part_key: String,
        source_skin_resource_id: String,
    },
    CopyComponent {
        target_logical_path: String,
        source_skin_resource_id: String,
        source_logical_path: String,
    },
    CopyConfigEntry {
        source_skin_resource_id: String,
        section: String,
        key: String,
        occurrence: usize,
    },
    UpdateConfigSource {
        source: String,
    },
    UpdateConfigEntry {
        section: String,
        key: String,
        occurrence: usize,
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SkinWorkshopWriteMode {
    Overwrite,
    CreateCopy { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkinWorkshopPreset {
    MigrateMania { source_skin_resource_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinWorkshopMutationResult {
    pub name: String,
    pub path: String,
    pub created_copy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PackageState {
    pub summary: LocalSkinSummary,
    pub root: String,
    pub source_path: String,
}
