use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityMetadata {
    pub public_key: String,
    pub user_id: String,
    pub device_id: String,
    pub display_name: String,
    pub device_name: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub has_identity: bool,
    pub connected: bool,
    pub public_key: Option<String>,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub display_name: Option<String>,
    pub device_name: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubUser {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubDeviceSummary {
    pub id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionResponse {
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub user: HubUser,
    pub device: HubDeviceSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub device_name: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub user: HubUser,
    pub current_device_id: String,
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackOwner {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRating {
    pub average: Option<f64>,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackViewer {
    pub rating: Option<u8>,
    pub favorited: bool,
    pub can_edit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub title: String,
    pub description: String,
    pub owner: PackOwner,
    pub beatmapset_ids: Vec<i32>,
    pub manifest_hash: String,
    pub rating: PackRating,
    pub viewer: Option<PackViewer>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackPreview {
    pub pack: Pack,
    pub locally_available_ids: Vec<i32>,
    pub missing_ids: Vec<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishResult {
    pub id: String,
    pub included: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolvedBeatmap {
    pub id: i32,
    pub checksum: Option<String>,
    pub mode: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolvedBeatmapset {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub creator: String,
    #[serde(default)]
    pub beatmaps: Vec<ResolvedBeatmap>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub folder_id: String,
    pub imported_sets: usize,
    pub imported_entries: usize,
    pub unresolved_sets: usize,
}

#[derive(Debug, Deserialize)]
pub struct ChallengeResponse {
    pub challenge_id: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkTokenResponse {
    pub link_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePackResponse {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ApiError,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
