//! 聊天翻译：多提供方文本翻译（Google 免费端点 / 腾讯云 / 百度 / OpenAI 兼容 LLM）。
//!
//! - `models`：结果契约与语言码映射
//! - `ports`：提供方统一接口（静态分发）
//! - `adapters`：各外部翻译 API 的隔离适配
//! - `service`：提供方解析、LRU 缓存与翻译必要性判定
//! - `commands`：Tauri 命令暴露
//!
//! 入站方向（IRC 收到的聊天）由后端自动翻译；出站方向（用户输入）由
//! 前端经 `translate_texts` 按需调用。凭据只入系统密钥环，不落 state.json。

pub(crate) mod adapters;
pub(crate) mod commands;
pub(crate) mod models;
pub(crate) mod ports;
pub(crate) mod service;

pub use commands::{translate_texts, translation_set_credential, translation_test};
pub use service::TranslationService;
