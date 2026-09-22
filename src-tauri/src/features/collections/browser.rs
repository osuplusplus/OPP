use super::{
    CollectionEntry, CollectionFolder,
    notebook::{PersonalRecord, PoolSnapshot},
};
use crate::{
    error::{CommandError, CommandResult},
    features::{
        local_analysis::{LocalBeatmapSummary, LocalClient},
        local_scores::LocalScore,
    },
    infrastructure::logging::{finish_span, global},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectionBrowseQuery {
    pub folder_id: Option<String>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    pub player: Option<String>,
}
#[derive(Serialize)]
pub struct SearchHit {
    pub field: String,
    pub text: String,
}
#[derive(Serialize)]
pub struct CollectionBrowseRow {
    pub key: String,
    pub folder_id: String,
    pub folder_name: String,
    pub read_only: bool,
    pub entry: CollectionEntry,
    pub local: Option<LocalBeatmapSummary>,
    pub metadata: Option<serde_json::Value>,
    pub slot: String,
    pub source_slot: String,
    pub selected_by: String,
    pub pool_comment: String,
    pub is_custom: bool,
    pub is_original: bool,
    pub record: PersonalRecord,
    pub latest_score: Option<LocalScore>,
    pub representative_available: bool,
    pub matches: Vec<SearchHit>,
}
#[derive(Serialize)]
pub struct CollectionBrowsePage {
    pub items: Vec<CollectionBrowseRow>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub matching_folders: Vec<String>,
    pub pool: Option<PoolSnapshot>,
    pub overview: CollectionOverview,
}
#[derive(Default, Serialize)]
pub struct CollectionOverview {
    pub local_count: usize,
    pub marked_count: usize,
    pub noted_count: usize,
    pub stars: Option<[f64; 2]>,
    pub bpm: Option<[f64; 2]>,
    pub total_length_ms: u64,
    pub slots: Vec<(String, usize)>,
}

fn collection_overview(rows: &[CollectionBrowseRow]) -> CollectionOverview {
    let mut value = CollectionOverview::default();
    let mut seen = HashSet::new();
    let mut groups = HashMap::<String, usize>::new();
    let range = |bounds: &mut Option<[f64; 2]>, next: f64| {
        if !next.is_finite() {
            return;
        }
        *bounds = Some(match bounds {
            Some([min, max]) => [min.min(next), max.max(next)],
            None => [next, next],
        });
    };
    for row in rows {
        if !row.slot.is_empty() {
            *groups.entry(slot_key(&row.slot).1).or_default() += 1;
        }
        if !seen.insert((&row.folder_id, &row.entry.id)) {
            continue;
        }
        value.marked_count += usize::from(!row.record.tags.is_empty());
        value.noted_count += usize::from(!row.record.note.trim().is_empty());
        if let Some(map) = &row.local {
            value.local_count += 1;
            if let Some(stars) = map.stars {
                range(&mut value.stars, stars);
            }
            if map.bpm > 0.0 {
                range(&mut value.bpm, map.bpm);
            }
            value.total_length_ms = value
                .total_length_ms
                .saturating_add(map.length_ms.max(0.0) as u64);
        } else if let Some(map) = &row.metadata {
            if let Some(stars) = map["difficulty_rating"].as_f64() {
                range(&mut value.stars, stars);
            }
            if let Some(bpm) = map["bpm"].as_f64().filter(|v| *v > 0.0) {
                range(&mut value.bpm, bpm);
            }
            value.total_length_ms = value.total_length_ms.saturating_add(
                map["total_length"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_mul(1000),
            );
        }
    }
    value.slots = groups.into_iter().collect();
    value.slots.sort_by_key(|(label, _)| slot_key(label));
    value
}
#[derive(Debug, Clone, Serialize)]
pub struct CollectionArtwork {
    pub client: LocalClient,
    pub resource_id: String,
}

type Resource = (Option<String>, LocalBeatmapSummary, String, String);
struct Resources {
    items: Vec<Resource>,
    hashes: HashMap<String, usize>,
    bids: HashMap<i32, usize>,
}
impl Resources {
    fn new(state: &AppState) -> CommandResult<Self> {
        let items = state.local_analysis.collection_resources()?;
        let mut hashes = HashMap::new();
        let mut bids = HashMap::new();
        for (i, (hash, summary, _, _)) in items.iter().enumerate().rev() {
            if let Some(hash) = hash {
                hashes.insert(hash.to_lowercase(), i);
            }
            if let Some(id) = summary.beatmap_id.filter(|id| *id > 0) {
                bids.insert(id, i);
            }
        }
        Ok(Self {
            items,
            hashes,
            bids,
        })
    }
    fn find(&self, entry: &CollectionEntry) -> Option<&Resource> {
        let index = if let Some(hash) = entry.checksum.as_ref().filter(|h| !h.is_empty()) {
            self.hashes.get(&hash.to_lowercase())
        } else {
            entry.beatmap_id.and_then(|id| self.bids.get(&id))
        };
        index.map(|i| &self.items[*i])
    }
}

fn slot_key(value: &str) -> (u8, String, u32) {
    if value.is_empty() {
        return (9, String::new(), 0);
    }
    let split = value
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(value.len());
    let group = value[..split].trim().to_uppercase();
    let position = value[split..].parse().unwrap_or(0);
    let rank = match group.as_str() {
        "NM" => 0,
        "HD" => 1,
        "HR" => 2,
        "DT" => 3,
        "FM" => 4,
        "TB" => 6,
        _ => 5,
    };
    (rank, group, position)
}
fn hits(fields: &[(String, String)], tokens: &[String]) -> Option<Vec<SearchHit>> {
    if !tokens.iter().all(|token| {
        fields
            .iter()
            .any(|(_, text)| text.to_lowercase().contains(token))
    }) {
        return None;
    }
    Some(
        fields
            .iter()
            .filter(|(_, text)| tokens.iter().any(|t| text.to_lowercase().contains(t)))
            .map(|(field, text)| {
                let chars: Vec<_> = text.chars().collect();
                let first = tokens
                    .iter()
                    .filter_map(|t| text.to_lowercase().find(t))
                    .min()
                    .unwrap_or(0);
                let position = text.to_lowercase()[..first]
                    .chars()
                    .count()
                    .saturating_sub(30);
                let excerpt: String = chars.iter().skip(position).take(180).collect();
                SearchHit {
                    field: field.clone(),
                    text: format!(
                        "{}{}{}",
                        if position > 0 { "…" } else { "" },
                        excerpt,
                        if chars.len() > position + 180 {
                            "…"
                        } else {
                            ""
                        }
                    ),
                }
            })
            .collect(),
    )
}
fn hashes(entry: &CollectionEntry, local: Option<&Resource>) -> Vec<String> {
    let mut hashes: Vec<_> = entry.checksum.iter().cloned().collect();
    if let Some((md5, map, _, _)) = local {
        hashes.extend(md5.iter().cloned());
        hashes.push(map.resource.content_hash.clone());
    }
    hashes
}
fn score_time(score: &LocalScore) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    score
        .played_at
        .as_ref()
        .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
}
fn entry_folder(
    state: &AppState,
    folder_id: &str,
    entry_id: &str,
) -> CommandResult<(CollectionFolder, CollectionEntry)> {
    let folder = state.collections.folder(folder_id)?;
    let entry = folder
        .entries
        .iter()
        .find(|e| e.id == entry_id)
        .cloned()
        .ok_or_else(|| CommandError::new("COLLECTION_ENTRY_NOT_FOUND", "谱面已从收藏夹移除"))?;
    Ok((folder, entry))
}

fn browse(state: &AppState, query: CollectionBrowseQuery) -> CommandResult<CollectionBrowsePage> {
    browse_folders(
        &state.collections,
        &state.local_scores,
        Resources::new(state)?,
        query,
    )
}
fn browse_folders(
    collections: &super::CollectionService,
    local_scores: &crate::features::local_scores::LocalScoreService,
    resources: Resources,
    query: CollectionBrowseQuery,
) -> CommandResult<CollectionBrowsePage> {
    let folders = collections.value.lock()?.folders.clone();
    let tokens: Vec<_> = query
        .search
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let mut rows = Vec::new();
    let mut matching_folders = Vec::new();
    let mut pool = None;
    for folder in folders
        .iter()
        .filter(|f| !tokens.is_empty() || query.folder_id.as_ref().is_none_or(|id| id == &f.id))
    {
        let mut notebook = collections.notebooks.get(&folder.id)?;
        if folder.pool.is_some() {
            notebook.pool = folder.pool.clone();
        }
        if query.folder_id.as_ref() == Some(&folder.id) {
            pool = notebook.pool.clone();
            // Collections linked before notebooks existed can still explicitly resync.
            if pool.is_none() {
                let parts: Vec<_> = folder
                    .external_id
                    .as_deref()
                    .unwrap_or("")
                    .split(':')
                    .collect();
                if let ["tournament", provider, season, category] = parts.as_slice() {
                    let reference = crate::features::tournament_pools::TournamentPoolRef {
                        url: None,
                        provider: (*provider).into(),
                        season: (*season).into(),
                        category: (*category).into(),
                    };
                    if reference.validate().is_ok() {
                        pool = Some(PoolSnapshot {
                            title: String::new(),
                            info: Default::default(),
                            reference,
                            slots: Vec::new(),
                        });
                    }
                }
            }
        }
        if !tokens.is_empty()
            && tokens.iter().all(|t| {
                format!("{} {}", folder.name, folder.creator)
                    .to_lowercase()
                    .contains(t)
            })
        {
            matching_folders.push(folder.id.clone());
        }
        for entry in &folder.entries {
            let local = resources.find(entry);
            let record = notebook.record(entry);
            let all_scores: Vec<_> = local_scores
                .matching(&hashes(entry, local), None)?
                .into_iter()
                .filter(|s| entry.ruleset.as_ref().is_none_or(|mode| mode == &s.ruleset))
                .collect();
            let scores: Vec<_> = all_scores
                .iter()
                .filter(|s| {
                    query
                        .player
                        .as_ref()
                        .is_some_and(|p| p.eq_ignore_ascii_case(&s.player))
                })
                .cloned()
                .collect();
            let representative_available = record.representative.as_ref().is_none_or(|r| {
                record
                    .scores
                    .iter()
                    .chain(&all_scores)
                    .any(|s| s.id == r.id)
            });
            let latest_score = record
                .scores
                .iter()
                .chain(&scores)
                .max_by(|a, b| score_time(a).cmp(&score_time(b)))
                .cloned();
            let slots: Vec<_> = notebook
                .pool
                .as_ref()
                .map(|p| {
                    p.slots
                        .iter()
                        .filter(|s| Some(s.beatmap_id) == entry.beatmap_id)
                        .map(Some)
                        .collect()
                })
                .filter(|v: &Vec<_>| !v.is_empty())
                .unwrap_or_else(|| vec![None]);
            for (index, slot) in slots.into_iter().enumerate() {
                let source_slot = slot.map(|s| s.label.clone()).unwrap_or_default();
                let label = record
                    .slot_override
                    .clone()
                    .unwrap_or_else(|| source_slot.clone());
                let selected_by = slot.map(|s| s.selected_by.clone()).unwrap_or_default();
                let pool_comment = slot.map(|s| s.comment.clone()).unwrap_or_default();
                let fields = vec![
                    (
                        "收藏夹".into(),
                        format!("{} {}", folder.name, folder.creator),
                    ),
                    (
                        "谱面".into(),
                        format!(
                            "{} {} {} {} {} {} {}",
                            entry.title,
                            entry.artist,
                            entry.creator,
                            entry.difficulty_name,
                            entry.beatmap_id.map(|v| v.to_string()).unwrap_or_default(),
                            entry
                                .beatmapset_id
                                .map(|v| v.to_string())
                                .unwrap_or_default(),
                            entry.checksum.as_deref().unwrap_or("")
                        ),
                    ),
                    (
                        "本地信息".into(),
                        local
                            .map(|(_, m, tags, source)| {
                                format!("{} {} {tags} {source}", m.title_unicode, m.artist_unicode)
                            })
                            .unwrap_or_default(),
                    ),
                    ("图位".into(), label.clone()),
                    ("选图人".into(), selected_by.clone()),
                    ("比赛评论".into(), pool_comment.clone()),
                    (
                        "标签".into(),
                        record
                            .tags
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                    ("笔记".into(), record.note.clone()),
                    (
                        "成绩".into(),
                        record
                            .scores
                            .iter()
                            .chain(record.representative.iter())
                            .map(|s| format!("{} {} {} {}", s.score, s.player, s.mods, s.note))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                ];
                let Some(matches) = hits(&fields, &tokens) else {
                    continue;
                };
                rows.push(CollectionBrowseRow {
                    key: format!("{}:{}:{index}", folder.id, entry.id),
                    folder_id: folder.id.clone(),
                    folder_name: folder.name.clone(),
                    read_only: folder.read_only,
                    entry: entry.clone(),
                    metadata: slot.and_then(|s| s.metadata.clone()),
                    local: local.map(|(_, m, _, _)| m.clone()),
                    slot: label,
                    source_slot,
                    selected_by,
                    pool_comment,
                    is_custom: slot.is_some_and(|s| s.is_custom),
                    is_original: slot.is_some_and(|s| s.is_original),
                    record: record.clone(),
                    latest_score: latest_score.clone(),
                    representative_available,
                    matches,
                });
            }
        }
    }
    match query.sort.as_str() {
        "title" => rows.sort_by_key(|r| r.entry.title.to_lowercase()),
        "stars" => rows.sort_by(|a, b| {
            a.local
                .as_ref()
                .and_then(|l| l.stars)
                .or_else(|| {
                    a.metadata
                        .as_ref()
                        .and_then(|m| m["difficulty_rating"].as_f64())
                })
                .unwrap_or(f64::INFINITY)
                .total_cmp(
                    &b.local
                        .as_ref()
                        .and_then(|l| l.stars)
                        .or_else(|| {
                            b.metadata
                                .as_ref()
                                .and_then(|m| m["difficulty_rating"].as_f64())
                        })
                        .unwrap_or(f64::INFINITY),
                )
        }),
        "order" => {}
        _ => rows.sort_by_key(|r| slot_key(&r.slot)),
    }
    let total = rows.len();
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.min(total.saturating_sub(1) / limit * limit);
    let overview = collection_overview(&rows);
    Ok(CollectionBrowsePage {
        items: rows.into_iter().skip(offset).take(limit).collect(),
        total,
        offset,
        limit,
        matching_folders,
        pool,
        overview,
    })
}

#[tauri::command]
pub async fn query_collection_browser(
    app: AppHandle,
    query: CollectionBrowseQuery,
) -> CommandResult<CollectionBrowsePage> {
    crate::infrastructure::tasks::blocking_io("query_collection_browser", move || {
        let span = global().map(|l| l.operation("collections", "query_collection_browser"));
        finish_span(span, browse(&app.state::<AppState>(), query))
    })
    .await?
}
#[tauri::command]
pub async fn get_collection_record(
    app: AppHandle,
    folder_id: String,
    entry_id: String,
) -> CommandResult<PersonalRecord> {
    crate::infrastructure::tasks::blocking_io("get_collection_record", move || {
        let span = global().map(|log| log.operation("collections", "get_collection_record"));
        finish_span(
            span,
            (|| {
                let state = app.state::<AppState>();
                let (_, entry) = entry_folder(&state, &folder_id, &entry_id)?;
                Ok(state.collections.notebooks.get(&folder_id)?.record(&entry))
            })(),
        )
    })
    .await?
}

#[tauri::command]
pub async fn save_collection_record(
    app: AppHandle,
    folder_id: String,
    entry_id: String,
    record: PersonalRecord,
) -> CommandResult<PersonalRecord> {
    crate::infrastructure::tasks::blocking_io("save_collection_record", move || {
        let span = global().map(|log| log.operation("collections", "save_collection_record"));
        finish_span(
            span,
            (|| {
                let state = app.state::<AppState>();
                let (_, entry) = entry_folder(&state, &folder_id, &entry_id)?;
                state.collections.notebooks.save(&folder_id, &entry, record)
            })(),
        )
    })
    .await?
}
#[tauri::command]
pub async fn get_collection_entry_scores(
    app: AppHandle,
    folder_id: String,
    entry_id: String,
    player: Option<String>,
) -> CommandResult<Vec<LocalScore>> {
    crate::infrastructure::tasks::blocking_io("get_collection_entry_scores", move || {
        let span = global().map(|log| log.operation("collections", "get_collection_entry_scores"));
        finish_span(
            span,
            (|| {
                let state = app.state::<AppState>();
                let (_, entry) = entry_folder(&state, &folder_id, &entry_id)?;
                let resources = Resources::new(&state)?;
                Ok(state
                    .local_scores
                    .matching(&hashes(&entry, resources.find(&entry)), player.as_deref())?
                    .into_iter()
                    .filter(|s| entry.ruleset.as_ref().is_none_or(|mode| mode == &s.ruleset))
                    .collect())
            })(),
        )
    })
    .await?
}
#[tauri::command]
pub async fn get_collection_artwork(
    app: AppHandle,
    folder_id: Option<String>,
    offset: usize,
) -> CommandResult<Vec<CollectionArtwork>> {
    crate::infrastructure::tasks::blocking_io("get_collection_artwork", move || {
        let span = global().map(|log| log.operation("collections", "get_collection_artwork"));
        finish_span(
            span,
            (|| {
                let state = app.state::<AppState>();
                let resources = Resources::new(&state)?;
                let background_keys = state.local_analysis.collection_background_keys()?;
                let folders = state.collections.value.lock()?.folders.clone();
                let mut seen = HashSet::new();
                let mut items = Vec::new();
                for entry in folders
                    .iter()
                    .filter(|f| folder_id.as_ref().is_none_or(|id| id == &f.id))
                    .flat_map(|f| &f.entries)
                {
                    if let Some((_, map, _, _)) = resources.find(entry)
                        && let Some(key) = background_keys
                            .get(&(map.resource.client, map.resource.resource_id.clone()))
                        && seen.insert(key.clone())
                    {
                        items.push(CollectionArtwork {
                            client: map.resource.client,
                            resource_id: map.resource.resource_id.clone(),
                        });
                    }
                }
                if items.is_empty() {
                    return Ok(items);
                }
                Ok(items
                    .iter()
                    .cycle()
                    .skip(offset % items.len())
                    .take(items.len().min(24))
                    .cloned()
                    .collect())
            })(),
        )
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn natural_slots_unknowns_and_unassigned() {
        let mut slots = vec!["TB1", "NM10", "", "LZ2", "HD1", "NM2", "LZ1"];
        slots.sort_by_key(|s| slot_key(s));
        assert_eq!(slots, ["NM2", "NM10", "HD1", "LZ1", "LZ2", "TB1", ""]);
    }
    #[test]
    fn searches_fragments_across_fields_and_snippets() {
        let fields = vec![
            ("笔记".into(), format!("{}这里很容易失误", "前".repeat(200))),
            ("曲名".into(), "Freedom Dive".into()),
        ];
        let matched = hits(&fields, &["失误".into(), "dive".into()]).unwrap();
        assert_eq!(matched.len(), 2);
        assert!(matched[0].text.contains("失误"));
        assert!(hits(&fields, &["不存在".into()]).is_none());
    }
    #[test]
    fn global_search_reaches_unloaded_notes_and_preserves_duplicate_slots() {
        let dir = tempfile::tempdir().unwrap();
        let collections = super::super::CollectionService::new(dir.path()).unwrap();
        let first = collections.create("第一份", "Player").unwrap();
        let second = collections.create("比赛", "Player").unwrap();
        collections
            .update(|data| {
                data.folders[1].entries = (0..10_000)
                    .map(|i| CollectionEntry {
                        id: format!("e{i}"),
                        beatmap_id: Some(i + 1),
                        beatmapset_id: None,
                        checksum: None,
                        ruleset: Some("osu".into()),
                        difficulty_name: "Insane".into(),
                        title: format!("Song {i}"),
                        artist: String::new(),
                        creator: String::new(),
                        resolved: false,
                    })
                    .collect();
                Ok(())
            })
            .unwrap();
        let entry = collections.folder(&second.id).unwrap().entries[9999].clone();
        collections
            .notebooks
            .save(
                &second.id,
                &entry,
                PersonalRecord {
                    note: "这里容易失误".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let reference = crate::features::tournament_pools::TournamentPoolRef {
            url: None,
            provider: "rino".into(),
            season: "s2".into(),
            category: "finals".into(),
        };
        collections
            .notebooks
            .save_pool(
                &second.id,
                PoolSnapshot {
                    title: String::new(),
                    info: Default::default(),
                    reference,
                    slots: vec!["NM10", "NM2"]
                        .into_iter()
                        .map(|label| super::super::notebook::PoolSlot {
                            metadata: None,
                            is_custom: false,
                            is_original: false,
                            download_disabled: false,
                            beatmap_id: 10_000,
                            label: label.into(),
                            selected_by: "选图人".into(),
                            comment: "尾段注意".into(),
                        })
                        .collect(),
                },
            )
            .unwrap();
        let resources = || Resources {
            items: Vec::new(),
            hashes: HashMap::new(),
            bids: HashMap::new(),
        };
        let scores = crate::features::local_scores::LocalScoreService::default();
        let found = browse_folders(
            &collections,
            &scores,
            resources(),
            CollectionBrowseQuery {
                folder_id: Some(first.id),
                search: "失误 song".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(found.total, 2);
        assert_eq!(found.items[0].slot, "NM2");
        assert_eq!(found.items[1].slot, "NM10");
        assert_eq!(found.items[0].record, found.items[1].record);
        assert_eq!(found.overview.noted_count, 1);
        assert_eq!(found.overview.slots, vec![("NM".into(), 2)]);
        assert!(found.items.iter().all(|r| r.folder_id == second.id));
        let page = browse_folders(
            &collections,
            &scores,
            resources(),
            CollectionBrowseQuery {
                folder_id: Some(second.id),
                offset: 5000,
                limit: Some(50),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.items.len(), 50);
        assert_eq!(page.total, 10001);
        assert_eq!(page.overview.noted_count, 1);
        assert!(serde_json::to_vec(&page).unwrap().len() < 100_000);
    }
}
