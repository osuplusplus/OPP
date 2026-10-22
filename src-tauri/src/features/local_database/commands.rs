//! 本地数据库 Tauri 命令：均经 `blocking_io` 线程池执行，避免阻塞异步运行时。
use tauri::{AppHandle, Manager};

use super::models::LocalDatabaseStatus;
use crate::{
    error::CommandResult,
    infrastructure::{
        logging::{finish_span, global},
        tasks,
    },
    state::AppState,
};

/// 数据库状态变化后把本地分析的缓存索引迁移入库；
/// 迁移失败只体现在存储状态中，不回滚有效的数据库配置。
fn sync_indexes(
    app: &AppHandle,
    status: LocalDatabaseStatus,
) -> CommandResult<LocalDatabaseStatus> {
    let span = global().map(|logger| logger.operation("local_database", "migrate_cached_indexes"));
    // Index migration failures remain visible in storage status without undoing valid configuration.
    let _ = finish_span(
        span,
        app.state::<AppState>()
            .local_analysis
            .migrate_cached_indexes(),
    );
    Ok(status)
}

/// 查询数据库当前状态（首次调用触发加载）。
#[tauri::command]
pub(crate) async fn get_local_database_status(
    app: AppHandle,
) -> CommandResult<LocalDatabaseStatus> {
    tasks::blocking_io("get_local_database_status", move || {
        let span = global().map(|logger| logger.operation("local_database.command", "get_status"));
        finish_span(span, app.state::<AppState>().local_database.status())
    })
    .await?
}

/// 在指定目录完成首次初始化，成功后顺带迁移缓存索引。
#[tauri::command]
pub(crate) async fn initialize_local_database(
    app: AppHandle,
    directory: String,
) -> CommandResult<LocalDatabaseStatus> {
    tasks::blocking_io("initialize_local_database", move || {
        let span = global().map(|logger| logger.operation("local_database.command", "initialize"));
        let result = app
            .state::<AppState>()
            .local_database
            .initialize(&directory)
            .and_then(|status| sync_indexes(&app, status));
        finish_span(span, result)
    })
    .await?
}

/// 关闭并重新加载配置与数据库（目录恢复、文件解锁后重试）。
#[tauri::command]
pub(crate) async fn retry_local_database(app: AppHandle) -> CommandResult<LocalDatabaseStatus> {
    tasks::blocking_io("retry_local_database", move || {
        let span = global().map(|logger| logger.operation("local_database.command", "retry"));
        let result = app
            .state::<AppState>()
            .local_database
            .retry()
            .and_then(|status| sync_indexes(&app, status));
        finish_span(span, result)
    })
    .await?
}

/// 到指定目录重新定位既有数据库（UUID 必须匹配原库）。
#[tauri::command]
pub(crate) async fn locate_local_database(
    app: AppHandle,
    directory: String,
) -> CommandResult<LocalDatabaseStatus> {
    tasks::blocking_io("locate_local_database", move || {
        let span = global().map(|logger| logger.operation("local_database.command", "locate"));
        let result = app
            .state::<AppState>()
            .local_database
            .locate(&directory)
            .and_then(|status| sync_indexes(&app, status));
        finish_span(span, result)
    })
    .await?
}
