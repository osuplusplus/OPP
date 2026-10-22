//! 玩家生涯记录模块：按天将玩家资料、Top 成绩、奖牌与本地回放/截图
//! 快照写入独立的 `career.sqlite3`，并提供日历视图与逐日差异查询。

mod commands;
mod models;
mod service;

pub(crate) use commands::{
    capture_career_snapshot, clear_career_history, get_career_calendar, get_career_day,
    get_career_status,
};
pub(crate) use service::CareerService;
