use serde::{Deserialize, Serialize};

use crate::features::local_analysis::LocalClient;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CollectionSource {
    Opp,
    Stable,
    Lazer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionEntry {
    pub id: String,
    pub beatmap_id: Option<i32>,
    pub beatmapset_id: Option<i32>,
    pub checksum: Option<String>,
    pub ruleset: Option<String>,
    pub difficulty_name: String,
    pub title: String,
    pub artist: String,
    pub creator: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionFolder {
    pub id: String,
    pub name: String,
    pub creator: String,
    pub created_at: String,
    pub updated_at: String,
    pub source: CollectionSource,
    pub read_only: bool,
    pub pending_write: bool,
    pub entries: Vec<CollectionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionCandidate {
    pub beatmap_id: Option<i32>,
    pub beatmapset_id: Option<i32>,
    pub checksum: Option<String>,
    pub ruleset: Option<String>,
    pub difficulty_name: String,
    pub title: String,
    pub artist: String,
    pub creator: String,
    #[serde(default)]
    pub local_client: Option<LocalClient>,
    #[serde(default)]
    pub local_resource_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSourceStatus {
    pub client: LocalClient,
    pub available: bool,
    pub read_only: bool,
    pub message: String,
    pub refreshed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSnapshot {
    pub folders: Vec<CollectionFolder>,
    pub sources: Vec<CollectionSourceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSyncStatus {
    pub available: bool,
    pub in_sync: bool,
    pub pending_changes: bool,
    pub game_changed: bool,
    pub missing_downloadable_count: usize,
    pub missing_unresolved_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSharePreview {
    pub name: String,
    pub creator: String,
    pub created_at: String,
    pub exported_at: String,
    pub entries: Vec<CollectionEntry>,
    pub available_count: usize,
    pub downloadable_count: usize,
    pub unresolved_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionDownloadItem {
    pub beatmapset_id: i32,
    pub artist: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionWriteResult {
    pub written_folders: usize,
    pub skipped_entries: usize,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInstallResult {
    pub installed_sets: usize,
    pub resolved_entries: usize,
    pub unresolved_entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionOpenResult {
    pub opened: usize,
    pub failed: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionTaskProgress {
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub message: String,
}
