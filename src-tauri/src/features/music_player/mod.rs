//! 本地音乐播放模块：构建曲目目录、维护播放队列与音频输出，
//! 集成系统媒体控制，并负责主窗口与迷你播放器窗口之间的切换。
mod catalog;
// 目录构建：把多客户端扫描出的候选音频合并为可播放的曲目。
mod commands;
// Tauri 命令层：负责参数反序列化并转发到播放服务。
mod media;
// 系统媒体控制集成（Windows SMTC / Linux MPRIS）。
mod models;
// 前后端共享的数据契约与序列化结构。
mod queue;
// 队列导航状态机：顺序、列表循环、单曲循环与洗牌袋随机。
mod service;
// 播放服务编排：工作线程驱动音频输出、持久化与状态发布。
pub(crate) mod windows;
// 窗口集成：主窗口与迷你播放器的切换、布局约束与会话恢复。
pub use commands::*;
pub use service::MusicRuntime;
#[cfg(test)]
mod tests;
