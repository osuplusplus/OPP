//! 翻译域模型：结果契约与各提供方的语言码映射。
//!
//! 内部统一使用简短语言码（zh/en/ja/...），适配器负责换算为各家 API
//! 的具体代码（如百度日语为 `jp`、Google 中文为 `zh-CN`）。

use serde::Serialize;

/// 单条文本的翻译结果；`texts` 与结果按序一一对应。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TranslationOutcome {
    /// 译文。
    pub text: String,
    /// 提供方标识：google_free | tencent | baidu | openai。
    pub provider: String,
    /// 检测到的源语言（内部语言码），无法识别时为空。
    pub detected_lang: String,
}

/// 内部语言码 → Google 免费端点语言码（中文需展开为区域变体）。
pub(crate) fn google_language_code(lang: &str) -> &str {
    match lang {
        "zh" => "zh-CN",
        other => other,
    }
}

/// 内部语言码 → 腾讯云 TMT 语言码（与内部码一致的直接透传）。
pub(crate) fn tencent_language_code(lang: &str) -> &str {
    lang
}

/// 内部语言码 → 百度翻译语言码（部分语言百度使用三字码）。
pub(crate) fn baidu_language_code(lang: &str) -> &'static str {
    match lang {
        "zh" => "zh",
        "en" => "en",
        "ja" => "jp",
        "ko" => "kor",
        "ru" => "ru",
        "es" => "spa",
        "pt" => "pt",
        "de" => "de",
        "fr" => "fra",
        "it" => "it",
        "tr" => "tr",
        "th" => "th",
        "vi" => "vie",
        "id" => "id",
        "ar" => "ara",
        "pl" => "pl",
        _ => "en",
    }
}

/// 内部语言码 → 英文显示名（OpenAI 兼容 LLM 的提示词用）。
pub(crate) fn language_display_name(lang: &str) -> &str {
    match lang {
        "zh" => "Simplified Chinese (简体中文)",
        "en" => "English",
        "ja" => "Japanese (日本語)",
        "ko" => "Korean (한국어)",
        "ru" => "Russian (Русский)",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "de" => "German",
        "fr" => "French",
        "it" => "Italian",
        "tr" => "Turkish",
        "th" => "Thai (ไทย)",
        "vi" => "Vietnamese",
        "id" => "Indonesian",
        "ar" => "Arabic (العربية)",
        "pl" => "Polish",
        _ => "English",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_internal_codes_to_vendor_codes() {
        assert_eq!(google_language_code("zh"), "zh-CN");
        assert_eq!(google_language_code("en"), "en");
        assert_eq!(baidu_language_code("ja"), "jp");
        assert_eq!(baidu_language_code("ko"), "kor");
        assert_eq!(baidu_language_code("fr"), "fra");
        assert_eq!(baidu_language_code("unknown"), "en");
        assert_eq!(tencent_language_code("zh"), "zh");
        assert!(language_display_name("zh").contains("Chinese"));
    }
}
