//! 单个谱面集的下载流程：镜像级联尝试、压缩包一致性校验与目标文件命名。

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

/// 生成目标文件名：镜像建议的名字必须以 `.osz` 结尾才会采用，且会被补上
/// `BeatmapSetID` 前缀，保证 `find_existing_beatmapset` 能在目录中识别既有文件。
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

/// 校验镜像压缩包与所选谱面集一致，拦截镜像缓存过期或返回错包：
/// 包内带 ID 的 `.osu` 的 `BeatmapSetID` 必须匹配，收集到的 `BeatmapID`
/// 集合须覆盖 `expected_beatmap_ids`（谱面池模式下允许为超集）。
/// 2014 前的老谱面 `.osu` 元数据没有任何 ID 字段，此时退化为标题比对
/// （ASCII/Unicode 任一命中即可），仍然拦截镜像返回完全不同曲目的错包。
pub fn validate_osz<R: Read + Seek>(reader: R, item: &BeatmapDownloadItem) -> CommandResult<()> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|_| {
        CommandError::new(
            "INVALID_BEATMAP_ARCHIVE",
            "下载源返回的不是有效的 .osz 压缩包",
        )
    })?;
    let expected: HashSet<u64> = item.expected_beatmap_ids.iter().copied().collect();
    let mut found = HashSet::new();
    // 无 ID 老格式 `.osu` 的标题收集（ASCII 与 Unicode 各一），供兜底比对。
    let mut legacy_titles: Vec<[String; 2]> = Vec::new();
    let mut saw_submitted_set_id = false;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| CommandError::new("INVALID_BEATMAP_ARCHIVE", "无法读取 .osz 压缩包"))?;
        if !file.name().to_ascii_lowercase().ends_with(".osu") {
            continue;
        }
        // 单个 `.osu` 超过 16 MiB 视为异常载荷，直接拒绝。
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
        // 只需要 [Metadata] 段里的字段，不解析 `.osu` 其余内容。
        let text = String::from_utf8_lossy(&bytes);
        let mut in_metadata = false;
        let mut beatmap_id: Option<i64> = None;
        let mut set_id: Option<i64> = None;
        let mut title = None;
        let mut title_unicode = None;
        for line in text.lines() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.starts_with('[') && line.ends_with(']') {
                in_metadata = line == "[Metadata]";
            } else if in_metadata {
                if let Some(value) = line.strip_prefix("BeatmapID:") {
                    beatmap_id = value.trim().parse::<i64>().ok();
                } else if let Some(value) = line.strip_prefix("BeatmapSetID:") {
                    set_id = value.trim().parse::<i64>().ok();
                } else if let Some(value) = line.strip_prefix("Title:") {
                    title = Some(value.trim().to_string());
                } else if let Some(value) = line.strip_prefix("TitleUnicode:") {
                    title_unicode = Some(value.trim().to_string());
                }
            }
        }
        if beatmap_id.is_some_and(|id| id < -1) {
            return Err(CommandError::new(
                "STALE_BEATMAP_ARCHIVE",
                "镜像返回的谱面文件与所选谱面集不符",
            ));
        }
        // 带 ID 的难度：集合 ID 必须精确匹配；ID 缺失/未提交（0/-1）的难度
        // 既不能满足请求的难度，也不会否决其余已提交难度。
        if let Some(set_id) = set_id.filter(|value| *value > 0) {
            saw_submitted_set_id = true;
            if u64::try_from(set_id) != Ok(item.beatmapset_id) {
                return Err(CommandError::new(
                    "STALE_BEATMAP_ARCHIVE",
                    "镜像返回的谱面文件与所选谱面集不符",
                ));
            }
        }
        if let Some(id) = beatmap_id.filter(|id| *id > 0) {
            found.insert(id as u64);
        } else if set_id.filter(|value| *value > 0).is_none() {
            let titles = [title.unwrap_or_default(), title_unicode.unwrap_or_default()];
            if titles.iter().any(|value| !value.is_empty()) {
                legacy_titles.push(titles);
            }
        }
    }
    let matches = if item.allow_extra_difficulties {
        expected.is_subset(&found)
    } else {
        found == expected
    };
    if found.is_empty() || (!expected.is_empty() && !matches) {
        // 整包都是无 ID 的老格式（2014 前经典图常见）：无法比对难度 ID，
        // 退化为标题比对，拦截镜像返回完全不同曲目的错包。
        if !saw_submitted_set_id && !legacy_titles.is_empty() {
            let wanted = normalize_title(&item.title);
            if !wanted.is_empty()
                && legacy_titles
                    .iter()
                    .any(|titles| titles.iter().any(|value| normalize_title(value) == wanted))
            {
                return Ok(());
            }
        }
        return Err(CommandError::new(
            "STALE_BEATMAP_ARCHIVE",
            "镜像中的难度与官网当前难度不一致",
        ));
    }
    Ok(())
}

