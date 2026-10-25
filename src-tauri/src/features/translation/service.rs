//! 翻译服务：按设置解析提供方、LRU 结果缓存与是否需要翻译的判定。
//!
//! 出站方向（用户输入框）由前端经命令调用；入站方向（IRC 聊天消息）
//! 由 overlay 在收到消息后自动触发，失败静默降级为仅展示原文。

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::{
    domain::AppSettings,
    error::{CommandError, CommandResult},
    state::AppState,
};

use super::{
    adapters::{
        baidu::BaiduProvider, google_free::GoogleFreeProvider, openai_compat::OpenAiCompatProvider,
        tencent::TencentProvider,
    },
    models::TranslationOutcome,
    ports::TranslationProvider,
};

/// 缓存条数上限：聊天场景高频重复（gg/EZ/客套话），命中即免请求。
const CACHE_CAPACITY: usize = 512;
/// 单次翻译的条数与单条长度上限（IRC 行长天然受 ~450 字节限制）。
const MAX_BATCH: usize = 20;
const MAX_TEXT_CHARS: usize = 500;

struct CacheEntry {
    key: String,
    outcome: TranslationOutcome,
}

/// 按设置构造的提供方；枚举静态分发，避免 dyn 异步对象。
enum Provider {
    GoogleFree(GoogleFreeProvider),
    Tencent(TencentProvider),
    Baidu(BaiduProvider),
    OpenAi(OpenAiCompatProvider),
}

impl Provider {
    async fn translate(
        &self,
        texts: &[String],
        target: &str,
    ) -> CommandResult<Vec<TranslationOutcome>> {
        match self {
            Self::GoogleFree(provider) => provider.translate(texts, target).await,
            Self::Tencent(provider) => provider.translate(texts, target).await,
            Self::Baidu(provider) => provider.translate(texts, target).await,
            Self::OpenAi(provider) => provider.translate(texts, target).await,
        }
    }
}

/// 翻译服务：持有跨消息的 LRU 结果缓存，随 `AppState` 共享。
pub struct TranslationService {
    /// LRU 缓存，队头最新；仅缓存成功结果。
    cache: Mutex<VecDeque<CacheEntry>>,
}

impl Default for TranslationService {
    fn default() -> Self {
        Self::new()
    }
}

impl TranslationService {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(VecDeque::with_capacity(CACHE_CAPACITY)),
        }
    }

    fn cache_key(text: &str, target: &str) -> String {
        // NUL 不会出现在正常聊天文本中，可安全做分隔符。
        format!("{target}\u{0}{text}")
    }

    /// 命中时把条目提升到队头（LRU 语义）；锁中毒按未命中处理。
    fn cache_get(&self, key: &str) -> Option<TranslationOutcome> {
        let mut cache = self.cache.lock().ok()?;
        let index = cache.iter().position(|entry| entry.key == key)?;
        let entry = cache.remove(index)?;
        let outcome = entry.outcome.clone();
        cache.push_front(entry);
        Some(outcome)
    }

    fn cache_put(&self, key: String, outcome: TranslationOutcome) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.push_front(CacheEntry { key, outcome });
            while cache.len() > CACHE_CAPACITY {
                cache.pop_back();
            }
        }
    }

    /// 按设置构造当前提供方；off 或配置缺失返回带引导信息的错误。
    fn resolve_provider(
        &self,
        settings: &AppSettings,
        credential: Option<&str>,
    ) -> CommandResult<Provider> {
        let credential = credential.unwrap_or_default();
        match settings.translation_provider.as_str() {
            "off" => Err(CommandError::new(
                "TRANSLATION_DISABLED",
                "翻译功能已关闭，请在设置 > 游戏覆盖中开启",
            )),
            "google_free" => Ok(Provider::GoogleFree(GoogleFreeProvider::new())),
            "tencent" => Ok(Provider::Tencent(TencentProvider::new(credential))),
            "baidu" => Ok(Provider::Baidu(BaiduProvider::new(credential))),
            "openai" => {
                let base = settings
                    .translation_api_base_url
                    .as_deref()
                    .unwrap_or_default()
                    .trim();
                if base.is_empty() {
                    return Err(CommandError::new(
                        "TRANSLATION_NOT_CONFIGURED",
                        "LLM 翻译未配置 API 地址，请在设置 > 游戏覆盖中填写",
                    ));
                }
                let model = settings
                    .translation_api_model
                    .as_deref()
                    .unwrap_or_default();
                Ok(Provider::OpenAi(OpenAiCompatProvider::new(
                    credential, base, model,
                )))
            }
            other => Err(CommandError::new(
                "TRANSLATION_NOT_CONFIGURED",
                format!("未知的翻译服务：{other}"),
            )),
        }
    }

    /// 翻译入口（带缓存）：命中的条目直接返回，未命中的批量请求后回填。
    pub async fn translate(
        &self,
        settings: &AppSettings,
        credential: Option<&str>,
        texts: &[String],
        target: &str,
    ) -> CommandResult<Vec<TranslationOutcome>> {
        let mut results: Vec<Option<TranslationOutcome>> = Vec::with_capacity(texts.len());
        let mut misses: Vec<usize> = Vec::new();
        for (index, text) in texts.iter().enumerate() {
            match self.cache_get(&Self::cache_key(text, target)) {
                Some(outcome) => results.push(Some(outcome)),
                None => {
                    results.push(None);
                    misses.push(index);
                }
            }
        }
        if !misses.is_empty() {
            let provider = self.resolve_provider(settings, credential)?;
            let missed_texts: Vec<String> =
                misses.iter().map(|&index| texts[index].clone()).collect();
            let translated = provider.translate(&missed_texts, target).await?;
            // 数量不匹配说明适配器实现有缺陷，宁可整体失败也不错位回填。
            if translated.len() != missed_texts.len() {
                return Err(CommandError::new(
                    "TRANSLATION_PARSE_ERROR",
                    "翻译结果数量与输入不匹配",
                ));
            }
            crate::log_info!(
                "translation",
                "翻译完成：{} 条，提供方 {}",
                translated.len(),
                translated
                    .first()
                    .map(|outcome| outcome.provider.as_str())
                    .unwrap_or_default()
            );
            for (offset, outcome) in translated.into_iter().enumerate() {
                let index = misses[offset];
                self.cache_put(Self::cache_key(&texts[index], target), outcome.clone());
                results[index] = Some(outcome);
            }
        }
        results
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| CommandError::new("TRANSLATION_PARSE_ERROR", "翻译结果不完整"))
    }
}

