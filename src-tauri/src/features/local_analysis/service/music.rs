//! 音乐/收藏联动的后端支持：曲库候选构建、谱面集定位、收藏背景键收集，
//! 以及"仅音乐"模式的空闲内存释放。
use super::*;
use crate::domain::Ruleset;
use crate::features::local_analysis::{MusicAsset, MusicCandidate};
use std::collections::HashMap;

// The index already describes the file layout. Playback and artwork loading
// validate the resolved path before reading it, so queue construction need not
// canonicalize two files for every difficulty in a large library.
// 只按索引拼路径、不预先校验；合法性延迟到真正读取时再验证。
fn indexed_music_path(
    client: LocalClient,
    entry: &IndexedEntry,
    root: &Path,
    filename: &str,
) -> Option<PathBuf> {
    if filename.trim().is_empty() {
        return None;
    }
    match client {
        LocalClient::Stable => {
            let normalized = filename.replace('\\', "/");
            let relative = Path::new(&normalized);
            // 拒绝绝对路径与 `..`、盘符、根目录等片段，文件名必须约束在库内。
            if relative.is_absolute()
                || relative.components().any(|part| {
                    matches!(
                        part,
                        std::path::Component::ParentDir
                            | std::path::Component::Prefix(_)
                            | std::path::Component::RootDir
                    )
                })
            {
                return None;
            }
            Some(root.join(normalized))
        }
        LocalClient::Lazer => {
            // 哈希须为至少 4 位的十六进制串，才能拼出合法的 files/ 内容寻址路径。
            let hash = &entry
                .lazer_files
                .as_ref()?
                .iter()
                .find(|file| file.filename.eq_ignore_ascii_case(filename))?
                .hash;
            if hash.len() < 4 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            Some(root.join(lazer_realm::blob_relative_path(hash)))
        }
    }
}

impl LocalAnalysisService {
    /// Deduplicate actual background references, not sets: difficulties can use different art.
    pub(crate) fn collection_background_keys(
        &self,
    ) -> CommandResult<HashMap<(LocalClient, String), String>> {
        let mut keys = HashMap::new();
        for client in [LocalClient::Stable, LocalClient::Lazer] {
            let Some(index) = self.current_index(client)? else {
                continue;
            };
            for entry in &index.entries {
                let IndexedData::Beatmap { summary, .. } = &entry.data else {
                    continue;
                };
                if let Some(key) = super::artwork::background_key(client, entry) {
                    keys.insert((client, summary.resource.resource_id.clone()), key);
                }
            }
        }
        Ok(keys)
    }
    /// Lightweight collection joins; no .osu reads or difficulty calculations.
    /// 输出来源仍与当前配置匹配的索引条目（MD5、摘要、标签、来源元组）。
    pub(crate) fn collection_resources(
        &self,
    ) -> CommandResult<Vec<crate::features::local_analysis::CollectionResource>> {
        let mut resources = Vec::new();
        for client in [LocalClient::Stable, LocalClient::Lazer] {
            let Some(index) = self.current_index(client)? else {
                continue;
            };
            if !source_matches(&self.sources.resolve(client)?, &index.source_root) {
                continue;
            }
            for entry in &index.entries {
                if let IndexedData::Beatmap { summary, detail } = &entry.data {
                    resources.push((
                        entry.beatmap_md5.clone(),
                        summary.clone(),
                        detail.tags.clone(),
                        detail.source.clone(),
                    ));
                }
            }
        }
        Ok(resources)
    }
    /// 返回指定资源所属的 `set_key` 与游戏模式；无索引或来源已变时为 `None`。
    pub(crate) fn music_location(
        &self,
        client: LocalClient,
        resource_id: &str,
    ) -> CommandResult<Option<(String, Ruleset)>> {
        let Some(index) = self.current_index(client)? else {
            return Ok(None);
        };
        if !source_matches(&self.sources.resolve(client)?, &index.source_root) {
            return Ok(None);
        }
        Ok(index.entries.iter().find_map(|entry| match &entry.data {
            IndexedData::Beatmap { summary, .. } if summary.resource.resource_id == resource_id => {
                Some((summary.set_key.clone(), summary.ruleset))
            }
            _ => None,
        }))
    }

