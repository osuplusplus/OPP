//! 社区功能服务层：无状态封装列表/详情与房间的增删改，
//! 统一经 `CommunityClient` 访问后端。
use reqwest::Method;
use serde::de::DeserializeOwned;

use super::models::*;
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::community_client::CommunityClient,
};

/// 社区服务，本身无状态，按请求借用底层客户端。
pub struct CommunityService<'a, C> {
    client: &'a C,
}

impl<'a, C: CommunityClient> CommunityService<'a, C> {
    pub fn new(client: &'a C) -> Self {
        Self { client }
    }

    /// 分页列出指定类型的资源（如 `tournaments`、`lobbies`）。
    ///
    /// `mine` 为 `true` 时改走认证接口，只取当前用户创建的内容；
    /// 查询参数仅序列化字符串与数字，`None` 字段不进入 URL。
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

    /// 获取指定类型资源的详情。
    pub async fn detail<T: DeserializeOwned>(&self, kind: &str, id: &str) -> CommandResult<T> {
        self.client.public_get(&resource(kind, id)?).await
    }

    /// 创建（`id` 为 `None`）或更新房间，提交前先做本地校验。
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

    /// 关闭房间。
    pub async fn close(&self, id: &str) -> CommandResult<CommunityLobby> {
        self.client
            .authenticated_json(
                Method::POST,
                &format!("{}/close", resource("lobbies", id)?),
                None,
            )
            .await
    }

    /// 删除房间。
    pub async fn delete(&self, id: &str) -> CommandResult<()> {
        self.client
            .authenticated_delete(&resource("lobbies", id)?)
            .await
    }
}

/// 校验 ID 为合法 UUID 后再拼接资源路径，防止路径注入。
fn resource(kind: &str, id: &str) -> CommandResult<String> {
    let id = uuid::Uuid::parse_str(id)
        .map_err(|_| CommandError::new("INVALID_REQUEST", "活动 ID 无效"))?;
    Ok(format!("/community/{kind}/{id}"))
}

/// 房间输入的客户端侧校验：长度、枚举值与时间合法性。
///
/// 允许已开始的房间，但结束时间必须同时晚于开始时间与当前时间。
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
    /// 非 UUID 的 ID 必须被拒绝，防止拼出越权路径。
    #[test]
    fn rejects_resource_path_injection() {
        assert!(resource("lobbies", "../admin/tournaments").is_err());
    }
    /// 进行中的房间合法，结束时间早于现在的房间非法。
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
