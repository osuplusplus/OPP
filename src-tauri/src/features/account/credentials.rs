//! 账号域凭据存储：基于 `keyring` 访问 Windows 凭据管理器与 Linux Secret Service，
//! Client Secret 与 OAuth 令牌只落系统密钥环，不进入可同步的持久化文件。
use chrono::{DateTime, Utc};
use keyring::{Entry, Error as KeyringError};

use crate::{
    domain::TokenSet,
    error::{CommandError, CommandResult},
};

/// keyring 服务名，作为所有账号域条目的统一命名空间。
const SERVICE: &str = "com.opp.desktop";
// OAuth 条目：Client Secret 与令牌分条存储；`osu-oauth-tokens` 是旧版合并 JSON
// 条目，仅在读取时迁移为分条后删除。
const CLIENT_SECRET_ENTRY: &str = "osu-oauth-client-secret";
const LEGACY_TOKENS_ENTRY: &str = "osu-oauth-tokens";
const ACCESS_TOKEN_ENTRY: &str = "osu-oauth-access-token";
const REFRESH_TOKEN_ENTRY: &str = "osu-oauth-refresh-token";
const TOKEN_EXPIRY_ENTRY: &str = "osu-oauth-token-expiry";
// 第三方集成口令条目：OBS WebSocket、osu! IRC 与聊天翻译。
const OBS_WEBSOCKET_PASSWORD_ENTRY: &str = "obs-websocket-password";
const OSU_IRC_TOKEN_ENTRY: &str = "osu-irc-token";
// 翻译凭据按提供方解释内容：baidu="appid|密钥"、tencent="SecretId|SecretKey"、
// openai=API key；google_free 无需凭据。
const TRANSLATION_CREDENTIAL_ENTRY: &str = "translation-credential";

/// 账号域统一的密钥环访问层；自身无状态，每次读写都按服务名加条目名即时定位。
#[derive(Default)]
pub struct CredentialStore;

impl CredentialStore {
    pub fn set_client_secret(&self, secret: &str) -> CommandResult<()> {
        // Client Secret 仅保存到系统密钥环，永不写入可同步的 state.json。
        Self::write_password(CLIENT_SECRET_ENTRY, secret)
    }

    /// 读取 Client Secret；`None` 表示尚未配置，调用方据此提示补全凭据。
    pub fn get_client_secret(&self) -> CommandResult<Option<String>> {
        Self::read_password(CLIENT_SECRET_ENTRY)
    }

    /// 将 `TokenSet` 分条写入密钥环并清理旧版合并条目；写入顺序见函数内说明。
    pub fn set_tokens(&self, tokens: &TokenSet) -> CommandResult<()> {
        // 分项存储令牌与过期时间，便于后续在不暴露令牌内容的前提下判断是否需要刷新。
        // Windows Credential Manager limits the size of a single credential.
        // OAuth access and refresh tokens must therefore be stored separately.
        Self::delete(ACCESS_TOKEN_ENTRY)?;
        Self::write_secret(TOKEN_EXPIRY_ENTRY, &tokens.expires_at.to_rfc3339())?;

        match tokens.refresh_token.as_deref() {
            Some(refresh_token) => Self::write_secret(REFRESH_TOKEN_ENTRY, refresh_token)?,
            None => Self::delete(REFRESH_TOKEN_ENTRY)?,
        }

        // Write the access token last so it acts as the commit marker.
        Self::write_secret(ACCESS_TOKEN_ENTRY, &tokens.access_token)?;
        Self::delete(LEGACY_TOKENS_ENTRY)
    }

    /// 读取当前令牌；发现旧版合并条目时现场迁移为分条存储。
    pub fn get_tokens(&self) -> CommandResult<Option<TokenSet>> {
        // 任一令牌字段缺失即视为未登录，避免构造不完整的认证状态。
        if let Some(access_token) = Self::read_secret(ACCESS_TOKEN_ENTRY)? {
            let expires_at = Self::read_secret(TOKEN_EXPIRY_ENTRY)?
                .ok_or_else(|| {
                    CommandError::new(
                        "INVALID_TOKEN_METADATA",
                        "安全存储中的 Token 过期时间缺失，请重新登录",
                    )
                })
                .and_then(|value| Self::parse_expiry(&value))?;

            return Ok(Some(TokenSet {
                access_token,
                refresh_token: Self::read_secret(REFRESH_TOKEN_ENTRY)?,
                expires_at,
            }));
        }

        let Some(value) = Self::read_password(LEGACY_TOKENS_ENTRY)? else {
            return Ok(None);
        };
        let tokens: TokenSet = serde_json::from_str(&value)?;
        self.set_tokens(&tokens)?;
        Ok(Some(tokens))
    }

