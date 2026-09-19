use std::{
    collections::HashSet,
    io::{Cursor, Read, Seek},
    path::Path,
};

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

pub fn validate_osz<R: Read + Seek>(reader: R, item: &BeatmapDownloadItem) -> CommandResult<()> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|_| {
        CommandError::new(
            "INVALID_BEATMAP_ARCHIVE",
            "下载源返回的不是有效的 .osz 压缩包",
        )
    })?;
    let expected: HashSet<u64> = item.expected_beatmap_ids.iter().copied().collect();
    let mut found = HashSet::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| CommandError::new("INVALID_BEATMAP_ARCHIVE", "无法读取 .osz 压缩包"))?;
        if !file.name().to_ascii_lowercase().ends_with(".osu") {
            continue;
        }
        if file.size() > 16 * 1024 * 1024 {
            return Err(CommandError::new(
                "INVALID_BEATMAP_ARCHIVE",
                ".osu 文件过大",
            ));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|_| {
            CommandError::new("INVALID_BEATMAP_ARCHIVE", "无法读取压缩包中的 .osu 文件")
        })?;
        let text = String::from_utf8_lossy(&bytes);
        let mut in_metadata = false;
        let mut beatmap_id = None;
        let mut set_id = None;
        for line in text.lines() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.starts_with('[') && line.ends_with(']') {
                in_metadata = line == "[Metadata]";
            } else if in_metadata {
                if let Some(value) = line.strip_prefix("BeatmapID:") {
                    beatmap_id = value.trim().parse::<u64>().ok();
                } else if let Some(value) = line.strip_prefix("BeatmapSetID:") {
                    set_id = value.trim().parse::<u64>().ok();
                }
            }
        }
        if set_id != Some(item.beatmapset_id) || beatmap_id.is_none_or(|id| id == 0) {
            return Err(CommandError::new(
                "STALE_BEATMAP_ARCHIVE",
                "镜像返回的谱面文件与所选谱面集不符",
            ));
        }
        found.insert(beatmap_id.expect("checked above"));
    }
    if found.is_empty() || (!expected.is_empty() && found != expected) {
        return Err(CommandError::new(
            "STALE_BEATMAP_ARCHIVE",
            "镜像中的难度与官网当前难度不一致",
        ));
    }
    Ok(())
}

/// Downloads a beatmapset through the selected mirror, then tries the other registered mirrors.
/// Sayobot is OPP's preferred mirror. Hinai itself implements a multi-source cascade; the remaining
/// attempts are an additional OPP fallback if a public endpoint is unavailable.
pub async fn download_with_adapters<F>(
    state: &AppState,
    item: &BeatmapDownloadItem,
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
                item.beatmapset_id,
                adapter,
                include_video,
                cancel,
                &mut on_progress,
            )
            .await
        {
            Ok(download) => match validate_osz(Cursor::new(&download.bytes), item) {
                Ok(()) => return Ok(download),
                Err(error) => failures.push(format!("{adapter}: {}", error.message)),
            },
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
        expected_beatmap_ids: vec![],
    };
    assert_eq!(download_file_name(&item, None), "42 A_B - Title_ Test_.osz");
    assert_eq!(
        download_file_name(&item, Some("../remote.osz")),
        "42 remote.osz"
    );
}

#[cfg(test)]
#[test]
fn rejects_a_stale_mirror_archive_and_accepts_current_difficulties() {
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn archive(ids: &[u64]) -> Cursor<Vec<u8>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for id in ids {
            writer
                .start_file(format!("{id}.osu"), SimpleFileOptions::default())
                .unwrap();
            write!(
                writer,
                "osu file format v14\n[Metadata]\nBeatmapID:{id}\nBeatmapSetID:2419109\n"
            )
            .unwrap();
        }
        writer.finish().unwrap()
    }

    let item = BeatmapDownloadItem {
        beatmapset_id: 2419109,
        artist: "yax03".into(),
        title: "down".into(),
        expected_beatmap_ids: vec![5589234, 5589235],
    };
    assert_eq!(
        validate_osz(archive(&[5399092, 5260901]), &item)
            .unwrap_err()
            .code,
        "STALE_BEATMAP_ARCHIVE"
    );
    validate_osz(archive(&[5589234, 5589235]), &item).unwrap();
}
