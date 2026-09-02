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
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub external_fingerprint: Option<String>,
    #[serde(default)]
    pub last_read_at: Option<String>,
    #[serde(default)]
    pub backup_path: Option<String>,
    #[serde(default)]
    pub backup_fingerprint: Option<String>,
    #[serde(default)]
    pub backup_confirmed_at: Option<String>,
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
    #[serde(default)]
    pub sources: Vec<CollectionSourceSyncStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSourceSyncStatus {
    pub client: LocalClient,
    pub available: bool,
    pub external_changed: bool,
    pub pending_write: bool,
    pub backup_available: bool,
    pub backup_confirmed: bool,
    pub backup_count: usize,
    pub latest_backup: Option<String>,
}
impl Default for CollectionSourceSyncStatus {
    fn default() -> Self {
        Self {
            client: LocalClient::Stable,
            available: false,
            external_changed: false,
            pending_write: false,
            backup_available: false,
            backup_confirmed: false,
            backup_count: 0,
            latest_backup: None,
        }
    }
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
pub struct CollectionManagerStatus {
    pub configured: bool,
    pub available: bool,
    pub protocol_version: Option<String>,
    pub version: Option<String>,
    pub operations: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionBackupStatus {
    pub client: LocalClient,
    pub target: Option<String>,
    pub fingerprint: Option<String>,
    pub backups: Vec<String>,
    pub latest: Option<String>,
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
