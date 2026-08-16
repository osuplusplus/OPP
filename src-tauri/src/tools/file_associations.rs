use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    game_session::executable,
    local_analysis::LocalClient,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultFileClients {
    pub beatmap: LocalClient,
    pub skin: LocalClient,
}

fn explorer_select(path: &Path) -> CommandResult<()> {
    crate::platform::reveal_path(path)
        .map_err(|error| CommandError::new("EXPLORER_OPEN_FAILED", error.to_string()))
}

fn safe_candidate(base: &Path, relative: &str) -> Option<PathBuf> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    let candidate = base.join(relative_path).canonicalize().ok()?;
    let root = base.canonicalize().ok()?;
    (candidate.starts_with(&root) && candidate.is_file()).then_some(candidate)
}

#[tauri::command]
pub fn open_local_resource_in_explorer(
    client: LocalClient,
    logical_path: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let source = state.local_analysis.source_status(client)?;
    let mut candidates = Vec::new();
    for root in source
        .install_root
        .into_iter()
        .chain(source.data_root.into_iter())
    {
        let base = PathBuf::from(root);
        candidates.push((base.clone(), logical_path.clone()));
        candidates.push((base.clone(), format!("Songs/{logical_path}")));
        candidates.push((base, format!("Skins/{logical_path}")));
    }
    candidates
        .into_iter()
        .find_map(|(base, relative)| safe_candidate(&base, &relative))
        .map_or_else(
            || {
                Err(CommandError::new(
                    "LOCAL_RESOURCE_NOT_FOUND",
                    "未找到对应的本地资源文件",
                ))
            },
            |candidate| explorer_select(&candidate),
        )
}

#[cfg(windows)]
fn registry_client(kind: &str) -> Option<LocalClient> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let classes = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Classes")
        .ok()?;
    let extension = if kind == "skin" { ".osk" } else { ".osz" };
    let prog_id: String = classes.open_subkey(extension).ok()?.get_value("").ok()?;
    if prog_id.ends_with(".lazer") {
        Some(LocalClient::Lazer)
    } else if prog_id.ends_with(".stable") {
        Some(LocalClient::Stable)
    } else {
        None
    }
}

#[tauri::command]
pub fn get_default_file_clients() -> CommandResult<DefaultFileClients> {
    #[cfg(windows)]
    {
        Ok(DefaultFileClients {
            beatmap: registry_client("beatmap").unwrap_or(LocalClient::Stable),
            skin: registry_client("skin").unwrap_or(LocalClient::Stable),
        })
    }
    #[cfg(not(windows))]
    {
        Ok(DefaultFileClients {
            beatmap: LocalClient::Stable,
            skin: LocalClient::Stable,
        })
    }
}

#[tauri::command]
pub fn set_default_file_client(
    kind: String,
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let extension = match kind.as_str() {
            "skin" => ".osk",
            "beatmap" => ".osz",
            _ => {
                return Err(CommandError::new(
                    "INVALID_FILE_KIND",
                    "只支持谱面和 Skin 文件",
                ));
            }
        };
        let source = state.local_analysis.source_status(client)?;
        let root = source
            .install_root
            .ok_or_else(|| CommandError::new("GAME_NOT_FOUND", "未找到对应 osu! 客户端安装目录"))?;
        let executable = executable(client, &root)
            .ok_or_else(|| CommandError::new("GAME_NOT_FOUND", "安装目录中未找到 osu!.exe"))?;
        let classes = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey("Software\\Classes")
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?
            .0;
        let prog_id = format!(
            "OPP.{}.{}",
            kind,
            if client == LocalClient::Stable {
                "stable"
            } else {
                "lazer"
            }
        );
        let (file_type, _) = classes
            .create_subkey(&prog_id)
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?;
        file_type
            .set_value(
                "",
                &format!(
                    "OPP {} ({})",
                    if extension == ".osz" {
                        "Beatmap"
                    } else {
                        "Skin"
                    },
                    if client == LocalClient::Stable {
                        "Stable"
                    } else {
                        "Lazer"
                    }
                ),
            )
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?;
        let (command, _) = file_type
            .create_subkey("shell\\open\\command")
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?;
        command
            .set_value("", &format!("\"{}\" \"%1\"", executable.display()))
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?;
        let (extension_key, _) = classes
            .create_subkey(extension)
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?;
        extension_key
            .set_value("", &prog_id)
            .map_err(|error| CommandError::new("REGISTRY_WRITE_FAILED", error.to_string()))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (kind, client, state);
        Err(CommandError::new(
            "FILE_ASSOCIATION_UNSUPPORTED",
            "文件默认打开设置仅支持 Windows",
        ))
    }
}