/// 判断消息是否需要翻译：已是目标语言文字、纯符号/数字、纯链接时跳过。
pub(crate) fn should_translate(text: &str, target: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // 无字母文字（纯表情/符号/数字）无需翻译。
    if !trimmed.chars().any(|ch| ch.is_alphabetic()) {
        return false;
    }
    // 无空格的裸链接无需翻译。
    let looks_like_url = trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("www.");
    if looks_like_url && !trimmed.contains(' ') {
        return false;
    }
    let has_cjk = trimmed.chars().any(|ch| {
        matches!(
            ch,
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}'
        )
    });
    let has_cyrillic = trimmed
        .chars()
        .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch));
    match target {
        // 目标中日文：已含 CJK 表意的消息视为无需翻译（MP 聊天里的 CJK
        // 绝大多数就是中文本身；假名居多的日文不含表意外壳时仍会翻译）。
        "zh" | "ja" => !has_cjk,
        // 目标拉丁等文字：仅当包含 CJK/西里尔等非拉丁文字时才翻译。
        _ => has_cjk || has_cyrillic,
    }
}

/// 出站翻译（用户输入框 → 目标语言）；`target_override` 缺省取设置值。
pub async fn translate_outgoing(
    state: &AppState,
    texts: Vec<String>,
    target_override: Option<String>,
) -> CommandResult<Vec<TranslationOutcome>> {
    if texts.is_empty() {
        return Err(CommandError::new(
            "TRANSLATION_INVALID_TEXT",
            "没有需要翻译的文本",
        ));
    }
    if texts.len() > MAX_BATCH
        || texts
            .iter()
            .any(|text| text.chars().count() > MAX_TEXT_CHARS)
    {
        return Err(CommandError::new(
            "TRANSLATION_INVALID_TEXT",
            format!("单次翻译最多 {MAX_BATCH} 条、每条 {MAX_TEXT_CHARS} 字"),
        ));
    }
    let settings = state
        .store
        .settings_snapshot()
        .map_err(|_| CommandError::new("SETTINGS_LOCKED", "设置状态不可用"))?;
    let target = target_override
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| settings.translation_outgoing_target_lang.clone());
    let credential = state.credentials.get_translation_credential()?;
    state
        .translation
        .translate(&settings, credential.as_deref(), &texts, &target)
        .await
}

/// 入站自动翻译（IRC 聊天消息）：未启用/无需翻译/失败一律返回 `None`，
/// 聊天流本身绝不因翻译失败而中断。
pub async fn translate_incoming(state: &AppState, text: &str) -> Option<String> {
    let settings = state.store.settings_snapshot().ok()?;
    if !settings.translation_enabled || settings.translation_provider == "off" {
        return None;
    }
    if !should_translate(text, &settings.translation_target_lang) {
        return None;
    }
    let credential = state
        .credentials
        .get_translation_credential()
        .ok()
        .flatten();
    let mut results = state
        .translation
        .translate(
            &settings,
            credential.as_deref(),
            &[text.to_owned()],
            &settings.translation_target_lang,
        )
        .await
        .ok()?;
    let translated = results.pop()?.text;
    // 译文与原文相同（已是目标语言/模型原样保留）时不展示，避免视觉噪音。
    (translated != text).then_some(translated)
}

#[cfg(test)]
mod tests {
    use super::{CACHE_CAPACITY, TranslationService, should_translate};
    use crate::features::translation::models::TranslationOutcome;

    fn outcome(text: &str) -> TranslationOutcome {
        TranslationOutcome {
            text: text.into(),
            provider: "test".into(),
            detected_lang: String::new(),
        }
    }

    #[test]
    fn heuristically_skips_translatable_texts() {
        // 目标中文：含 CJK 的跳过，纯拉丁/西里尔的翻译。
        assert!(!should_translate("哈哈哈 太搞了", "zh"));
        assert!(!should_translate("666", "zh"));
        assert!(!should_translate("😂😂", "zh"));
        assert!(!should_translate("https://osu.ppy.sh/b/123", "zh"));
        assert!(should_translate("gg wp everyone", "zh"));
        assert!(should_translate("привет", "zh"));
        // 目标英文：纯拉丁跳过，含 CJK 才翻译。
        assert!(!should_translate("gg wp", "en"));
        assert!(should_translate("下饭了兄弟", "en"));
    }

    #[test]
    fn cache_is_lru_and_bounded() {
        let service = TranslationService::new();
        for index in 0..CACHE_CAPACITY {
            service.cache_put(format!("key{index}"), outcome(&format!("t{index}")));
        }
        // 触碰最旧的条目使其变新，再插入一条挤掉次旧项。
        assert_eq!(service.cache_get("key0").map(|o| o.text), Some("t0".into()));
        service.cache_put("extra".into(), outcome("e"));
        assert!(service.cache_get("key1").is_none());
        assert!(service.cache_get("key0").is_some());
        assert!(service.cache_get("extra").is_some());
    }
}
