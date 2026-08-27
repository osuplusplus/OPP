//! Query predicates, sorting and pagination for the local-analysis index.

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::error::{CommandError, CommandResult};

#[cfg(test)]
use super::super::models::Page;
use super::super::{
    models::{
        BeatmapQuery, BeatmapSort, LocalBeatmapDetail, LocalBeatmapSetSummary, LocalBeatmapSummary,
        LocalSkinAssetSummary, LocalSkinSummary, SkinAssetKind, SkinSort, SortDirection,
    },
    parser::sha256,
};
use super::service_data::{IndexedData, IndexedEntry, LocalIndex};

pub(super) const MAX_QUERY_LIMIT: usize = 500;

pub(super) fn insert_bounded<T>(
    items: &mut Vec<T>,
    item: T,
    capacity: usize,
    compare: impl Fn(&T, &T) -> Ordering,
) {
    if capacity == 0 {
        return;
    }
    let position = items
        .binary_search_by(|current| compare(current, &item))
        .unwrap_or_else(|position| position);
    if position < capacity {
        items.insert(position, item);
        if items.len() > capacity {
            items.pop();
        }
    }
}

pub(super) fn find_skin_entry<'a>(
    index: &'a LocalIndex,
    resource_id: &str,
) -> CommandResult<&'a IndexedEntry> {
    index
        .entries
        .iter()
        .find(|entry| {
            matches!(
                &entry.data,
                IndexedData::Skin { detail }
                    if detail.summary.resource.resource_id == resource_id
            )
        })
        .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该 Skin 资源"))
}

pub(super) fn skin_root(entry: &IndexedEntry) -> CommandResult<PathBuf> {
    entry
        .physical_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_READ_ERROR", "Skin 目录无效"))
}

/// 由 Realm 登记的皮肤文件清单生成可预览资源（图片 / 音效）。
/// resource_id 生成规则与 Stable 目录枚举完全一致，前端无需区分客户端。
pub(super) fn enumerate_lazer_skin_assets(
    files: &[super::lazer_realm::LazerRealmFile],
    skin_resource_id: &str,
) -> Vec<LocalSkinAssetSummary> {
    let mut assets = files
        .iter()
        .filter_map(|file| {
            let path = Path::new(&file.filename);
            let extension = path
                .extension()
                .and_then(|value| value.to_str())?
                .to_ascii_lowercase();
            let kind = match extension.as_str() {
                "bmp" | "gif" | "jpeg" | "jpg" | "png" | "webp" => SkinAssetKind::Image,
                "mp3" | "ogg" | "wav" => SkinAssetKind::Audio,
                _ => return None,
            };
            let logical_path = file.filename.replace('\\', "/");
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| logical_path.clone());
            let category = skin_asset_category(&logical_path, kind).to_string();
            let resource_id = format!(
                "skin-asset:{}",
                sha256(format!("{skin_resource_id}:{}", logical_path.to_lowercase()).as_bytes())
            );
            Some(LocalSkinAssetSummary {
                resource_id,
                kind,
                name,
                logical_path,
                extension,
                size: file.size,
                category,
            })
        })
        .collect::<Vec<_>>();
    assets.sort_by(|left, right| {
        text_order(&left.category, &right.category)
            .then_with(|| text_order(&left.logical_path, &right.logical_path))
    });
    assets
}

pub(super) fn enumerate_skin_assets(
    root: &Path,
    skin_resource_id: &str,
) -> Vec<LocalSkinAssetSummary> {
    let mut assets = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let extension = entry
                .path()
                .extension()
                .and_then(|value| value.to_str())?
                .to_ascii_lowercase();
            let kind = match extension.as_str() {
                "bmp" | "gif" | "jpeg" | "jpg" | "png" | "webp" => SkinAssetKind::Image,
                "mp3" | "ogg" | "wav" => SkinAssetKind::Audio,
                _ => return None,
            };
            let logical_path = entry
                .path()
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            let category = skin_asset_category(&logical_path, kind).to_string();
            let resource_id = format!(
                "skin-asset:{}",
                sha256(format!("{skin_resource_id}:{}", logical_path.to_lowercase()).as_bytes())
            );
            Some(LocalSkinAssetSummary {
                resource_id,
                kind,
                name,
                logical_path,
                extension,
                size: metadata.len(),
                category,
            })
        })
        .collect::<Vec<_>>();
    assets.sort_by(|left, right| {
        text_order(&left.category, &right.category)
            .then_with(|| text_order(&left.logical_path, &right.logical_path))
    });
    assets
}

fn skin_asset_category(path: &str, kind: SkinAssetKind) -> &'static str {
    let value = path.to_ascii_lowercase();
    if kind == SkinAssetKind::Audio {
        return if value.contains("hit") || value.contains("slider") || value.contains("spinner") {
            "击打音效"
        } else {
            "界面音效"
        };
    }
    if value.contains("mania") || value.contains("key") || value.contains("stage") {
        "Mania"
    } else if value.contains("cursor") {
        "光标"
    } else if value.contains("hit")
        || value.contains("approach")
        || value.contains("slider")
        || value.contains("spinner")
    {
        "游戏元素"
    } else if value.contains("rank") || value.contains("score") || value.contains("grade") {
        "成绩界面"
    } else {
        "界面元素"
    }
}

pub(super) fn audio_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("audio/wav")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.starts_with(b"ID3")
        || bytes
            .get(..2)
            .is_some_and(|prefix| prefix[0] == 0xff && prefix[1] & 0xe0 == 0xe0)
    {
        Some("audio/mpeg")
    } else {
        None
    }
}

