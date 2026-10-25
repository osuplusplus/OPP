//! Google 免费翻译端点适配器。
//!
//! 使用 `translate_a/single?client=gtx` 非官方免费端点，无需 API key；
//! 端点在国内不可直连，客户端构建时应用代理自动发现（Clash 等）。
//! 响应为嵌套数组分段文本，需按序拼接。

use std::time::Duration;

use crate::error::{CommandError, CommandResult};

use super::super::{
    models::{TranslationOutcome, google_language_code},
    ports::TranslationProvider,
};

/// Google 免费端点适配器；零配置，仅持有一个带可选代理的 HTTP 客户端。
pub(crate) struct GoogleFreeProvider {
    client: reqwest::Client,
}

impl GoogleFreeProvider {
    /// 构建客户端：8 秒超时，发现本机代理时自动挂载（国内访问必需）。
    pub(crate) fn new() -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(5));
        if let Some(proxy_url) = crate::infrastructure::proxy::discover_proxy_url()
            && let Ok(proxy) = reqwest::Proxy::all(&proxy_url)
        {
            builder = builder.proxy(proxy);
        }
        Self {
            client: builder.build().unwrap_or_default(),
        }
    }

    fn upstream_error(status: reqwest::StatusCode) -> CommandError {
        if status.as_u16() == 429 {
            CommandError::new(
                "TRANSLATION_RATE_LIMITED",
                "翻译服务限流，请稍后重试或更换翻译服务",
            )
        } else {
            CommandError::new(
                "TRANSLATION_UPSTREAM_ERROR",
                format!("Google 翻译服务异常（HTTP {status}）"),
            )
        }
    }
}

impl TranslationProvider for GoogleFreeProvider {
    async fn translate(
        &self,
        texts: &[String],
        target: &str,
    ) -> CommandResult<Vec<TranslationOutcome>> {
        let tl = google_language_code(target);
        let mut results = Vec::with_capacity(texts.len());
        // 聊天消息零散且量小，逐条顺序请求即可满足限速要求。
        for text in texts {
            let response = self
                .client
                .get("https://translate.googleapis.com/translate_a/single")
                .query(&[
                    ("client", "gtx"),
                    ("sl", "auto"),
                    ("tl", tl),
                    ("dt", "t"),
                    ("q", text.as_str()),
                ])
                .send()
                .await
                .map_err(|error| {
                    CommandError::new(
                        "TRANSLATION_UPSTREAM_ERROR",
                        format!("Google 翻译请求失败：{error}"),
                    )
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(Self::upstream_error(status));
            }
            let body = response.text().await.map_err(|error| {
                CommandError::new(
                    "TRANSLATION_UPSTREAM_ERROR",
                    format!("Google 翻译响应读取失败：{error}"),
                )
            })?;
            let (translated, detected) = parse_response(&body).ok_or_else(|| {
                CommandError::new("TRANSLATION_PARSE_ERROR", "Google 翻译响应格式无法解析")
            })?;
            results.push(TranslationOutcome {
                text: translated,
                provider: "google_free".into(),
                detected_lang: detected,
            });
        }
        Ok(results)
    }
}

/// 解析端点响应：`[[[译文,原文,…],…],null,"检测语言",…]`，按序拼接分段。
fn parse_response(body: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let segments = value.get(0)?.as_array()?;
    let mut translated = String::new();
    for segment in segments {
        // 每段为 [译文, 原文, ...]；仅个别段可能缺失译文，跳过而非整体失败。
        if let Some(piece) = segment.get(0).and_then(serde_json::Value::as_str) {
            translated.push_str(piece);
        }
    }
    if translated.is_empty() {
        return None;
    }
    let detected = value
        .get(2)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Some((translated, detected))
}

#[cfg(test)]
mod tests {
    use super::parse_response;

    #[test]
    fn parses_segmented_translation() {
        // 多分段译文按序拼接，语言检测字段取第 3 层。
        let body = r#"[[["你","you",null,null,10],["好","好",null,null,10]],null,"en"]"#;
        let (text, detected) = parse_response(body).unwrap();
        assert_eq!(text, "你好");
        assert_eq!(detected, "en");
    }

    #[test]
    fn rejects_empty_or_malformed_payload() {
        assert!(parse_response("[]").is_none());
        assert!(parse_response("not json").is_none());
        assert!(parse_response(r#"[[["","x"]],null,"en"]"#).is_none());
    }
}
