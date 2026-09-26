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
