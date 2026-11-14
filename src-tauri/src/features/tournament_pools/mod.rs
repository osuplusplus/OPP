//! 联赛图池：把线上比赛图池同步为本地收藏夹文件夹，供批量下载与练习管理。
//!
//! 支持三种来源：`rino`（按赛季/阶段拉取 ASC 星域杯官方接口）、`standard`
//! （任意公开 HTTPS 地址上的 OPP 标准 v1 JSON 文档，经 `opp://mappool/import` 导入）
//! 与 `romai`（romai.dev 公开图池库浏览 + 导入，附本地缓存）。
mod commands;
mod models;
mod rino;
mod romai;
mod service;
mod standard;
// `uri` 需要在 lib.rs 启动流程中注册 `opp://` 协议并注入收件箱状态，故提升为 crate 可见。
pub(crate) mod uri;

pub use commands::*;
pub(crate) use models::{TournamentInfo, TournamentPoolRef};
pub use romai::RomaiPoolsClient;
pub use uri::{acknowledge_tournament_link, get_pending_tournament_link};

#[cfg(test)]
mod tests;
