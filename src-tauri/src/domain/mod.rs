//! 跨功能共享的领域模型与默认配置。

mod defaults;
mod models;

// 领域模型扁平导出，调用方统一经 `crate::domain::` 引用；`defaults` 仅供 `models` 内部使用。
pub(crate) use models::*;
