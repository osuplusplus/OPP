//! 翻译能力端口：所有外部翻译适配器实现的统一契约。
//!
//! 使用静态分发（各适配器实现本 trait，`service::Provider` 枚举按设置选择），
//! 避免 dyn 异步对象与额外的 async-trait 依赖。

use super::models::TranslationOutcome;
use crate::error::CommandResult;

/// 文本批量翻译端口：`texts` 与返回结果按序一一对应。
pub(crate) trait TranslationProvider {
    /// 翻译一批文本到目标语言（内部语言码）。
    fn translate(
        &self,
        texts: &[String],
        target: &str,
    ) -> impl std::future::Future<Output = CommandResult<Vec<TranslationOutcome>>> + Send;
}
