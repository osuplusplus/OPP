use super::models::{QueueRequest, Track, TrackInfo};
use crate::features::{collections::CollectionEntry, local_analysis::MusicCandidate};
use std::collections::{BTreeMap, BTreeSet};

/// Connected components join unsubmitted sets only when an identical .osu checksum proves identity.
/// Distinct positive online set IDs are never joined by a shared song or title.
pub(crate) fn build_queue(
    candidates: Vec<MusicCandidate>,
    request: &QueueRequest,
    collection: Option<&[CollectionEntry]>,
) -> (Vec<Track>, usize) {
    let mut groups: BTreeMap<String, Vec<MusicCandidate>> = BTreeMap::new();
    for candidate in candidates {
        let key = candidate
            .set_id
            .map(|id| format!("online:{id}"))
            .unwrap_or_else(|| format!("{}:{}", candidate.client, candidate.set_key));
        groups.entry(key).or_default().push(candidate);
    }
    let mut checksum_owner: BTreeMap<String, String> = BTreeMap::new();
    let mut aliases: BTreeMap<String, String> =
        groups.keys().map(|k| (k.clone(), k.clone())).collect();
    fn root(aliases: &BTreeMap<String, String>, key: &str) -> String {
        let mut current = key;
        while aliases[current] != current {
            current = &aliases[current];
        }
        current.to_owned()
    }
    for (key, maps) in &groups {
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
                    aliases.insert(left.max(right.clone()), root(&aliases, key).min(right));
                }
            } else {
                checksum_owner.insert(checksum.clone(), key.clone());
            }
        }
    }
    let mut merged: BTreeMap<String, Vec<MusicCandidate>> = BTreeMap::new();
    for (key, maps) in groups {
        merged.entry(root(&aliases, &key)).or_default().extend(maps);
    }
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
        let mut seen_assets = BTreeSet::new();
        let assets: Vec<_> = maps
            .iter()
            .filter_map(|m| m.asset.clone())
            .filter(|a| seen_assets.insert((a.client, a.audio.clone())))
            .collect();
        if assets.is_empty() {
            continue;
        }
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
