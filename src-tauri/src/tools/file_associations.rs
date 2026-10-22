//! 文件关联与本地资源定位：管理 `.osz`（谱面）/`.osk`（皮肤）的默认打开
//! 客户端——读写当前用户注册表 `HKCU\Software\Classes`（仅 Windows 支持写
//! 入，读取失败或非 Windows 回退显示 stable）；另提供在资源管理器中定位
//! stable/lazer 本地文件的命令，候选路径经白名单校验，不接受绝对路径与
//! `..` 穿越。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    error::{CommandError, CommandResult},
    features::{game_session::executable, local_analysis::LocalClient},
    state::AppState,
};

/// `.osz`（谱面）与 `.osk`（皮肤）当前关联的默认打开客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultFileClients {
    /// `.osz` 谱面文件的默认打开客户端。
    pub beatmap: LocalClient,
    /// `.osk` 皮肤文件的默认打开客户端。
    pub skin: LocalClient,
}

fn explorer_select(path: &Path) -> CommandResult<()> {
    crate::infrastructure::platform::reveal_path(path)
        .map_err(|error| CommandError::new("EXPLORER_OPEN_FAILED", error.to_string()))
}

/// 在 `base` 目录下解析 `relative` 指向的文件：拒绝绝对路径与含 `..` 的
/// 相对路径，canonicalize 后必须仍位于 `base` 内且是普通文件，防止前端
/// 传入任意路径打开不该访问的文件。
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

/// 在资源管理器中定位本地资源：对安装目录与数据目录分别尝试原路径、
/// `Songs/<相对路径>`、`Skins/<相对路径>` 三种拼法，取第一个真实存在的
/// 文件；逻辑路径由前端从谱面/皮肤列表带入。
#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
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

/// 读取 `.osz`/`.osk` 当前的关联客户端：HKCU `Software\Classes\<扩展名>`
/// 的默认值是 ProgID，OPP 写入的 ProgID 以 `.stable`/`.lazer` 结尾；
/// 被其他程序接管或读取失败时返回 `None`（调用方回退显示 stable）。
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

/// 读取两类文件的默认打开客户端；非 Windows 平台固定返回 stable
///（该平台不支持文件关联设置，见 `set_default_file_client` 的错误说明）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
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

/// 把 `.osz`/`.osk` 的默认打开方式切到指定客户端：在 HKCU `Software\Classes`
/// 下创建/更新 ProgID `OPP.<kind>.<stable|lazer>` 及其 `shell\open\command`
///（命令为 `"<客户端exe>" "%1"`，双击文件即由该客户端打开），再把扩展名
/// 的默认值指到该 ProgID；要求能解析出对应客户端的安装目录与可执行文件。
#[tauri::command]
/// 供前端调用的 Tauri 命令：更新运行时或持久化配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
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
