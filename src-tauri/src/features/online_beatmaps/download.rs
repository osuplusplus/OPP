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
                    beatmap_id = value.trim().parse::<i64>().ok();
                } else if let Some(value) = line.strip_prefix("BeatmapSetID:") {
                    set_id = value.trim().parse::<u64>().ok();
                }
            }
        }
        if set_id != Some(item.beatmapset_id) || beatmap_id.is_none_or(|id| id < -1) {
            return Err(CommandError::new(
                "STALE_BEATMAP_ARCHIVE",
                "镜像返回的谱面文件与所选谱面集不符",
            ));
        }
        // Published archives can also contain an unsubmitted guest difficulty (0/-1).
        // It cannot satisfy a requested BID, but does not invalidate the submitted maps.
        if let Some(id) = beatmap_id.filter(|id| *id > 0) {
            found.insert(id as u64);
        }
    }
    let matches = if item.allow_extra_difficulties {
        expected.is_subset(&found)
    } else {
        found == expected
    };
    if found.is_empty() || (!expected.is_empty() && !matches) {
        return Err(CommandError::new(
            "STALE_BEATMAP_ARCHIVE",
            "镜像中的难度与官网当前难度不一致",
        ));
    }
    Ok(())
}

fn adapters(provider: &str) -> CommandResult<&'static [&'static str]> {
    // Official .osz downloads require the lazer OAuth scope, unavailable to OPP.
    // Offer the official website in the UI; never send ordinary OAuth tokens to mirrors.
    // Hinai's own cascade can stall, so only use it when explicitly selected.
    match provider {
        "sayobot" => Ok(&["sayobot", "catboy", "nerinyan"]),
        "hinai" => Ok(&["hinai", "sayobot", "catboy", "nerinyan"]),
        "catboy" => Ok(&["catboy", "sayobot", "nerinyan"]),
        "nerinyan" => Ok(&["nerinyan", "sayobot", "catboy"]),
        _ => Err(CommandError::new(
            "DOWNLOAD_ADAPTER_DISABLED",
            "No download mirror is selected.",
        )),
    }
}

/// Downloads and validates each source before accepting it, with Sayobot preferred by default.
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
    let adapters = adapters(provider)?;

    let mut failures = Vec::new();
    for adapter in adapters {
        if cancel.load(Ordering::Relaxed) {
            return Err(CommandError::new("DOWNLOAD_CANCELLED", "下载已取消"));
        }
        on_progress(0, None);
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
        allow_extra_difficulties: false,
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
        allow_extra_difficulties: false,
    };
    assert_eq!(
        validate_osz(archive(&[5399092, 5260901]), &item)
            .unwrap_err()
            .code,
        "STALE_BEATMAP_ARCHIVE"
    );
    validate_osz(archive(&[5589234, 5589235]), &item).unwrap();
    // Exact matching remains the default for normal online search downloads.
    assert!(validate_osz(archive(&[5589234, 5589235, 5589236]), &item).is_err());
    let pool_item = BeatmapDownloadItem {
        allow_extra_difficulties: true,
        ..item
    };
    validate_osz(archive(&[5589234, 5589235, 5589236]), &pool_item).unwrap();
    assert!(validate_osz(archive(&[5589234, 5589236]), &pool_item).is_err());
    assert!(validate_osz(archive(&[]), &pool_item).is_err());
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use std::io::Write;
    fn archive(entries: &[(i64, u64)]) -> Cursor<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (index, (bid, sid)) in entries.iter().enumerate() {
            writer
                .start_file(
                    format!("{index}.osu"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            write!(
                writer,
                "osu file format v14\r\n[Metadata]\r\nBeatmapID:{bid}\r\nBeatmapSetID:{sid}\r\n"
            )
            .unwrap();
        }
        writer.finish().unwrap()
    }

    #[tokio::test]
    #[ignore = "live Sayobot availability check; downloads the Aurora archive"]
    async fn live_sayobot_aurora_archive() {
        let registry = super::super::providers::ProviderRegistry::new().unwrap();
        let download = registry
            .osz_with_progress(
                2007718,
                "sayobot",
                true,
                &AtomicBool::new(false),
                &mut |_, _| {},
            )
            .await
            .unwrap();
        let item = BeatmapDownloadItem {
            beatmapset_id: 2007718,
            artist: "Synthion".into(),
            title: "Aurora".into(),
            expected_beatmap_ids: vec![4176248],
            allow_extra_difficulties: true,
        };
        validate_osz(Cursor::new(download.bytes), &item).unwrap();
    }

    #[test]
    fn aurora_guest_difficulty_zero_does_not_reject_the_selected_radiance_map() {
        // Metadata observed in both Sayobot and Nerinyan archives for SID 2007718.
        let entries: Vec<_> = [
            4737419, 4194938, 4186621, 4176247, 4177521, 4176248, 4938756, 5020543, 5020544,
            5020545, 0,
        ]
        .into_iter()
        .map(|id| (id, 2007718))
        .collect();
        let item = BeatmapDownloadItem {
            beatmapset_id: 2007718,
            artist: "Synthion".into(),
            title: "Aurora".into(),
            expected_beatmap_ids: vec![4176248],
            allow_extra_difficulties: true,
        };
        validate_osz(archive(&entries), &item).unwrap();
        let missing: Vec<_> = entries
            .iter()
            .copied()
            .filter(|(id, _)| *id != 4176248)
            .collect();
        assert!(validate_osz(archive(&missing), &item).is_err());
        assert!(validate_osz(archive(&[(4176248, 2007718), (0, 999)]), &item).is_err());
        assert!(validate_osz(archive(&[(0, 2007718), (-1, 2007718)]), &item).is_err());
        assert!(validate_osz(archive(&[(4176248, 2007718), (-2, 2007718)]), &item).is_err());
        // The same official IDs must still match exactly for a normal search download.
        let exact = BeatmapDownloadItem {
            expected_beatmap_ids: entries
                .iter()
                .filter_map(|(id, _)| (*id > 0).then_some(*id as u64))
                .collect(),
            allow_extra_difficulties: false,
            ..item
        };
        validate_osz(archive(&entries), &exact).unwrap();
        assert!(validate_osz(archive(&missing), &exact).is_err());
    }

    #[test]
    fn sayobot_is_preferred_and_hinai_is_only_explicit() {
        assert_eq!(
            adapters("sayobot").unwrap(),
            ["sayobot", "catboy", "nerinyan"]
        );
        assert_eq!(adapters("hinai").unwrap()[0], "hinai");
        for provider in ["catboy", "nerinyan"] {
            let sources = adapters(provider).unwrap();
            assert_eq!(sources[0], provider);
            assert!(!sources.contains(&"hinai"));
        }
        assert!(adapters("none").is_err());
    }
}