/// 标题归一化：只保留字母数字并小写，供老谱面的兜底比对。
fn normalize_title(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 给出首选镜像对应的级联顺序；不在列表中的镜像不会被自动尝试。
fn adapters(provider: &str) -> CommandResult<&'static [&'static str]> {
    // Official .osz downloads require the lazer OAuth scope, unavailable to OPP.
    // Offer the official website in the UI; never send ordinary OAuth tokens to mirrors.
    // Hinai's own cascade can stall, so only use it when explicitly selected.
    // Nerinyan's API has been unstable (observed offline since 2026-11), so it is
    // demoted to the last automatic fallback; osu.direct and Nekoha fill the gap.
    match provider {
        "sayobot" => Ok(&["sayobot", "osudirect", "nekoha", "catboy", "nerinyan"]),
        "osudirect" => Ok(&["osudirect", "sayobot", "nekoha", "catboy", "nerinyan"]),
        "nekoha" => Ok(&["nekoha", "sayobot", "osudirect", "catboy", "nerinyan"]),
        "hinai" => Ok(&[
            "hinai",
            "sayobot",
            "osudirect",
            "nekoha",
            "catboy",
            "nerinyan",
        ]),
        "catboy" => Ok(&["catboy", "sayobot", "osudirect", "nekoha", "nerinyan"]),
        "nerinyan" => Ok(&["nerinyan", "sayobot", "osudirect", "nekoha", "catboy"]),
        _ => Err(CommandError::new(
            "DOWNLOAD_ADAPTER_DISABLED",
            "No download mirror is selected.",
        )),
    }
}

/// 按级联顺序逐个镜像尝试“下载 + 校验”，任一镜像成功且校验通过即返回；
/// 全部失败时把各镜像的失败原因汇总为一条 `BEATMAP_DOWNLOAD_FAILED` 错误。
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
            // 下载成功仍需校验，防止镜像返回过期或错包。
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

// 文件名净化：非法字符替换与路径穿越（`../`）防护。
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

// 过期压缩包拒绝、精确/超集两种难度匹配模式的行为测试。
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

// 线上观测数据的回归测试：guest 难度（ID 0/-1）处理与镜像级联顺序。
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

    /// 2014 前的老谱面整包没有 ID 字段（如 SID 48979 Mythologia's End）：
    /// 按归一化标题兜底放行，标题不同（镜像串包）仍然拒绝；
    /// 混入错误集合 ID 的难度文件依旧整体拒绝。
    #[test]
    fn legacy_archive_without_ids_validates_by_normalized_title() {
        fn legacy(title: &str, extra: &str) -> Cursor<Vec<u8>> {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file("old.osu", zip::write::SimpleFileOptions::default())
                .unwrap();
            write!(
                writer,
                "osu file format v5\n[Metadata]\nTitle:{title}\nArtist:Hatsune Miku\nVersion:Myth0108ia\n{extra}"
            )
            .unwrap();
            writer.finish().unwrap()
        }
        let item = BeatmapDownloadItem {
            beatmapset_id: 48979,
            artist: "Hatsune Miku".into(),
            title: "Mythologia's End".into(),
            expected_beatmap_ids: vec![151229],
            allow_extra_difficulties: true,
        };
        validate_osz(legacy("Mythologia's End", ""), &item).unwrap();
        // 大小写/标点差异经归一化后仍命中。
        validate_osz(legacy("mythologia's  END!", ""), &item).unwrap();
        // 镜像返回完全不同的曲目：标题不命中即拒绝。
        assert!(validate_osz(legacy("Solar Sect of Mystic Wisdom", ""), &item).is_err());
        // 混入带错误集合 ID 的难度文件仍然整体拒绝。
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("a.osu", zip::write::SimpleFileOptions::default())
            .unwrap();
        write!(
            writer,
            "osu file format v5\n[Metadata]\nTitle:Mythologia's End\n"
        )
        .unwrap();
        writer
            .start_file("b.osu", zip::write::SimpleFileOptions::default())
            .unwrap();
        write!(
            writer,
            "osu file format v14\n[Metadata]\nBeatmapID:1\nBeatmapSetID:999\n"
        )
        .unwrap();
        assert!(validate_osz(writer.finish().unwrap(), &item).is_err());
    }

    #[test]
    fn sayobot_is_preferred_and_hinai_is_only_explicit() {
        assert_eq!(
            adapters("sayobot").unwrap(),
            ["sayobot", "osudirect", "nekoha", "catboy", "nerinyan"]
        );
        assert_eq!(adapters("hinai").unwrap()[0], "hinai");
        for provider in ["osudirect", "nekoha", "catboy", "nerinyan"] {
            let sources = adapters(provider).unwrap();
            assert_eq!(sources[0], provider);
            assert!(!sources.contains(&"hinai"));
        }
        // 每个级联都包含 Sayobot 与 osu.direct 双保险，Nerinyan 不再出现在前三位。
        for provider in ["sayobot", "osudirect", "nekoha", "catboy", "nerinyan"] {
            let sources = adapters(provider).unwrap();
            assert!(sources.contains(&"sayobot") && sources.contains(&"osudirect"));
            assert_ne!(sources[1], "nerinyan");
        }
        assert!(adapters("none").is_err());
    }
}
