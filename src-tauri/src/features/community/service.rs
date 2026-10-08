use reqwest::Method;
use serde::de::DeserializeOwned;

use super::models::*;
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::community_client::CommunityClient,
};

pub struct CommunityService<'a, C> {
    client: &'a C,
}

impl<'a, C: CommunityClient> CommunityService<'a, C> {
    pub fn new(client: &'a C) -> Self {
        Self { client }
    }

    pub async fn list<T: DeserializeOwned>(
        &self,
        kind: &str,
        query: CommunityQuery,
        mine: bool,
    ) -> CommandResult<CommunityPage<T>> {
        let query = serde_json::to_value(query)?;
        let encoded = {
            let mut encoded = url::form_urlencoded::Serializer::new(String::new());
            if let Some(object) = query.as_object() {
                for (key, value) in object {
                    if let Some(value) = value.as_str() {
                        encoded.append_pair(key, value);
                    } else if let Some(value) = value.as_u64() {
                        encoded.append_pair(key, &value.to_string());
                    }
                }
            }
            encoded.finish()
        };
        let path = format!(
            "/community/{kind}{}?{}",
            if mine { "/mine" } else { "" },
            encoded
        );
        if mine {
            self.client
                .authenticated_json(Method::GET, &path, None)
                .await
        } else {
            self.client.public_get(&path).await
        }
    }

    pub async fn detail<T: DeserializeOwned>(&self, kind: &str, id: &str) -> CommandResult<T> {
        self.client.public_get(&resource(kind, id)?).await
    }

    pub async fn save(&self, id: Option<&str>, input: LobbyInput) -> CommandResult<CommunityLobby> {
        validate_lobby(&input)?;
        let path = match id {
            Some(id) => resource("lobbies", id)?,
            None => "/community/lobbies".into(),
        };
        self.client
            .authenticated_json(
                if id.is_some() {
                    Method::PATCH
                } else {
                    Method::POST
                },
                &path,
                Some(serde_json::to_value(input)?),
            )
            .await
    }

    pub async fn close(&self, id: &str) -> CommandResult<CommunityLobby> {
        self.client
            .authenticated_json(
                Method::POST,
                &format!("{}/close", resource("lobbies", id)?),
                None,
            )
            .await
    }

    pub async fn delete(&self, id: &str) -> CommandResult<()> {
        self.client
            .authenticated_delete(&resource("lobbies", id)?)
            .await
    }
}

fn resource(kind: &str, id: &str) -> CommandResult<String> {
    let id = uuid::Uuid::parse_str(id)
        .map_err(|_| CommandError::new("INVALID_REQUEST", "活动 ID 无效"))?;
    Ok(format!("/community/{kind}/{id}"))
}

fn validate_lobby(input: &LobbyInput) -> CommandResult<()> {
    let invalid = || CommandError::new("INVALID_REQUEST", "请检查标题、描述、活动类型和时间");
    if input.title.trim().is_empty()
        || input.title.chars().count() > 120
        || input.description.trim().is_empty()
        || input.description.chars().count() > 2000
        || !["osu", "taiko", "fruits", "mania"].contains(&input.ruleset.as_str())
        || !["mp", "duel", "ranked", "practice", "other"].contains(&input.activity_type.as_str())
        || input.platform.as_deref().is_some_and(|platform| {
            !["osu", "romai", "vash", "osu_rl", "other"].contains(&platform)
        })
    {
        return Err(invalid());
    }
    let start = chrono::DateTime::parse_from_rfc3339(&input.starts_at).map_err(|_| invalid())?;
    let end = chrono::DateTime::parse_from_rfc3339(&input.ends_at).map_err(|_| invalid())?;
    if end <= start || end <= chrono::Utc::now() {
        return Err(CommandError::new(
            "INVALID_TIME",
            "结束时间必须晚于开始时间和当前时间",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_resource_path_injection() {
        assert!(resource("lobbies", "../admin/tournaments").is_err());
    }
    #[test]
    fn allows_ongoing_lobby_and_rejects_past_end() {
        let now = chrono::Utc::now();
        let mut input = LobbyInput {
            title: "MP".into(),
            description: "一起打图".into(),
            ruleset: "osu".into(),
            activity_type: "mp".into(),
            platform: None,
            starts_at: (now - chrono::Duration::hours(1)).to_rfc3339(),
            ends_at: (now + chrono::Duration::hours(1)).to_rfc3339(),
        };
        assert!(validate_lobby(&input).is_ok());
        input.ends_at = (now - chrono::Duration::minutes(1)).to_rfc3339();
        assert!(validate_lobby(&input).is_err());
    }
}