    /// Copies only music metadata, never hit objects, strains or Realm file tables.
    /// 遍历两个客户端的索引构建候选：音频路径可解析时附带 `asset`，
    /// 结果按（客户端, `set_key`, 资源 ID）稳定排序。
    pub(crate) fn music_candidates(
        &self,
        query: Option<&BeatmapQuery>,
    ) -> CommandResult<Vec<MusicCandidate>> {
        let span = global().map(|log| log.operation("local_analysis", "music_candidates"));
        crate::infrastructure::logging::finish_span(
            span,
            (|| {
                let mut result = Vec::new();
                for client in [LocalClient::Stable, LocalClient::Lazer] {
                    let Some(index) = self.current_index(client)? else {
                        continue;
                    };
                    if !source_matches(&self.sources.resolve(client)?, &index.source_root) {
                        continue;
                    }
                    let lazer_root = if client == LocalClient::Lazer {
                        self.lazer_files_root(client).ok()
                    } else {
                        None
                    };
                    let search = query
                        .map(|q| q.search.trim().to_lowercase())
                        .unwrap_or_default();
                    for entry in &index.entries {
                        let IndexedData::Beatmap { summary, detail } = &entry.data else {
                            continue;
                        };
                        let root = match client {
                            LocalClient::Stable => {
                                entry.physical_path.parent().map(Path::to_path_buf)
                            }
                            LocalClient::Lazer => lazer_root.clone(),
                        };
                        let asset = root.and_then(|root| {
                            indexed_music_path(client, entry, &root, &detail.audio_file).map(
                                |audio| {
                                    let artwork = indexed_music_path(
                                        client,
                                        entry,
                                        &root,
                                        &detail.background_file,
                                    );
                                    MusicAsset {
                                        client,
                                        resource_id: summary.resource.resource_id.clone(),
                                        root,
                                        audio,
                                        artwork,
                                        beatmap: entry.physical_path.clone(),
                                    }
                                },
                            )
                        });
                        result.push(MusicCandidate {
                            client,
                            resource_id: summary.resource.resource_id.clone(),
                            set_key: summary.set_key.clone(),
                            set_id: summary.beatmap_set_id.filter(|id| *id > 0),
                            beatmap_id: summary.beatmap_id,
                            checksum: entry.beatmap_md5.clone(),
                            title: if summary.title_unicode.is_empty() {
                                &summary.title
                            } else {
                                &summary.title_unicode
                            }
                            .clone(),
                            artist: if summary.artist_unicode.is_empty() {
                                &summary.artist
                            } else {
                                &summary.artist_unicode
                            }
                            .clone(),
                            matches: query.is_none_or(|q| {
                                q.client == client && beatmap_matches(summary, detail, q, &search)
                            }),
                            asset,
                        });
                    }
                }
                result.sort_by(|a, b| {
                    (a.client, &a.set_key, &a.resource_id).cmp(&(
                        b.client,
                        &b.set_key,
                        &b.resource_id,
                    ))
                });
                Ok(result)
            })(),
        )
    }

    /// 开关"仅音乐"模式；开启后空闲时会释放索引内存（见下方释放函数）。
    pub(crate) fn set_music_only(&self, enabled: bool) {
        self.music_only.store(enabled, AtomicOrdering::Relaxed);
    }

    /// 仍在执行的后台扫描任务数（取不到锁时按 0 计）。
    pub(crate) fn music_background_tasks(&self) -> usize {
        self.scans.lock().map(|scans| scans.len()).unwrap_or(0)
    }

    /// 仅音乐模式下且无后台扫描时，清空内存索引与皮肤资源缓存以释放
    /// 内存；其余情况直接返回，避免释放正被使用的状态。
    pub(crate) fn release_music_idle_indexes(&self) {
        if !self.music_only.load(AtomicOrdering::Relaxed) || self.music_background_tasks() > 0 {
            return;
        }
        if let Ok(mut indexes) = self.indexes.write() {
            indexes.clear();
        }
        if let Ok(mut assets) = self.skin_assets.write() {
            assets.clear();
        }
    }
}

impl MusicAsset {
    /// Revalidate persisted descriptors each time. Symlinks cannot escape the library root.
    /// 双方 canonicalize 后要求路径仍位于 `root` 之下且为普通文件。
    pub(crate) fn checked_path(&self, path: &Path) -> CommandResult<PathBuf> {
        let root = self.root.canonicalize()?;
        let path = path.canonicalize()?;
        if !path.starts_with(root) || !path.is_file() {
            return Err(CommandError::new(
                "MUSIC_RESOURCE_MISSING",
                "歌曲文件已移走或路径无效",
            ));
        }
        Ok(path)
    }
    /// 读取谱面 `[General]` 段的 `PreviewTime` 并换算为秒；
    /// 文件缺失或数值非法（负数、NaN、非有限值）时返回 0。
    pub(crate) fn preview_seconds(&self) -> f64 {
        let Ok(path) = self.checked_path(&self.beatmap) else {
            return 0.0;
        };
        let Ok(bytes) = fs::read(path) else {
            return 0.0;
        };
        let text = super::super::parser::decode_text(&bytes);
        let mut general = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                general = line == "[General]";
            }
            if general && let Some(value) = line.strip_prefix("PreviewTime:") {
                return value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(0.0)
                    / 1000.0;
            }
        }
        0.0
    }
}
