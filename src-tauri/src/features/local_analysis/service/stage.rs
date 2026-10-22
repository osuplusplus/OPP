//! Focused set selection and on-demand local audio.
//!
//! 聚焦选谱（按集补全、随机抽取）与本地音频试听读取。
use super::super::models::LocalBeatmapAudioPayload;
use super::*;
use crate::domain::Ruleset;
use crate::infrastructure::logging::finish_span;
use rand_core::{OsRng, RngCore};
use std::io::Read;

impl LocalAnalysisService {
    /// 汇总指定谱面集在给定模式下的全部难度；
    /// 该模式下没有难度（或集不存在）时报"资源不存在"。
    pub fn complete_beatmap_set(
        &self,
        client: LocalClient,
        set_key: &str,
        ruleset: Ruleset,
    ) -> CommandResult<LocalBeatmapSetSummary> {
        let span = global().map(|log| log.operation("local_analysis", "get_local_beatmap_set"));
        finish_span(
            span,
            (|| {
                let index = self.require_current_index(client)?;
                let maps = index
                    .beatmap_sets
                    .get(set_key)
                    .into_iter()
                    .flatten()
                    .filter_map(|position| match &index.entries.get(*position)?.data {
                        IndexedData::Beatmap { summary, detail } if summary.ruleset == ruleset => {
                            Some((summary, detail.as_ref()))
                        }
                        _ => None,
                    })
                    .collect();
                summarize_set(set_key, maps).ok_or_else(|| {
                    CommandError::new(
                        "LOCAL_RESOURCE_NOT_FOUND",
                        "该谱面集在当前模式下已不存在，请重新选谱",
                    )
                })
            })(),
        )
    }

    /// 用蓄水池抽样在匹配查询的谱面集中等概率随机选一个；
    /// 若唯一命中的是被排除的集，则回退返回该集。
    pub fn random_beatmap_set(
        &self,
        query: BeatmapQuery,
        exclude_set_key: Option<&str>,
    ) -> CommandResult<Option<LocalBeatmapSetSummary>> {
        let span =
            global().map(|log| log.operation("local_analysis", "pick_random_local_beatmap_set"));
        finish_span(
            span,
            (|| {
                let index = self.require_current_index(query.client)?;
                let search = query.search.trim().to_lowercase();
                let mut chosen = None;
                let mut fallback = None;
                let mut count = 0_u64;
                // Reservoir sampling retains only a set key, never all result payloads.
                for (key, positions) in &index.beatmap_sets {
                    let matches = positions.iter().any(|position| {
                        matches!(&index.entries[*position].data,
                    IndexedData::Beatmap { .. } if index.matches(*position, &query, &search))
                    });
                    if !matches {
                        continue;
                    }
                    if Some(key.as_str()) == exclude_set_key {
                        fallback = Some(key);
                        continue;
                    }
                    count += 1;
                    if random_below(count) == 0 {
                        chosen = Some(key);
                    }
                }
                Ok(chosen.or(fallback).and_then(|key| {
                    let maps = index.beatmap_sets[key]
                        .iter()
                        .filter_map(|position| match &index.entries[*position].data {
                            IndexedData::Beatmap { summary, detail }
                                if index.matches(*position, &query, &search) =>
                            {
                                Some((summary, detail.as_ref()))
                            }
                            _ => None,
                        })
                        .collect();
                    summarize_set(key, maps)
                }))
            })(),
        )
    }

