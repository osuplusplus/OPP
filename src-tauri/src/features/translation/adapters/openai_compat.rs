//! OpenAI 兼容 LLM 适配器（GLM-4-Flash / DeepSeek / OpenAI / Ollama 等）。
//!
//! LLM 是唯一能可靠处理俚语与 meme 的方案；系统提示内置 osu! 玩家黑话
//! 对照表，批量输入输出用编号行对应。国内直连服务（GLM/DeepSeek/Ollama）
//! 与需要代理的服务（OpenAI）共用本适配器，代理按自动发现结果挂载。

use std::time::Duration;

use crate::error::{CommandError, CommandResult};

use super::super::{
    models::{TranslationOutcome, language_display_name},
    ports::TranslationProvider,
};

/// 面向 osu! 多人聊天场景的翻译系统提示：语域、黑话对照与输出格式约束。
const SYSTEM_PROMPT: &str = "You translate osu! multiplayer lobby chat messages. \
For each numbered input line, output its translation into the requested target language, \
keeping the same line numbering.\n\
Style rules:\n\
- Chat register: short, casual, natural — how gamers actually type. No formal phrasing, no explanations.\n\
- Keep player names, map/artist names, numbers, URLs, emotes and unicode untouched.\n\
- osu!/gaming slang (translate the meaning, keep the vibe): EZ=easy(太简单了), \
farm=farming easy pp maps(刷分图), choke=bottling a run at the end(最后掉了), \
FC=full combo (keep \"FC\"), 727=osu! meme number (keep 727), W=win(赢), L=lose(输), \
nt=nice try, gg/wp/o7 keep as-is, DT/HR/HD/EZ/NM/FM are mod names — keep the letters.\n\
- Chinese gamer slang (zh→en): 下饭=sloppy but funny play, 整活=memeing/doing bits, \
起飞=popping off, 牛/太强了=insane, 菜=trash (bad at the game), 大佬=god/pro, 寄=RIP/it's over, \
破防=tilted, 笑死=lmao, 逆天=unreal, 带带我=carry me, 拉了=played badly, 稳=clean.\n\
- If a line is already in the target language, or is universal shorthand (gg, wp, EZ, o7, 727), \
output it unchanged.\n\
- Output ONLY numbered translation lines (\"1. ...\", \"2. ...\"), exactly one per input line, \
nothing else.";

/// OpenAI 兼容适配器；`base_url` 允许带或不带 `/v1` 版本段。
pub(crate) struct OpenAiCompatProvider {
    api_key: String,
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub(crate) fn new(api_key: &str, base_url: &str, model: &str) -> Self {
        // OpenAI 等海外端点国内需代理；GLM/DeepSeek 等直连流量经本地代理
        // （Clash 等）规则分流后同样可达，统一按发现结果挂载。
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8));
        if let Some(proxy_url) = crate::infrastructure::proxy::discover_proxy_url()
            && let Ok(proxy) = reqwest::Proxy::all(&proxy_url)
        {
            builder = builder.proxy(proxy);
        }
        Self {
            api_key: api_key.trim().to_owned(),
            endpoint: chat_completions_url(base_url),
            model: model.trim().to_owned(),
            client: builder.build().unwrap_or_default(),
        }
    }

    fn http_error(status: reqwest::StatusCode) -> CommandError {
        match status.as_u16() {
            401 | 403 => CommandError::new(
                "TRANSLATION_NOT_CONFIGURED",
                "LLM 服务拒绝了 API Key，请检查密钥是否有效",
            ),
            429 => CommandError::new("TRANSLATION_RATE_LIMITED", "LLM 服务限流，请稍后重试"),
            code => CommandError::new(
                "TRANSLATION_UPSTREAM_ERROR",
                format!("LLM 服务异常（HTTP {code}）"),
            ),
        }
    }
}

