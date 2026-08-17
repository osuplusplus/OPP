use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CommandError, CommandResult};

/// Opens NetEase Cloud Music's web search in the user's default browser.
#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn open_netease_music_search(
    app: AppHandle,
    artist: String,
    title: String,
) -> CommandResult<()> {
    let query = [artist.trim(), title.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if query.is_empty() {
        return Err(CommandError::new(
            "NETEASE_EMPTY_QUERY",
            "Song title and artist cannot both be empty.",
        ));
    }

    let search = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("s", &query)
        .append_pair("type", "1")
        .finish();
    let url = format!("https://music.163.com/#/search/m/?{search}");

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| CommandError::new("NETEASE_OPEN_FAILED", error.to_string()))
}
