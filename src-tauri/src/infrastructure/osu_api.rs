//! osu! 官方 API v2 客户端：OAuth 令牌的交换、刷新与吊销，以及资料、成绩、
//! 谱面等查询端点；带认证的 GET 统一经 `authorized_get` 处理认证头与错误映射。
use std::time::Duration;

use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::{
    domain::{OwnProfile, Ruleset, Score, TokenResponse},
    error::{CommandError, CommandResult},
    infrastructure::http::{HttpClientSpec, http_client},
    infrastructure::logging::{finish_span, global},
};

const API_BASE_URL: &str = "https://osu.ppy.sh/api/v2";
const TOKEN_URL: &str = "https://osu.ppy.sh/oauth/token";

/// osu! API v2 客户端；基地址与令牌端点拆成字段，便于测试注入本地服务。
pub struct OsuApi {
    /// 统一 UA 与超时配置的 reqwest 客户端。
    client: reqwest::Client,
    /// API v2 基地址，测试中可替换为本地地址。
    api_base_url: String,
    /// OAuth 令牌端点。
    token_url: String,
}

impl OsuApi {
    /// 创建指向官方 API 的客户端，并统一配置请求超时与 TLS 实现。
    pub fn new() -> CommandResult<Self> {
        let client = http_client(HttpClientSpec {
            timeout: Some(Duration::from_secs(120)),
            ua_note: " (all-in-one osu! toolkit)",
            connect_timeout: None,
        })?;
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

    /// 用授权码换取令牌（`authorization_code` 授权类型）。
    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
    ) -> CommandResult<TokenResponse> {
        let span = global().map(|logger| logger.operation("osu_api", "exchange_code"));
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
            .map_err(|error| CommandError::from_error("NETWORK_ERROR", error));
        let result = match response {
            Ok(response) => Self::parse_response(response, "AUTH_EXCHANGE_FAILED").await,
            Err(error) => Err(error),
        };
        finish_span(span, result)
    }

    /// 用刷新令牌换取新的访问令牌（`refresh_token` 授权类型）。
    pub async fn refresh_token(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> CommandResult<TokenResponse> {
        let span = global().map(|logger| logger.operation("osu_api", "refresh_token"));
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
            .map_err(|error| CommandError::from_error("NETWORK_ERROR", error));
        let result = match response {
            Ok(response) => Self::parse_response(response, "TOKEN_REFRESH_FAILED").await,
            Err(error) => Err(error),
        };
        finish_span(span, result)
    }

    /// 获取当前令牌对应用户在指定游戏模式的资料。
    pub async fn get_own_profile(
        &self,
        access_token: &str,
        ruleset: Ruleset,
    ) -> CommandResult<OwnProfile> {
        let url = format!("{}/me/{ruleset}", self.api_base_url);
        self.authorized_get(&url, access_token).await
    }

    /// 分页获取用户成绩；`Recent` 分类显式附带 `include_fails=0` 排除失败
    /// 成绩，其余分类沿用服务端默认值。
    pub async fn get_user_scores(
        &self,
        access_token: &str,
        user_id: u64,
        ruleset: Ruleset,
        category: crate::domain::ScoreCategory,
        offset: u32,
        limit: u8,
    ) -> CommandResult<Vec<Score>> {
        let include_fails = if category == crate::domain::ScoreCategory::Recent {
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

    /// 获取 bp 榜前 100 条成绩。
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
            crate::domain::ScoreCategory::Best,
            0,
            100,
        )
        .await
    }

    /// 获取最近 100 条成绩（不含失败）。
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
            crate::domain::ScoreCategory::Recent,
            0,
            100,
        )
        .await
    }

    /// 按调用方给出的键值对搜索谱面集，参数原样拼进查询串。
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

    /// 获取整个谱面集（含全部难度）的元数据。
    pub async fn get_beatmapset(
        &self,
        access_token: &str,
        beatmapset_id: u64,
    ) -> CommandResult<Value> {
        let url = format!("{}/beatmapsets/{beatmapset_id}", self.api_base_url);
        self.authorized_get(&url, access_token).await
    }

    /// 获取单个难度（beatmap）的元数据。
    pub async fn get_beatmap(&self, access_token: &str, beatmap_id: u64) -> CommandResult<Value> {
        let url = format!("{}/beatmaps/{beatmap_id}", self.api_base_url);
        self.authorized_get(&url, access_token).await
    }

    /// 按谱面文件的 MD5 校验和反查难度元数据，供收藏下载流程核对本地文件。
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

    /// 吊销当前访问令牌（`DELETE /oauth/tokens/current`）；断开账户连接且
    /// 调用方选择吊销时使用，失败由调用方降级为警告。
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

    /// 所有带认证 GET 的公共入口：附加 Bearer 令牌、固定 `x-api-version` 与
    /// 中文语言偏好，并按端点路径记录操作日志。
    async fn authorized_get<T: DeserializeOwned>(
        &self,
        url: &str,
        access_token: &str,
    ) -> CommandResult<T> {
        let span = global().map(|logger| {
            let endpoint = Url::parse(url)
                .map(|value| value.path().to_string())
                .unwrap_or_else(|_| "<invalid-url>".into());
            logger.operation("osu_api", format!("authorized_get {endpoint}"))
        });
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
            .map_err(|error| CommandError::from_error("NETWORK_ERROR", error));
        let result = match response {
            Ok(response) => Self::parse_response(response, "API_ERROR").await,
            Err(error) => Err(error),
        };
        finish_span(span, result)
    }

    /// 校验状态码后反序列化响应体；非 2xx 先经 `map_status` 转成领域错误。
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

    /// 把 HTTP 状态映射为前端可识别的错误码：401 要求重新登录、403 拒绝、
    /// 429 限速（透传服务端 `retry-after` 秒数）、5xx 视为服务暂不可用。
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

    // 端点响应中的未知字段应保留进 extra，而不是让反序列化失败。
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
