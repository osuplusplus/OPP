use std::path::Path;

use super::models::BeatmapDownloadItem;
use super::tools::sanitize_filename;

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn download_file_name(item: &BeatmapDownloadItem, suggested: Option<&str>) -> String {
    // 以谱面集 ID 作为稳定前缀，避免镜像给出相同文件名时发生覆盖。
    let fallback = format!(
        "{} {} - {}.osz",
        item.beatmapset_id,
        item.artist.trim(),
        item.title.trim()
    );
    let suggested = suggested
        .and_then(|name| Path::new(name).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| name.to_ascii_lowercase().ends_with(".osz"))
        .unwrap_or(&fallback);
    let with_id = if suggested.starts_with(&item.beatmapset_id.to_string()) {
        suggested.to_string()
    } else {
        format!("{} {suggested}", item.beatmapset_id)
    };
    sanitize_filename(&with_id)
}

/// Downloads a beatmapset through the selected mirror, then tries the other registered mirrors.
/// Sayobot is OPP's preferred mirror. Hinai itself implements a multi-source cascade; the remaining
/// attempts are an additional OPP fallback if a public endpoint is unavailable.
pub async fn download_with_adapters<F>(
    state: &AppState,
    beatmapset_id: u64,
    provider: &str,
    include_video: bool,
    cancel: &AtomicBool,
    mut on_progress: F,
) -> CommandResult<super::providers::ProviderBytes>
where
    F: FnMut(u64, Option<u64>),
{
    let adapters = match provider {
        "sayobot" => ["sayobot", "hinai", "catboy", "nerinyan"],
        "hinai" => ["hinai", "sayobot", "catboy", "nerinyan"],
        "catboy" => ["catboy", "sayobot", "hinai", "nerinyan"],
        "nerinyan" => ["nerinyan", "sayobot", "hinai", "catboy"],
        _ => {
            return Err(CommandError::new(
                "DOWNLOAD_ADAPTER_DISABLED",
                "No download mirror is selected.",
            ));
        }
    };

    let mut failures = Vec::new();
    for adapter in adapters {
        if cancel.load(Ordering::Relaxed) {
            return Err(CommandError::new("DOWNLOAD_CANCELLED", "下载已取消"));
        }
        match state
            .providers
            .osz_with_progress(
                beatmapset_id,
                adapter,
                include_video,
                cancel,
                &mut on_progress,
            )
            .await
        {
            Ok(download) => return Ok(download),
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CommandError::new("DOWNLOAD_CANCELLED", "下载已取消"));
                }
                failures.push(format!("{adapter}: {}", error.message));
            }
        }
    }
    Err(CommandError::new(
        "BEATMAP_DOWNLOAD_FAILED",
        failures.join("; "),
    ))
}

#[cfg(test)]
#[test]
fn sanitizes_windows_download_names() {
    let item = BeatmapDownloadItem {
        beatmapset_id: 42,
        artist: "A/B".into(),
        title: "Title: Test?".into(),
    };
    assert_eq!(download_file_name(&item, None), "42 A_B - Title_ Test_.osz");
    assert_eq!(
        download_file_name(&item, Some("../remote.osz")),
        "42 remote.osz"
    );
}
