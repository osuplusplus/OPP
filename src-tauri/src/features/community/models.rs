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
    pub registration_starts_at: Option<String>,
    pub registration_ends_at: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::CommunityTournament;

    #[test]
    fn tournament_contract_preserves_unpublished_dates() {
        let payload = serde_json::json!({
            "id": "00000000-0000-4000-8000-000000000101",
            "title": "Partner Cup", "organizer": "Partner", "ruleset": "osu",
            "summary": "Preparing", "description": "Schedule pending", "requirements": "",
            "poster_url": null, "registration_url": "https://rino.ink/",
            "registration_starts_at": null, "registration_ends_at": null,
            "starts_at": null, "ends_at": null, "status": "upcoming",
            "created_at": "2026-10-08T00:00:00Z", "updated_at": "2026-10-08T00:00:00Z"
        });
        let tournament: CommunityTournament = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(serde_json::to_value(&tournament).unwrap(), payload);
        let scheduled = serde_json::json!({
            "starts_at": "2026-11-01T00:00:00Z",
            "ends_at": "2026-11-02T00:00:00Z"
        });
        let mut payload = payload;
        for (key, value) in scheduled.as_object().unwrap() {
            payload[key] = value.clone();
        }
        let tournament: CommunityTournament = serde_json::from_value(payload).unwrap();
        assert_eq!(
            tournament.starts_at.as_deref(),
            Some("2026-11-01T00:00:00Z")
        );
    }
}
