//! 腾讯云机器翻译（TMT）适配器（国内直连，免费额度 500 万字符/月）。
//!
//! 使用云 API 3.0 的 TC3-HMAC-SHA256 签名；凭据为 `SecretId|SecretKey`。
//! 免费额度按月发放不可结转，超出部分需手动开启后付费才会计费。

use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::error::{CommandError, CommandResult};

use super::super::{
    models::{TranslationOutcome, tencent_language_code},
    ports::TranslationProvider,
};

/// TMT 固定接入参数：全球服务，区域仅用于请求头展示。
const HOST: &str = "tmt.tencentcloudapi.com";
const SERVICE: &str = "tmt";
const ACTION: &str = "TextTranslate";
const API_VERSION: &str = "2018-03-21";
const REGION: &str = "ap-guangzhou";

type HmacSha256 = Hmac<Sha256>;

/// 腾讯云 TMT 适配器；凭据为 `SecretId|SecretKey`。
pub(crate) struct TencentProvider {
    secret_id: String,
    secret_key: String,
    client: reqwest::Client,
}

impl TencentProvider {
    pub(crate) fn new(credential: &str) -> Self {
        // 国内服务直连，不挂代理。
        let client =
            crate::infrastructure::http::http_client(crate::infrastructure::http::HttpClientSpec {
                ua_note: " (tencent-tmt)",
                timeout: Some(Duration::from_secs(10)),
                connect_timeout: Some(Duration::from_secs(5)),
            })
            .unwrap_or_default();
        let (secret_id, secret_key) = split_credential(credential);
        Self {
            secret_id,
            secret_key,
            client,
        }
    }

    fn map_api_error(code: &str, message: &str) -> CommandError {
        let hint = match code {
            "AuthFailure.SignatureFailure" | "AuthFailure.SecretIdNotFound" => {
                "腾讯云签名或密钥无效，请检查 SecretId/SecretKey"
            }
            "AuthFailure.UnauthorizedOperation" => "腾讯云账号未开通机器翻译服务，请到控制台开通",
            "LimitExceeded" => "腾讯云翻译限流（QPS 超限），请稍后重试",
            _ => "腾讯云翻译服务异常",
        };
        CommandError::new(
            "TRANSLATION_UPSTREAM_ERROR",
            format!("{hint}（{code} {message}）"),
        )
    }
}

