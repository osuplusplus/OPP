//! 队列目录构建：把本地扫描出的候选音频按谱面集合合并为曲目，
//! 处理跨客户端合并、收藏夹解析与播放源排序。
use super::models::{QueueRequest, Track, TrackInfo};
use crate::features::{collections::CollectionEntry, local_analysis::MusicCandidate};
use std::collections::{BTreeMap, BTreeSet};

/// 由候选音频构建播放队列，返回 `(曲目列表, 收藏夹未命中条目数)`。
/// 产出的曲目按标题、艺人、ID 排序，保证相同输入得到稳定输出。
/// Connected components join unsubmitted sets only when an identical .osu checksum proves identity.
/// Distinct positive online set IDs are never joined by a shared song or title.
pub(crate) fn build_queue(
    candidates: Vec<MusicCandidate>,
    request: &QueueRequest,
    collection: Option<&[CollectionEntry]>,
) -> (Vec<Track>, usize) {
    // 分组：有线上谱集 ID 的用 "online:<id>"（跨客户端同名同集合），否则用 "客户端:集合键"。
    let mut groups: BTreeMap<String, Vec<MusicCandidate>> = BTreeMap::new();
    for candidate in candidates {
        let key = candidate
            .set_id
            .map(|id| format!("online:{id}"))
            .unwrap_or_else(|| format!("{}:{}", candidate.client, candidate.set_key));
        groups.entry(key).or_default().push(candidate);
    }
    // 并查集：仅对没有线上 ID 的本地集合，以完全一致的 .osu 校验和作为同一谱面的证据。
    let mut checksum_owner: BTreeMap<String, String> = BTreeMap::new();
    let mut aliases: BTreeMap<String, String> =
        groups.keys().map(|k| (k.clone(), k.clone())).collect();
    // 沿别名链找到代表键（并查集的根）。
    fn root(aliases: &BTreeMap<String, String>, key: &str) -> String {
        let mut current = key;
        while aliases[current] != current {
            current = &aliases[current];
        }
        current.to_owned()
    }
    for (key, maps) in &groups {
        // 线上谱集已按 ID 天然合并，不再参与校验和合并。
        if key.starts_with("online:") {
            continue;
        }
        for checksum in maps
            .iter()
            .filter_map(|m| m.checksum.as_ref())
            .filter(|s| !s.is_empty())
        {
            if let Some(owner) = checksum_owner.get(checksum) {
                let left = root(&aliases, key);
                let right = root(&aliases, owner);
                if left != right {
                    // 共享同一校验和的集合合并：较大的根改为指向较小的根。
                    aliases.insert(left.max(right.clone()), root(&aliases, key).min(right));
                }
            } else {
                checksum_owner.insert(checksum.clone(), key.clone());
            }
        }
    }
    // 按根键归并各分组，合并后的根键即曲目 ID。
    let mut merged: BTreeMap<String, Vec<MusicCandidate>> = BTreeMap::new();
    for (key, maps) in groups {
        merged.entry(root(&aliases, &key)).or_default().extend(maps);
    }
    // 收藏夹条目匹配规则：校验和优先（忽略大小写），其次 beatmap ID，
    // 仅有谱集 ID 的条目退回按谱集匹配。
    let entry_matches = |entry: &CollectionEntry, candidate: &MusicCandidate| {
        entry
            .checksum
            .as_ref()
            .filter(|s| !s.is_empty())
            .is_some_and(|hash| {
                candidate
                    .checksum
                    .as_ref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(hash))
            })
            || entry
                .beatmap_id
                .filter(|id| *id > 0)
                .is_some_and(|id| candidate.beatmap_id == Some(id))
            || (entry.beatmap_id.is_none_or(|id| id <= 0)
                && entry.checksum.as_ref().is_none_or(|s| s.is_empty())
                && entry
                    .beatmapset_id
                    .filter(|id| *id > 0)
                    .is_some_and(|id| candidate.set_id == Some(id)))
    };
    let mut resolved = BTreeSet::new();
    let mut tracks = Vec::new();
    for (id, mut maps) in merged {
        // 任一候选满足请求过滤（客户端、查询命中、指定资源、收藏夹）即保留整个曲目。
        let selected = maps.iter().any(|m| {
            request.client.is_none_or(|c| c == m.client)
                && m.matches
                && request
                    .resource_id
                    .as_ref()
                    .is_none_or(|r| r == &m.resource_id)
                && collection
                    .is_none_or(|entries| entries.iter().any(|entry| entry_matches(entry, m)))
        });
        if !selected {
            continue;
        }
        // 播放源排序：请求指定的资源最优先，其次请求客户端，再按客户端与资源稳定排序。
        maps.sort_by_key(|m| {
            (
                request
                    .resource_id
                    .as_ref()
                    .is_none_or(|id| id != &m.resource_id),
                request.client.is_some_and(|c| c != m.client),
                m.client,
                m.resource_id.clone(),
            )
        });
        let first = &maps[0];
        let info = TrackInfo {
            id,
            title: first.title.clone(),
            artist: first.artist.clone(),
            clients: maps
                .iter()
                .filter(|m| m.asset.is_some())
                .map(|m| m.client)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        };
        // 同一客户端指向同一音频文件的多个难度只保留一个播放源。
        let mut seen_assets = BTreeSet::new();
        let assets: Vec<_> = maps
            .iter()
            .filter_map(|m| m.asset.clone())
            .filter(|a| seen_assets.insert((a.client, a.audio.clone())))
            .collect();
        if assets.is_empty() {
            continue;
        }
        // 标记收藏夹中已被本曲目覆盖的条目，用于统计本地缺失数。
        if let Some(entries) = collection {
            for (i, entry) in entries.iter().enumerate() {
                if maps.iter().any(|m| entry_matches(entry, m)) {
                    resolved.insert(i);
                }
            }
        }
        tracks.push(Track { info, assets });
    }
    tracks.sort_by(|a, b| {
        (&a.info.title, &a.info.artist, &a.info.id).cmp(&(
            &b.info.title,
            &b.info.artist,
            &b.info.id,
        ))
    });
    let missing = collection.map_or(0, |entries| entries.len() - resolved.len());
    (tracks, missing)
}
