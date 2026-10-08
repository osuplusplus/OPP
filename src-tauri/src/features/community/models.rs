use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommunityPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub server_time: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CommunityQuery {
    pub q: Option<String>,
    pub ruleset: Option<String>,
    pub activity_type: Option<String>,
    pub platform: Option<String>,
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LobbyInput {
    pub title: String,
    pub description: String,
    pub ruleset: String,
    pub activity_type: String,
    pub platform: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommunityLobby {
    pub id: String,
    pub owner_user_id: String,
    pub osu_user_id: String,
    pub osu_username: String,
    pub title: String,
    pub description: String,
    pub ruleset: String,
    pub activity_type: String,
    pub platform: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
    pub closed_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommunityTournament {
    pub id: String,
    pub title: String,
    pub organizer: String,
    pub ruleset: String,
    pub summary: String,
    pub description: String,
    pub requirements: String,
    pub poster_url: Option<String>,
    pub registration_url: Option<String>,
    pub registration_starts_at: String,
    pub registration_ends_at: String,
    pub starts_at: String,
    pub ends_at: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}