impl TranslationProvider for TencentProvider {
    async fn translate(
        &self,
        texts: &[String],
        target: &str,
    ) -> CommandResult<Vec<TranslationOutcome>> {
        if self.secret_id.is_empty() || self.secret_key.is_empty() {
            return Err(CommandError::new(
                "TRANSLATION_NOT_CONFIGURED",
                "腾讯云凭据不完整，请在设置中填写 SecretId 与 SecretKey",
            ));
        }
        let to = tencent_language_code(target);
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            let body = serde_json::json!({
                "ProjectId": 0,
                "Source": "auto",
                "SourceText": text,
                "Target": to,
                "UntranslatedText": "",
            })
            .to_string();
            let timestamp = chrono::Utc::now().timestamp();
            let authorization =
                tc3_authorization(&self.secret_id, &self.secret_key, &body, timestamp);
            let response = self
                .client
                .post(format!("https://{HOST}/"))
                .header("Authorization", &authorization)
                .header("Content-Type", "application/json; charset=utf-8")
                .header("X-TC-Action", ACTION)
                .header("X-TC-Timestamp", timestamp.to_string())
                .header("X-TC-Version", API_VERSION)
                .header("X-TC-Region", REGION)
                .body(body)
                .send()
                .await
                .map_err(|error| {
                    CommandError::new(
                        "TRANSLATION_UPSTREAM_ERROR",
                        format!("腾讯云翻译请求失败：{error}"),
                    )
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(CommandError::new(
                    "TRANSLATION_UPSTREAM_ERROR",
                    format!("腾讯云翻译服务异常（HTTP {status}）"),
                ));
            }
            let payload: serde_json::Value = response.json().await.map_err(|error| {
                CommandError::new(
                    "TRANSLATION_UPSTREAM_ERROR",
                    format!("腾讯云翻译响应读取失败：{error}"),
                )
            })?;
            // 业务错误嵌在 200 响应的 Response.Error 中，与成功结构互斥。
            let envelope = payload.get("Response").cloned().unwrap_or_default();
            if let Some(error) = envelope.get("Error") {
                let code = error
                    .get("Code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let message = error
                    .get("Message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                return Err(Self::map_api_error(code, message));
            }
            let translated = envelope
                .get("TargetText")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if translated.is_empty() {
                return Err(CommandError::new(
                    "TRANSLATION_PARSE_ERROR",
                    "腾讯云翻译返回了空译文",
                ));
            }
            let detected = envelope
                .get("Source")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            results.push(TranslationOutcome {
                text: translated,
                provider: "tencent".into(),
                detected_lang: detected,
            });
        }
        Ok(results)
    }
}

/// 拆分 `SecretId|SecretKey` 形式的凭据。
fn split_credential(credential: &str) -> (String, String) {
    match credential.split_once('|') {
        Some((id, key)) => (id.trim().to_owned(), key.trim().to_owned()),
        None => (String::new(), credential.trim().to_owned()),
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    // HMAC-SHA256 接受任意长度密钥，`new_from_slice` 仅在实现缺陷时失败。
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 生成云 API 3.0 的 Authorization 头。签名参与方（body、时间戳）必须与实际
/// 请求逐字节一致，否则服务端返回 SignatureFailure。
pub(crate) fn tc3_authorization(
    secret_id: &str,
    secret_key: &str,
    body: &str,
    timestamp: i64,
) -> String {
    let date = chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(chrono::Utc::now)
        .format("%Y-%m-%d")
        .to_string();
    let action_lower = ACTION.to_ascii_lowercase();
    let hashed_payload = sha256_hex(body.as_bytes());

    // 规范请求：POST /（空查询串）+ 三个小写头 + 已排序签名头列表 + 哈希后的 body。
    let canonical_request = format!(
        "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:{HOST}\nx-tc-action:{action_lower}\n\ncontent-type;host;x-tc-action\n{hashed_payload}"
    );
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{date}/{SERVICE}/tc3_request\n{}",
        sha256_hex(canonical_request.as_bytes()),
    );

    let secret_date = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date.as_bytes());
    let secret_service = hmac_sha256(&secret_date, SERVICE.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let signature = hex_lower(&hmac_sha256(&secret_signing, string_to_sign.as_bytes()));

    format!(
        "TC3-HMAC-SHA256 Credential={secret_id}/{date}/{SERVICE}/tc3_request, SignedHeaders=content-type;host;x-tc-action, Signature={signature}"
    )
}

#[cfg(test)]
mod tests {
    use super::{split_credential, tc3_authorization};

    /// 固定输入产生确定签名：同样的密钥/body/时间戳必须得到同一 Authorization，
    /// 且签名为 64 位十六进制（结构与稳定性检查，防拼接顺序回归）。
    #[test]
    fn tc3_signature_is_deterministic_and_well_formed() {
        let body = r#"{"ProjectId":0,"Source":"auto","SourceText":"hello","Target":"zh","UntranslatedText":""}"#;
        let auth = tc3_authorization("AKIDtest", "secret-key", body, 1_700_000_000);
        let auth_again = tc3_authorization("AKIDtest", "secret-key", body, 1_700_000_000);
        assert_eq!(auth, auth_again);

        let signature = auth.rsplit("Signature=").next().unwrap();
        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert!(auth.contains("Credential=AKIDtest/2023-11-14/tmt/tc3_request"));
        assert!(auth.contains("SignedHeaders=content-type;host;x-tc-action"));
    }

    /// 时间戳或 body 变化必须改变签名（防止签名输入遗漏）。
    #[test]
    fn tc3_signature_changes_with_inputs() {
        let body = r#"{"a":1}"#;
        let base = tc3_authorization("id", "key", body, 1_700_000_000);
        let other_time = tc3_authorization("id", "key", body, 1_700_000_001);
        let other_body = tc3_authorization("id", "key", r#"{"a":2}"#, 1_700_000_000);
        assert_ne!(base, other_time);
        assert_ne!(base, other_body);
    }

    #[test]
    fn splits_pipe_separated_credential() {
        let (id, key) = split_credential("AKIDxxx|secret");
        assert_eq!(id, "AKIDxxx");
        assert_eq!(key, "secret");
    }
}
