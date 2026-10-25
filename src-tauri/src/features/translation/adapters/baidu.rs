//! 百度翻译开放平台适配器（国内直连）。
//!
//! 通用翻译 API：`POST /api/trans/vip/translate`，签名规则为
//! `MD5(appid + q + salt + 密钥)`（q 取原始 UTF-8 文本，不做 URL 编码）。
//! 免费额度：标准版 5 万字符/月（注册即用），个人认证高级版 100 万字符/月。

use std::time::Duration;

use crate::error::{CommandError, CommandResult};

use super::super::{
    models::{TranslationOutcome, baidu_language_code},
    ports::TranslationProvider,
};

const ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";

/// 百度翻译适配器；凭据为 `appid|密钥`。
pub(crate) struct BaiduProvider {
    appid: String,
    secret: String,
    client: reqwest::Client,
}

impl BaiduProvider {
    pub(crate) fn new(credential: &str) -> Self {
        // 国内服务直连，不挂代理。
        let client =
            crate::infrastructure::http::http_client(crate::infrastructure::http::HttpClientSpec {
                ua_note: " (baidu-translate)",
                timeout: Some(Duration::from_secs(10)),
                connect_timeout: Some(Duration::from_secs(5)),
            })
            .unwrap_or_default();
        let (appid, secret) = split_credential(credential);
        Self {
            appid,
            secret,
            client,
        }
    }

    fn map_api_error(code: &str, message: &str) -> CommandError {
        let hint = match code {
            "54001" => "百度翻译签名无效，请检查 APP ID 与密钥是否匹配",
            "54003" => "百度翻译限流（QPS 超限），请稍后重试",
            "54004" | "52001" | "52002" => "百度翻译账户额度不足或未开通服务，请到开放平台检查",
            "58001" => "百度翻译不支持该目标语言",
            _ => "百度翻译服务异常",
        };
        CommandError::new(
            "TRANSLATION_UPSTREAM_ERROR",
            format!("{hint}（{code} {message}）"),
        )
    }
}

impl TranslationProvider for BaiduProvider {
    async fn translate(
        &self,
        texts: &[String],
        target: &str,
    ) -> CommandResult<Vec<TranslationOutcome>> {
        if self.appid.is_empty() || self.secret.is_empty() {
            return Err(CommandError::new(
                "TRANSLATION_NOT_CONFIGURED",
                "百度翻译凭据不完整，请在设置中填写 APP ID 与密钥",
            ));
        }
        let to = baidu_language_code(target);
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            let salt = uuid::Uuid::new_v4().simple().to_string();
            let sign = sign_request(&self.appid, text, &salt, &self.secret);
            let response = self
                .client
                .post(ENDPOINT)
                .form(&[
                    ("q", text.as_str()),
                    ("from", "auto"),
                    ("to", to),
                    ("appid", self.appid.as_str()),
                    ("salt", salt.as_str()),
                    ("sign", sign.as_str()),
                ])
                .send()
                .await
                .map_err(|error| {
                    CommandError::new(
                        "TRANSLATION_UPSTREAM_ERROR",
                        format!("百度翻译请求失败：{error}"),
                    )
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(CommandError::new(
                    "TRANSLATION_UPSTREAM_ERROR",
                    format!("百度翻译服务异常（HTTP {status}）"),
                ));
            }
            let body: serde_json::Value = response.json().await.map_err(|error| {
                CommandError::new(
                    "TRANSLATION_UPSTREAM_ERROR",
                    format!("百度翻译响应读取失败：{error}"),
                )
            })?;
            // 错误响应为 {"error_code":"...","error_msg":"..."}，与成功结构互斥。
            if let (Some(code), Some(message)) = (
                body.get("error_code").and_then(serde_json::Value::as_str),
                body.get("error_msg").and_then(serde_json::Value::as_str),
            ) {
                return Err(Self::map_api_error(code, message));
            }
            let entries = body
                .get("trans_result")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    CommandError::new("TRANSLATION_PARSE_ERROR", "百度翻译响应缺少结果字段")
                })?;
            // 多行文本按行返回多条 dst，拼接还原为单条译文。
            let mut translated = String::new();
            for entry in entries {
                if let Some(piece) = entry.get("dst").and_then(serde_json::Value::as_str) {
                    translated.push_str(piece);
                }
            }
            if translated.is_empty() {
                return Err(CommandError::new(
                    "TRANSLATION_PARSE_ERROR",
                    "百度翻译返回了空译文",
                ));
            }
            let detected = body
                .get("from")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            results.push(TranslationOutcome {
                text: translated,
                provider: "baidu".into(),
                detected_lang: detected,
            });
        }
        Ok(results)
    }
}

/// 拆分 `appid|密钥` 形式的凭据；分隔符缺失时整体视为密钥（appid 置空报配置错误）。
fn split_credential(credential: &str) -> (String, String) {
    match credential.split_once('|') {
        Some((appid, secret)) => (appid.trim().to_owned(), secret.trim().to_owned()),
        None => (String::new(), credential.trim().to_owned()),
    }
}

/// 百度签名：MD5(appid + q + salt + 密钥) 的十六进制小写。
fn sign_request(appid: &str, query: &str, salt: &str, secret: &str) -> String {
    use md5::{Digest as _, Md5};

    let raw = format!("{appid}{query}{salt}{secret}");
    let digest = Md5::digest(raw.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{sign_request, split_credential};

    /// 百度官方文档签名示例：appid=2015063000000001、q=apple、salt=1435660288、
    /// 密钥=12345678 → f89f9594663708c1605f3d736d01d2d4。
    #[test]
    fn signs_with_official_document_vector() {
        let sign = sign_request("2015063000000001", "apple", "1435660288", "12345678");
        assert_eq!(sign, "f89f9594663708c1605f3d736d01d2d4");
    }

    #[test]
    fn splits_pipe_separated_credential() {
        let (appid, secret) = split_credential("12345|abcdef");
        assert_eq!(appid, "12345");
        assert_eq!(secret, "abcdef");
        let (appid, secret) = split_credential("only-secret");
        assert_eq!(appid, "");
        assert_eq!(secret, "only-secret");
    }
}