    /// 读取谱面音频，Base64 编码后连同 MIME 与预览时间一并返回；
    /// MIME 依魔数嗅探（含 `fLaC` 兜底），无法识别时直接报错。
    pub fn beatmap_audio(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<LocalBeatmapAudioPayload> {
        let span = global().map(|log| log.operation("local_analysis", "get_local_beatmap_audio"));
        let result = (|| {
            let index = self.require_current_index(client)?;
            let entry = index
                .resource(resource_id)
                .filter(|entry| matches!(entry.data, IndexedData::Beatmap { .. }))
                .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "未找到该谱面资源"))?;
            let IndexedData::Beatmap { detail, .. } = &entry.data else {
                unreachable!()
            };
            let path_result = self.stage_asset_path(client, entry, &detail.audio_file);
            if let Some(s) = &span {
                s.io("resolve_audio", &path_result);
            }
            let path = path_result?;
            let bytes_result = read_audio_bounded(&path);
            if let Some(s) = &span {
                s.fs_op("read", &path, &bytes_result);
            }
            let bytes = bytes_result?;
            let mime = audio_mime(&bytes)
                .or_else(|| bytes.starts_with(b"fLaC").then_some("audio/flac"))
                .ok_or_else(|| {
                    CommandError::new("LOCAL_AUDIO_FORMAT", "无法识别该谱面的音频格式")
                })?;
            let map_result = fs::read(&entry.physical_path);
            if let Some(s) = &span {
                s.fs_op("read", &entry.physical_path, &map_result);
            }
            let map_bytes = map_result
                .map_err(|e| CommandError::new("LOCAL_RESOURCE_READ_ERROR", e.to_string()))?;
            Ok(LocalBeatmapAudioPayload {
                mime_type: mime.to_owned(),
                bytes_base64: BASE64_STANDARD.encode(bytes),
                preview_time_ms: preview_time(&super::super::parser::decode_text(&map_bytes)),
            })
        })();
        finish_span(span, result)
    }

    /// 解析谱面资源（音频/背景）的实际路径并做边界校验。
    /// `Stable` 相对谱面目录拼接文件名；`Lazer` 先在 Realm 文件清单中按
    /// 文件名找到内容哈希，再换算为 `files/` 内容寻址路径；
    /// 最终路径 canonicalize 后必须仍在对应根目录内且为普通文件。
    pub(super) fn stage_asset_path(
        &self,
        client: LocalClient,
        entry: &IndexedEntry,
        filename: &str,
    ) -> CommandResult<PathBuf> {
        let missing = || CommandError::new("LOCAL_AUDIO_NOT_FOUND", "谱面音频缺失或路径无效");
        if filename.trim().is_empty() {
            return Err(missing());
        }
        let (root, path) = match client {
            LocalClient::Stable => {
                let root = entry
                    .physical_path
                    .parent()
                    .ok_or_else(missing)?
                    .canonicalize()
                    .map_err(|_| missing())?;
                let path = root.join(filename.replace('\\', "/"));
                (root, path)
            }
            LocalClient::Lazer => {
                let file = entry
                    .lazer_files
                    .as_ref()
                    .and_then(|files| {
                        files
                            .iter()
                            .find(|file| file.filename.eq_ignore_ascii_case(filename))
                    })
                    .ok_or_else(missing)?;
                if file.hash.len() < 4 || !file.hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(missing());
                }
                let root = self
                    .lazer_files_root(client)?
                    .canonicalize()
                    .map_err(|_| missing())?;
                let path = root.join(lazer_realm::blob_relative_path(&file.hash));
                (root, path)
            }
        };
        let path = path.canonicalize().map_err(|_| missing())?;
        if !path.starts_with(root) || !path.is_file() {
            return Err(missing());
        }
        Ok(path)
    }
}

/// 生成 `[0, bound)` 内的无偏随机数：拒绝采样丢弃会造成取模偏差的尾部取值。
fn random_below(bound: u64) -> u64 {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = OsRng.next_u64();
        if value >= threshold {
            return value % bound;
        }
    }
}

/// 读取音频文件并限制在 64 MiB 内：先查元数据大小，再以 `take` 截断读取，
/// 双重防护避免超大文件被整读进内存。
fn read_audio_bounded(path: &Path) -> CommandResult<Vec<u8>> {
    const LIMIT: u64 = 64 * 1024 * 1024;
    let read = || -> std::io::Result<Vec<u8>> {
        let file = fs::File::open(path)?;
        if file.metadata()?.len() > LIMIT {
            return Err(std::io::Error::other("音频超过 64 MiB，无法试听"));
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err(std::io::Error::other("音频超过 64 MiB，无法试听"));
        }
        Ok(bytes)
    };
    read().map_err(|e| CommandError::new("LOCAL_AUDIO_READ_ERROR", e.to_string()))
}

