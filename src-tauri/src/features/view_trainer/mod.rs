//! 视野训练谱面生成、时间线分析与导入。
//! 旧的 `features::trainer` 保留作为兼容入口；新 API 从此模块导出。
mod adapters;
pub mod commands;
mod models;
mod service;

pub use models::*;
