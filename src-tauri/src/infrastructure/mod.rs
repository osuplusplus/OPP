//! 外部系统适配层：网络、平台、持久化、日志与便携更新。

pub(crate) mod community_client;
pub(crate) mod http;
// 可重建的本地 SQLite 索引：身份识别、连接配置与事务式结构迁移。
pub(crate) mod local_database;
pub(crate) mod logging;
pub(crate) mod osu_api;
pub(crate) mod platform;
// 出站 HTTP 代理自动发现（环境变量 / 系统代理 / 本机 7890）。
pub(crate) mod portable_update;
pub(crate) mod proxy;
pub(crate) mod storage;
// 有限的重任务线程预算：异步运行时只负责等待，计算在独立线程池执行。
pub(crate) mod tasks;

// 可失败、可重试的延迟初始化互斥锁：初始化在首次访问前于调用线程完成。
pub(crate) mod lazy_mutex;

pub(crate) mod flights;

// 日志改造示例仅参与测试编译，保持与真实 API 的类型同步检查。
#[cfg(test)]
#[allow(dead_code)]
mod logging_examples;