/// 从 `.osu` 文本中提取 `[General]` 段的 `PreviewTime`（毫秒）；
/// 段外、非法或负值一律按 0 处理。
fn preview_time(text: &str) -> f64 {
    let mut general = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            general = line.eq_ignore_ascii_case("[General]");
        }
        if general
            && let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("PreviewTime")
        {
            return value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(0.0);
        }
    }
    0.0
}

/// 汇总谱面集，携带完整难度列表。
pub(super) fn summarize_set(
    set_key: &str,
    maps: Vec<(&LocalBeatmapSummary, &LocalBeatmapDetail)>,
) -> Option<LocalBeatmapSetSummary> {
    summarize_set_impl(set_key, maps, true)
}

/// 汇总谱面集但不携带难度列表，用于只需头信息的场景。
pub(super) fn summarize_set_header(
    set_key: &str,
    maps: Vec<(&LocalBeatmapSummary, &LocalBeatmapDetail)>,
) -> Option<LocalBeatmapSetSummary> {
    summarize_set_impl(set_key, maps, false)
}

/// 集合汇总的统一实现：难度按星级（其次难度名）排序，取首个难度作为展示
/// 代表；星级取集内最小/最大值，BPM、时长与物件数取最大值，背景取首个
/// 声明了背景文件的难度。
fn summarize_set_impl(
    set_key: &str,
    mut maps: Vec<(&LocalBeatmapSummary, &LocalBeatmapDetail)>,
    include_difficulties: bool,
) -> Option<LocalBeatmapSetSummary> {
    maps.sort_by(|(left, _), (right, _)| {
        option_f64_order(left.stars, right.stars)
            .then_with(|| text_order(&left.difficulty_name, &right.difficulty_name))
    });
    let (representative, _) = *maps.first()?;
    let creators = maps
        .iter()
        .map(|(summary, _)| summary.creator.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let stars = maps
        .iter()
        .filter_map(|(summary, _)| summary.stars)
        .collect::<Vec<_>>();
    let min_stars = stars.iter().copied().min_by(f64::total_cmp);
    let max_stars = stars.iter().copied().max_by(f64::total_cmp);
    let bpm = maps
        .iter()
        .map(|(summary, _)| summary.bpm)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let length_ms = maps
        .iter()
        .map(|(summary, _)| summary.length_ms)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let object_count = maps
        .iter()
        .map(|(summary, _)| summary.object_count)
        .max()
        .unwrap_or_default();
    let modified_at = maps
        .iter()
        .filter_map(|(summary, _)| summary.modified_at.clone())
        .max();
    let background_resource_id = maps.iter().find_map(|(summary, detail)| {
        (!detail.background_file.trim().is_empty()).then(|| summary.resource.resource_id.clone())
    });
    Some(LocalBeatmapSetSummary {
        set_key: set_key.to_owned(),
        completeness: Completeness::Complete,
        grouping_inferred: representative.set_grouping_inferred,
        beatmap_set_id: representative.beatmap_set_id,
        title: representative.title.clone(),
        title_unicode: representative.title_unicode.clone(),
        artist: representative.artist.clone(),
        artist_unicode: representative.artist_unicode.clone(),
        creators,
        min_stars,
        max_stars,
        bpm,
        length_ms,
        object_count,
        modified_at,
        background_resource_id,
        difficulties: if include_difficulties {
            maps.into_iter()
                .map(|(summary, _)| summary.clone())
                .collect()
        } else {
            Vec::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_time_only_accepts_finite_nonnegative_general_values() {
        assert_eq!(
            preview_time("[General]\nPreviewTime: 45000\n[Metadata]\nPreviewTime: 2"),
            45000.0
        );
        for value in ["-1", "NaN", "inf", "invalid"] {
            assert_eq!(
                preview_time(&format!("[General]\nPreviewTime:{value}")),
                0.0
            );
        }
        assert_eq!(preview_time("[Metadata]\nPreviewTime:100"), 0.0);
    }

    #[test]
    fn audio_read_rejects_oversize_files_without_loading_them() {
        let file = tempfile::NamedTempFile::new().expect("file");
        file.as_file()
            .set_len(64 * 1024 * 1024 + 1)
            .expect("length");
        assert!(read_audio_bounded(file.path()).is_err());
    }
}