    /// 清空全部分条令牌与旧版合并条目，使用户回到未登录状态。
    pub fn clear_tokens(&self) -> CommandResult<()> {
        Self::delete(ACCESS_TOKEN_ENTRY)?;
        Self::delete(REFRESH_TOKEN_ENTRY)?;
        Self::delete(TOKEN_EXPIRY_ENTRY)?;
        Self::delete(LEGACY_TOKENS_ENTRY)
    }

    /// 保存 OBS WebSocket 密码；传空字符串表示删除已保存的密码。
    pub fn set_obs_websocket_password(&self, password: &str) -> CommandResult<()> {
        if password.is_empty() {
            Self::delete(OBS_WEBSOCKET_PASSWORD_ENTRY)
        } else {
            Self::write_password(OBS_WEBSOCKET_PASSWORD_ENTRY, password)
        }
    }

    pub fn get_obs_websocket_password(&self) -> CommandResult<Option<String>> {
        Self::read_password(OBS_WEBSOCKET_PASSWORD_ENTRY)
    }

    /// osu! IRC 令牌（多人补缺监听用），仅存系统密钥环。
    pub fn set_osu_irc_token(&self, token: &str) -> CommandResult<()> {
        if token.is_empty() {
            Self::delete(OSU_IRC_TOKEN_ENTRY)
        } else {
            Self::write_password(OSU_IRC_TOKEN_ENTRY, token)
        }
    }

    pub fn get_osu_irc_token(&self) -> CommandResult<Option<String>> {
        Self::read_password(OSU_IRC_TOKEN_ENTRY)
    }

    /// 聊天翻译服务凭据（内容格式由所选提供方决定）；传空字符串表示删除。
    pub fn set_translation_credential(&self, credential: &str) -> CommandResult<()> {
        if credential.is_empty() {
            Self::delete(TRANSLATION_CREDENTIAL_ENTRY)
        } else {
            Self::write_password(TRANSLATION_CREDENTIAL_ENTRY, credential)
        }
    }

    pub fn get_translation_credential(&self) -> CommandResult<Option<String>> {
        Self::read_password(TRANSLATION_CREDENTIAL_ENTRY)
    }

    // 口令类条目（Client Secret、OBS、IRC）的读取路径。
    fn read_password(account: &str) -> CommandResult<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(Self::map_error(error)),
        }
    }

    // 令牌类条目的读取路径，与 `write_secret` 的字节写入方式配对。
    fn read_secret(account: &str) -> CommandResult<Option<String>> {
        match Self::entry(account)?.get_secret() {
            Ok(value) => String::from_utf8(value).map(Some).map_err(|_| {
                CommandError::new(
                    "INVALID_TOKEN_METADATA",
                    "安全存储中的 Token 编码无效，请重新登录",
                )
            }),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(Self::map_error(error)),
        }
    }

    // 删除保持幂等：条目不存在同样视为成功。
    fn delete(account: &str) -> CommandResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(Self::map_error(error)),
        }
    }

    fn write_password(account: &str, value: &str) -> CommandResult<()> {
        Self::entry(account)?
            .set_password(value)
            .map_err(Self::map_error)
    }

    fn write_secret(account: &str, value: &str) -> CommandResult<()> {
        Self::entry(account)?
            .set_secret(value.as_bytes())
            .map_err(Self::map_error)
    }

    fn parse_expiry(value: &str) -> CommandResult<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(value)
            .map(|date| date.with_timezone(&Utc))
            .map_err(|_| {
                CommandError::new(
                    "INVALID_TOKEN_METADATA",
                    "安全存储中的 Token 过期时间无效，请重新登录",
                )
            })
    }

    fn entry(account: &str) -> CommandResult<Entry> {
        Entry::new(SERVICE, account).map_err(Self::map_error)
    }

    fn map_error(error: KeyringError) -> CommandError {
        // 密钥环实现细节不应泄露给前端；统一映射为可恢复的本地凭据错误。
        CommandError::new(
            "CREDENTIAL_STORE_ERROR",
            format!("系统凭据存储不可用：{error}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::CredentialStore;

    // 旧条目可能带本地时区偏移，读取时必须归一为 UTC。
    #[test]
    fn parses_stored_expiry_as_utc() {
        let parsed = CredentialStore::parse_expiry("2026-07-25T12:34:56+08:00").unwrap();

        assert_eq!(
            parsed,
            Utc.with_ymd_and_hms(2026, 7, 25, 4, 34, 56).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_stored_expiry() {
        let error = CredentialStore::parse_expiry("not-a-date").unwrap_err();

        assert_eq!(error.code, "INVALID_TOKEN_METADATA");
    }
}
