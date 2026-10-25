//! 翻译命令层：参数校验、服务调用与错误转换。
//! 聊天原文与译文不进入日志，仅记录条数与提供方。

use serde::Serialize;
use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

use super::{models::TranslationOutcome, service};

#[tauri::command]
/// 供前端调用的 Tauri 命令：出站翻译（聊天输入框 → 目标语言）。
pub async fn translate_texts(
    texts: Vec<String>,
    target_lang: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<TranslationOutcome>> {
    crate::log_info!("translation", "出站翻译请求：{} 条", texts.len());
    service::translate_outgoing(state.inner(), texts, target_lang).await
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：保存/清除翻译服务凭据（仅存系统密钥环）。
pub fn translation_set_credential(
    credential: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let value = credential
        .map(|input| input.trim().to_owned())
        .unwrap_or_default();
    crate::log_info!(
        "translation",
        "翻译凭据更新：{}",
        if value.is_empty() {
            "清除"
        } else {
            "已保存"
        }
    );
    state.credentials.set_translation_credential(&value)
}

/// 连通性测试结果。
#[derive(Debug, Serialize)]
pub struct TranslationTestResult {
    pub provider: String,
    pub target: String,
    pub translated_text: String,
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：翻译一条样例验证配置（设置页测试按钮）。
pub async fn translation_test(state: State<'_, AppState>) -> CommandResult<TranslationTestResult> {
    crate::log_info!("translation", "连通性测试开始");
    // 样例刻意包含游戏黑话，顺带检验俚语处理是否自然。
    let sample = "gg wp everyone, that map was pure farm";
    let settings = state
        .store
        .settings_snapshot()
        .map_err(|_| CommandError::new("SETTINGS_LOCKED", "设置状态不可用"))?;
    if settings.translation_provider == "off" {
        return Err(CommandError::new(
            "TRANSLATION_DISABLED",
            "翻译功能已关闭，请先选择翻译服务",
        ));
    }
    let target = settings.translation_target_lang.clone();
    let credential = state.credentials.get_translation_credential()?;
    let mut results = state
        .translation
        .translate(
            &settings,
            credential.as_deref(),
            &[sample.to_owned()],
            &target,
        )
        .await?;
    let outcome = results
        .pop()
        .ok_or_else(|| CommandError::new("TRANSLATION_PARSE_ERROR", "测试未返回结果"))?;
    crate::log_info!("translation", "连通性测试通过：提供方 {}", outcome.provider);
    Ok(TranslationTestResult {
        provider: outcome.provider,
        target,
        translated_text: outcome.text,
    })
}
