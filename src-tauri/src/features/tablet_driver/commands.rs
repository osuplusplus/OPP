use super::{OtdStatus, service};
use crate::{
    domain::AppSettings,
    error::{CommandError, CommandResult},
    state::AppState,
};
use std::{path::Path, process::Command};
use tauri::State;

fn settings(state: &AppState) -> CommandResult<AppSettings> {
    Ok(state.store.snapshot()?.settings)
}

fn log_command(name: &str) {
    if let Some(logger) = crate::infrastructure::logging::global() {
        logger.log(
            "INFO",
            &format!("tablet_driver.{name}"),
            "OTD command invoked",
        );
    }
}

#[tauri::command]
pub fn get_otd_status(state: State<'_, AppState>) -> CommandResult<OtdStatus> {
    log_command("get_status");
    Ok(service::status(&state.otd, &settings(&state)?))
}

#[tauri::command]
pub fn set_otd_executable(path: String, state: State<'_, AppState>) -> CommandResult<OtdStatus> {
    log_command("set_executable");
    let executable = service::validate_executable(Path::new(&path))?;
    state.store.update(|persisted| {
        persisted.settings.otd_executable_path = Some(executable.display().to_string())
    })?;
    Ok(service::status(&state.otd, &settings(&state)?))
}

#[tauri::command]
pub fn start_otd(state: State<'_, AppState>) -> CommandResult<()> {
    log_command("start");
    service::start(&state.otd, &settings(&state)?)
}

#[tauri::command]
pub fn stop_otd(state: State<'_, AppState>) -> CommandResult<()> {
    log_command("stop");
    service::stop(&state.otd)
}

#[tauri::command]
pub fn read_otd_config_summary(state: State<'_, AppState>) -> CommandResult<OtdStatus> {
    log_command("read_config");
    Ok(service::status(&state.otd, &settings(&state)?))
}

#[tauri::command]
pub fn backup_otd_config(
    destination_dir: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    log_command("backup_config");
    service::backup(&settings(&state)?, Path::new(&destination_dir))
}

#[tauri::command]
pub fn open_otd(state: State<'_, AppState>) -> CommandResult<()> {
    log_command("open");
    let raw = settings(&state)?.otd_executable_path.ok_or_else(|| {
        CommandError::new("OTD_NOT_CONFIGURED", "请先选择 OpenTabletDriver 可执行文件")
    })?;
    let path = service::validate_executable(Path::new(&raw))?;
    Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| CommandError::new("OTD_OPEN_FAILED", e.to_string()))
}

pub fn start_managed_otd(state: &AppState) -> CommandResult<()> {
    service::start(&state.otd, &settings(state)?)
}
