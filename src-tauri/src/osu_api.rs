use std::time::Duration;

use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::{
    error::{CommandError, CommandResult},
    models::{OwnProfile, Ruleset, Score, TokenResponse},
};

const API_BASE_URL: &str = "https://osu.ppy.sh/api/v2";
const TOKEN_URL: &str = "https://osu.ppy.sh/oauth/token";

pub struct OsuApi {
    client: reqwest::Client,
    api_base_url: String,
    token_url: String,
}

impl OsuApi {
    /// 创建指向官方 API 的客户端，并统一配置请求超时与 TLS 实现。
    pub fn new() -> CommandResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(concat!(
                "OPP/",
                env!("CARGO_PKG_VERSION"),
                " (all-in-one osu! toolkit)"
            ))
            .build()
            .map_err(|error| CommandError::network(error.to_string()))?;
        Ok(Self {
            client,
            api_base_url: API_BASE_URL.into(),
            token_url: TOKEN_URL.into(),
        })
    }

    #[cfg(test)]
    /// 使用指定基地址创建客户端，主要用于集成测试或受控代理环境。
    pub fn with_base_url(base_url: String) -> CommandResult<Self> {
        let mut api = Self::new()?;
        api.api_base_url = base_url;
        Ok(api)
    }

    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
    ) -> CommandResult<TokenResponse> {
        let response = self
            .client
            .post(&self.token_url)
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("code", code),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        Self::parse_response(response, "AUTH_EXCHANGE_FAILED").await
    }

    pub async fn refresh_token(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> CommandResult<TokenResponse> {
        let response = self
            .client
            .post(&self.token_url)
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        Self::parse_response(response, "TOKEN_REFRESH_FAILED").await
    }

    pub async fn get_own_profile(
        &self,
        access_token: &str,
        ruleset: Ruleset,
    ) -> CommandResult<OwnProfile> {
        let url = format!("{}/me/{ruleset}", self.api_base_url);
        self.authorized_get(&url, access_token).await
    }

    pub async fn get_user_scores(
        &self,
        access_token: &str,
        user_id: u64,
        ruleset: Ruleset,
        category: crate::models::ScoreCategory,
        offset: u32,
        limit: u8,
    ) -> CommandResult<Vec<Score>> {
        let include_fails = if category == crate::models::ScoreCategory::Recent {
            "&include_fails=0"
        } else {
            ""
        };
        let url = format!(
            "{}/users/{user_id}/scores/{category}?mode={ruleset}&limit={limit}&offset={offset}&legacy_only=0{include_fails}",
            self.api_base_url
        );
        self.authorized_get(&url, access_token).await
    }

    pub async fn get_best_scores(
        &self,
        access_token: &str,
        user_id: u64,
        ruleset: Ruleset,
    ) -> CommandResult<Vec<Score>> {
        self.get_user_scores(
            access_token,
            user_id,
            ruleset,
            crate::models::ScoreCategory::Best,
            0,
            100,
        )
        .await
    }

    pub async fn get_recent_scores(
        &self,
        access_token: &str,
        user_id: u64,
        ruleset: Ruleset,
    ) -> CommandResult<Vec<Score>> {
        self.get_user_scores(
            access_token,
            user_id,
            ruleset,
            crate::models::ScoreCategory::Recent,
            0,
            100,
        )
        .await
    }

    pub async fn search_beatmapsets(
        &self,
        access_token: &str,
        parameters: &[(String, String)],
    ) -> CommandResult<Value> {
        let mut url = Url::parse(&format!("{}/beatmapsets/search", self.api_base_url))
            .map_err(|error| CommandError::new("INVALID_URL", error.to_string()))?;
        url.query_pairs_mut().extend_pairs(
            parameters
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
        self.authorized_get(url.as_str(), access_token).await
    }

    pub async fn get_beatmapset(
        &self,
        access_token: &str,
        beatmapset_id: u64,
    ) -> CommandResult<Value> {
        let url = format!("{}/beatmapsets/{beatmapset_id}", self.api_base_url);
        self.authorized_get(&url, access_token).await
    }

    pub async fn lookup_beatmap_by_checksum(
        &self,
        access_token: &str,
        checksum: &str,
    ) -> CommandResult<Value> {
        let mut url = Url::parse(&format!("{}/beatmaps/lookup", self.api_base_url))
            .map_err(|error| CommandError::new("INVALID_URL", error.to_string()))?;
        url.query_pairs_mut().append_pair("checksum", checksum);
        self.authorized_get(url.as_str(), access_token).await
    }

    pub async fn revoke_current_token(&self, access_token: &str) -> CommandResult<()> {
        let response = self
            .client
            .delete(format!("{}/oauth/tokens/current", self.api_base_url))
            .bearer_auth(access_token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::map_status(&response, "TOKEN_REVOKE_FAILED"))
        }
    }

    async fn authorized_get<T: DeserializeOwned>(
        &self,
        url: &str,
        access_token: &str,
    ) -> CommandResult<T> {
        // 所有携带访问令牌的 GET 请求集中在此处，保证认证头与错误映射保持一致。
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .header("Accept", "application/json")
            .header("Accept-Language", "zh-CN")
            .header("x-api-version", "20220705")
            .send()
            .await
            .map_err(|error| CommandError::network(error.to_string()))?;
        Self::parse_response(response, "API_ERROR").await
    }

    async fn parse_response<T: DeserializeOwned>(
        response: Response,
        default_code: &str,
    ) -> CommandResult<T> {
        // 非成功响应先保留服务端错误体，再映射为前端可识别的领域错误。
        if !response.status().is_success() {
            return Err(Self::map_status(&response, default_code));
        }
        response
            .json::<T>()
            .await
            .map_err(|error| CommandError::new("INVALID_DATA", error.to_string()))
    }

    fn map_status(response: &Response, default_code: &str) -> CommandError {
        let status = response.status();
        match status {
            StatusCode::UNAUTHORIZED => CommandError::auth_required(),
            StatusCode::FORBIDDEN => CommandError::new("PERMISSION_DENIED", "osu! 拒绝了此请求"),
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok());
                CommandError::new("RATE_LIMITED", "请求过于频繁，请稍后再试")
                    .retry_after(retry_after)
            }
            status if status.is_server_error() => {
                CommandError::new("SERVER_ERROR", format!("osu! 服务暂时不可用（{status}）"))
            }
            _ => CommandError::new(default_code, format!("osu! 请求失败（{status}）")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_profile_and_preserves_optional_fields() {
        let server = axum::Router::new().route(
            "/me/osu",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "id": 7,
                    "username": "test",
                    "avatar_url": "https://example.test/avatar.png",
                    "country_code": "CN",
                    "statistics_rulesets": {},
                    "future_field": 123
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, server)
                .await
                .expect("test server should run");
        });

        let api = OsuApi::with_base_url(format!("http://{address}")).expect("client");
        let profile = api
            .get_own_profile("test-token", Ruleset::Osu)
            .await
            .expect("profile");

        assert_eq!(profile.username, "test");
        assert_eq!(
            profile.extra.get("future_field"),
            Some(&serde_json::json!(123))
        );
    }
}