impl TranslationProvider for OpenAiCompatProvider {
    async fn translate(
        &self,
        texts: &[String],
        target: &str,
    ) -> CommandResult<Vec<TranslationOutcome>> {
        if self.api_key.is_empty() || self.model.is_empty() {
            return Err(CommandError::new(
                "TRANSLATION_NOT_CONFIGURED",
                "LLM 翻译缺少 API Key 或模型名，请在设置中补全",
            ));
        }
        let user_content = format!(
            "Target language: {}\n{}",
            language_display_name(target),
            texts
                .iter()
                .enumerate()
                .map(|(index, text)| format!("{}. {}", index + 1, text))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "model": self.model,
                "temperature": 0.2,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": user_content },
                ],
            }))
            .send()
            .await
            .map_err(|error| {
                CommandError::new(
                    "TRANSLATION_UPSTREAM_ERROR",
                    format!("LLM 翻译请求失败：{error}"),
                )
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Self::http_error(status));
        }
        let payload: serde_json::Value = response.json().await.map_err(|error| {
            CommandError::new(
                "TRANSLATION_UPSTREAM_ERROR",
                format!("LLM 翻译响应读取失败：{error}"),
            )
        })?;
        let content = payload
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                CommandError::new("TRANSLATION_PARSE_ERROR", "LLM 响应缺少 choices[0].message")
            })?;
        let translated = parse_numbered_lines(content, texts.len()).ok_or_else(|| {
            CommandError::new(
                "TRANSLATION_PARSE_ERROR",
                format!(
                    "LLM 译文行数或编号不匹配（期望 {} 条），请重试或更换模型",
                    texts.len()
                ),
            )
        })?;
        Ok(translated
            .into_iter()
            .map(|text| TranslationOutcome {
                text,
                provider: "openai".into(),
                detected_lang: String::new(),
            })
            .collect())
    }
}

/// 规范化 chat/completions 地址：末段已是 `v<数字>`（v1/v4）则沿用，否则补 `/v1`。
pub(crate) fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let versioned = trimmed.rsplit('/').next().is_some_and(|segment| {
        let mut chars = segment.chars();
        chars.next() == Some('v')
            && !chars.as_str().is_empty()
            && chars.all(|ch| ch.is_ascii_digit())
    });
    if versioned {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

/// 解析编号行输出：优先按 `N.` 前缀对号入座；模型偶尔省略编号时，
/// 行数恰好匹配则按顺序兜底。
pub(crate) fn parse_numbered_lines(body: &str, expected: usize) -> Option<Vec<String>> {
    let mut numbered = std::collections::BTreeMap::new();
    let mut plain: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match split_number_prefix(line) {
            Some((index, rest)) => {
                numbered.insert(index, rest.to_owned());
            }
            None => plain.push(line.to_owned()),
        }
    }
    if numbered.len() == expected && (1..=expected).all(|index| numbered.contains_key(&index)) {
        return Some(
            (1..=expected)
                .map(|index| numbered[&index].clone())
                .collect(),
        );
    }
    if numbered.is_empty() && plain.len() == expected {
        return Some(plain);
    }
    None
}

/// 剥离 `N.`/`N、`/`N:` 等编号前缀；数字后无分隔符（如正文以 "727" 开头）不算编号。
fn split_number_prefix(line: &str) -> Option<(usize, &str)> {
    let digits: String = line.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &line[digits.len()..];
    let stripped = rest.trim_start_matches(['.', '、', ':', '：', ')', '）']);
    if stripped.len() == rest.len() {
        return None;
    }
    let index = digits.parse().ok()?;
    Some((index, stripped.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::{chat_completions_url, parse_numbered_lines};

    #[test]
    fn normalizes_chat_completions_urls() {
        assert_eq!(
            chat_completions_url("https://api.deepseek.com"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://open.bigmodel.cn/api/paas/v4/"),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        assert_eq!(
            chat_completions_url("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
    }

    #[test]
    fn parses_numbered_translations() {
        let body = "1. 你好啊\n2. 727\n3. ez pz";
        assert_eq!(
            parse_numbered_lines(body, 3).unwrap(),
            vec!["你好啊", "727", "ez pz"]
        );
    }

    #[test]
    fn falls_back_to_plain_lines_when_count_matches() {
        assert_eq!(
            parse_numbered_lines("你好啊\n727", 2).unwrap(),
            vec!["你好啊", "727"]
        );
        // 数量不匹配时拒绝，避免错位回填。
        assert!(parse_numbered_lines("你好啊", 2).is_none());
    }

    #[test]
    fn leading_number_without_separator_is_not_a_prefix() {
        // 译文本体以 727 开头时不能被当作第 727 行。
        assert_eq!(
            parse_numbered_lines("1. 727 是一个梗", 1).unwrap(),
            vec!["727 是一个梗"]
        );
    }
}
