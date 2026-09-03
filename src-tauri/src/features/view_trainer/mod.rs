//! Trainer +谱面实时预览的独立 feature 边界。
//! 旧的 `features::trainer` 保留作为兼容入口；新 API 从此模块导出。
mod adapters;
pub mod commands;
mod models;
mod service;

pub use models::*;
