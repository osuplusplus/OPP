use std::{fs, io::Read, path::Path};

use md5::{Digest, Md5};
use tauri::State;
use zip::ZipArchive;

use super::{CollectionCandidate, CollectionFolder};
use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

const MAX_ARCHIVE_ENTRIES: usize = 5_000;
const MAX_OSU_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_OSU_BYTES: u64 = 64 * 1024 * 1024;

#[tauri::command]
pub fn import_collection_archive(
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<CollectionFolder> {
    let path = Path::new(&path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !matches!(extension.to_ascii_lowercase().as_str(), "zip" | "osz") {
        return Err(CommandError::new(
            "INVALID_BEATMAP_ARCHIVE",
            "请选择 .zip 或 .osz 压缩包",
        ));
    }
    let file = fs::File::open(path).map_err(|error| {
        CommandError::new(
            "BEATMAP_ARCHIVE_OPEN_FAILED",
            format!("无法打开压缩包：{error}"),
        )
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        CommandError::new(
            "INVALID_BEATMAP_ARCHIVE",
            format!("压缩包格式无效：{error}"),
        )
    })?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(CommandError::new(
            "BEATMAP_ARCHIVE_TOO_LARGE",
            "压缩包条目过多",
        ));
    }

    let mut candidates = Vec::new();
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            CommandError::new(
                "INVALID_BEATMAP_ARCHIVE",
                format!("读取压缩包失败：{error}"),
            )
        })?;
        if !entry.name().to_ascii_lowercase().ends_with(".osu") || entry.is_dir() {
            continue;
        }
        if entry.size() > MAX_OSU_BYTES
            || total_bytes.saturating_add(entry.size()) > MAX_TOTAL_OSU_BYTES
        {
            return Err(CommandError::new(
                "BEATMAP_ARCHIVE_TOO_LARGE",
                "压缩包内谱面文件过大",
            ));
        }
        total_bytes += entry.size();
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(|error| {
            CommandError::new(
                "BEATMAP_ARCHIVE_READ_FAILED",
                format!("读取谱面文件失败：{error}"),
            )
        })?;
        if let Some(candidate) = parse_osu_candidate(&bytes) {
            candidates.push(candidate);
        }
    }
    if candidates.is_empty() {
        return Err(CommandError::new(
            "EMPTY_BEATMAP_ARCHIVE",
            "压缩包中没有可识别的 .osu 谱面",
        ));
    }

    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("导入曲包");
    let folder = state.collections.create(name, "本地压缩包")?;
    if let Err(error) = state.collections.add_entries(&folder.id, candidates) {
        let _ = state.collections.delete(&folder.id);
        return Err(error);
    }
    state.collections.folder(&folder.id)
}

fn parse_osu_candidate(bytes: &[u8]) -> Option<CollectionCandidate> {
    let text = decode_osu(bytes);
    let mut section = "";
    let mut metadata = std::collections::HashMap::new();
    let mut general = std::collections::HashMap::new();
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match section {
            "Metadata" => {
                metadata.insert(key.trim().to_string(), value.trim().to_string());
            }
            "General" => {
                general.insert(key.trim().to_string(), value.trim().to_string());
            }
            _ => {}
        }
    }
    let difficulty_name = metadata.get("Version")?.trim().to_string();
    let checksum = format!("{:x}", Md5::digest(bytes));
    Some(CollectionCandidate {
        beatmap_id: parse_positive(metadata.get("BeatmapID")),
        beatmapset_id: parse_positive(metadata.get("BeatmapSetID")),
        checksum: Some(checksum),
        ruleset: Some(
            match general.get("Mode").map(String::as_str).unwrap_or("0") {
                "1" => "taiko",
                "2" => "fruits",
                "3" => "mania",
                _ => "osu",
            }
            .to_string(),
        ),
        difficulty_name,
        title: metadata
            .get("TitleUnicode")
            .filter(|value| !value.is_empty())
            .or_else(|| metadata.get("Title"))
            .cloned()
            .unwrap_or_else(|| "未命名谱面".into()),
        artist: metadata
            .get("ArtistUnicode")
            .filter(|value| !value.is_empty())
            .or_else(|| metadata.get("Artist"))
            .cloned()
            .unwrap_or_default(),
        creator: metadata.get("Creator").cloned().unwrap_or_default(),
        local_client: None,
        local_resource_id: None,
    })
}

fn parse_positive(value: Option<&String>) -> Option<i32> {
    value?.parse().ok().filter(|number| *number > 0)
}

fn decode_osu(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xfe]) {
        return encoding_rs::UTF_16LE.decode(&bytes[2..]).0.into_owned();
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return encoding_rs::UTF_16BE.decode(&bytes[2..]).0.into_owned();
    }
    if let Ok(value) = std::str::from_utf8(bytes) {
        return value.to_string();
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    detector.guess(None, true).decode(bytes).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::parse_osu_candidate;

    #[test]
    fn parses_archive_beatmap_metadata() {
        let candidate = parse_osu_candidate(b"osu file format v14\n[General]\nMode:3\n[Metadata]\nTitle:Song\nArtist:Artist\nCreator:Mapper\nVersion:Insane\nBeatmapID:12\nBeatmapSetID:34\n").unwrap();
        assert_eq!(candidate.beatmap_id, Some(12));
        assert_eq!(candidate.beatmapset_id, Some(34));
        assert_eq!(candidate.ruleset.as_deref(), Some("mania"));
        assert_eq!(candidate.difficulty_name, "Insane");
    }
}
