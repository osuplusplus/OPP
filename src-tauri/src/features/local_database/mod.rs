//! 用户自建本地数据库：负责首次初始化、位置配置持久化与失败后的重试/重新定位。
//! SQLite 的实际读写由 `infrastructure/local_database` 提供，本模块只做状态编排。
mod commands;
mod config;
mod models;
mod service;

pub(crate) use commands::{
    get_local_database_status, initialize_local_database, locate_local_database,
    retry_local_database,
};
pub(crate) use service::LocalDatabaseService;