pub(super) fn beatmap_matches(
    summary: &LocalBeatmapSummary,
    detail: &LocalBeatmapDetail,
    query: &BeatmapQuery,
    search: &str,
) -> bool {
    (search.is_empty()
        || [
            &summary.title,
            &summary.title_unicode,
            &summary.artist,
            &summary.artist_unicode,
            &summary.creator,
            &summary.difficulty_name,
            &detail.source,
            &detail.tags,
        ]
        .iter()
        .any(|value| value.to_lowercase().contains(search))
        || summary
            .beatmap_id
            .is_some_and(|id| id.to_string() == search)
        || summary
            .beatmap_set_id
            .is_some_and(|id| id.to_string() == search))
        && (query.rulesets.is_empty() || query.rulesets.contains(&summary.ruleset))
        && query
            .min_stars
            .is_none_or(|minimum| summary.stars.is_some_and(|stars| stars >= minimum))
        && query
            .max_stars
            .is_none_or(|maximum| summary.stars.is_some_and(|stars| stars <= maximum))
        && query.min_bpm.is_none_or(|minimum| summary.bpm >= minimum)
        && query.max_bpm.is_none_or(|maximum| summary.bpm <= maximum)
        && query
            .min_length_ms
            .is_none_or(|minimum| summary.length_ms >= minimum)
        && query
            .max_length_ms
            .is_none_or(|maximum| summary.length_ms <= maximum)
        && query
            .min_objects
            .is_none_or(|minimum| summary.object_count >= minimum)
        && query
            .max_objects
            .is_none_or(|maximum| summary.object_count <= maximum)
        && query.min_ar.is_none_or(|minimum| detail.ar >= minimum)
        && query.max_ar.is_none_or(|maximum| detail.ar <= maximum)
        && query.min_cs.is_none_or(|minimum| detail.cs >= minimum)
        && query.max_cs.is_none_or(|maximum| detail.cs <= maximum)
        && query.min_od.is_none_or(|minimum| detail.od >= minimum)
        && query.max_od.is_none_or(|maximum| detail.od <= maximum)
        && query
            .submitted
            .is_none_or(|submitted| summary.beatmap_set_id.is_some() == submitted)
}

pub(super) fn compare_beatmaps(
    left: &LocalBeatmapSummary,
    right: &LocalBeatmapSummary,
    sort: BeatmapSort,
) -> Ordering {
    let ordering = match sort {
        BeatmapSort::Title => text_order(&left.title, &right.title),
        BeatmapSort::Artist => text_order(&left.artist, &right.artist),
        BeatmapSort::Creator => text_order(&left.creator, &right.creator),
        BeatmapSort::Stars => option_f64_order(left.stars, right.stars),
        BeatmapSort::Bpm => left.bpm.total_cmp(&right.bpm),
        BeatmapSort::Length => left.length_ms.total_cmp(&right.length_ms),
        BeatmapSort::ObjectCount => left.object_count.cmp(&right.object_count),
        BeatmapSort::ModifiedAt => left.modified_at.cmp(&right.modified_at),
    };
    ordering.then_with(|| left.resource.resource_id.cmp(&right.resource.resource_id))
}

pub(super) fn compare_beatmap_sets(
    left: &LocalBeatmapSetSummary,
    right: &LocalBeatmapSetSummary,
    sort: BeatmapSort,
) -> Ordering {
    let ordering = match sort {
        BeatmapSort::Title => text_order(&left.title, &right.title),
        BeatmapSort::Artist => text_order(&left.artist, &right.artist),
        BeatmapSort::Creator => text_order(
            left.creators.first().map_or("", String::as_str),
            right.creators.first().map_or("", String::as_str),
        ),
        BeatmapSort::Stars => option_f64_order(left.max_stars, right.max_stars),
        BeatmapSort::Bpm => left.bpm.total_cmp(&right.bpm),
        BeatmapSort::Length => left.length_ms.total_cmp(&right.length_ms),
        BeatmapSort::ObjectCount => left.object_count.cmp(&right.object_count),
        BeatmapSort::ModifiedAt => left.modified_at.cmp(&right.modified_at),
    };
    ordering.then_with(|| left.set_key.cmp(&right.set_key))
}

pub(super) fn compare_skins(
    left: &LocalSkinSummary,
    right: &LocalSkinSummary,
    sort: SkinSort,
) -> Ordering {
    let ordering = match sort {
        SkinSort::Name => text_order(&left.name, &right.name),
        SkinSort::Author => text_order(&left.author, &right.author),
        SkinSort::Size => left.total_bytes.cmp(&right.total_bytes),
        SkinSort::ModifiedAt => left.modified_at.cmp(&right.modified_at),
    };
    ordering.then_with(|| left.resource.resource_id.cmp(&right.resource.resource_id))
}

pub(super) fn text_order(left: &str, right: &str) -> Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}

pub(super) fn option_f64_order(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub(super) fn apply_direction(ordering: Ordering, direction: SortDirection) -> Ordering {
    match direction {
        SortDirection::Asc => ordering,
        SortDirection::Desc => ordering.reverse(),
    }
}

#[cfg(test)]
pub(super) fn page<T>(items: Vec<T>, offset: usize, requested_limit: usize) -> Page<T> {
    let total = items.len();
    let limit = requested_limit.clamp(1, MAX_QUERY_LIMIT);
    let items = items.into_iter().skip(offset).take(limit).collect();
    Page {
        items,
        total,
        offset,
        limit,
    }
}
