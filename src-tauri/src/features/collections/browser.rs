//! 收藏夹浏览器查询：跨文件夹检索、图位展开、分页与概览统计。
//! 搜索为多关键字 AND 匹配并附带命中摘要；结果默认按图位自然排序。

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

/// 浏览/搜索查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectionBrowseQuery {
    /// 指定浏览的收藏夹；无搜索词时仅遍历该夹（`None` 为全部）。
    /// 带搜索词时检索始终覆盖全部收藏夹，此值只用于补充图池上下文。
    pub folder_id: Option<String>,
    /// 搜索词，按空白拆分为多个关键字，全部命中才保留该行。
    #[serde(default)]
    pub search: String,
    /// 排序键：`title`、`stars`、`order`（保持收藏夹内顺序），缺省按图位自然排序。
    #[serde(default)]
    pub sort: String,
    /// 分页起点；会被限制并对齐到最后一个有效页的起点。
    #[serde(default)]
    pub offset: usize,
    /// 每页数量；缺省 50，实际收敛到 1..=100。
    pub limit: Option<usize>,
    /// 按玩家名（忽略大小写）过滤本地成绩，影响最近成绩展示。
    pub player: Option<String>,
}
/// 单个命中字段说明：字段名与围绕关键字的摘要文本。
#[derive(Serialize)]
pub struct SearchHit {
    pub field: String,
    pub text: String,
}
/// 浏览结果中的一行：条目在某个收藏夹、某个图位下的展示数据。
#[derive(Serialize)]
pub struct CollectionBrowseRow {
    /// 行唯一键：`收藏夹ID:条目ID:图位展开序号`。
    pub key: String,
    pub folder_id: String,
    pub folder_name: String,
    pub read_only: bool,
    pub entry: CollectionEntry,
    pub local: Option<LocalBeatmapSummary>,
    /// 图池槽位附带的在线谱面元数据（星级等展示字段的回退来源）。
    pub metadata: Option<serde_json::Value>,
    /// 展示用图位标签；用户手动指定的覆盖值优先。
    pub slot: String,
    /// 图池原始图位标签，未经用户覆盖。
    pub source_slot: String,
    pub selected_by: String,
    pub pool_comment: String,
    pub is_custom: bool,
    pub is_original: bool,
    pub record: PersonalRecord,
    pub latest_score: Option<LocalScore>,
    /// 记录指向的代表成绩仍能在笔记本或本地成绩中找到时为真。
    pub representative_available: bool,
    pub matches: Vec<SearchHit>,
}
/// 分页结果：`total`、`matching_folders` 与 `overview` 基于全部命中行，不受分页影响。
#[derive(Serialize)]
pub struct CollectionBrowsePage {
    pub items: Vec<CollectionBrowseRow>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    /// 名称与创建者命中全部搜索词的收藏夹 ID。
    pub matching_folders: Vec<String>,
    /// 当前浏览收藏夹关联的图池快照。
    pub pool: Option<PoolSnapshot>,
    pub overview: CollectionOverview,
}
/// 概览统计：本地已安装数、标记/笔记数、星数与 BPM 范围、总时长（毫秒）与图位组计数。
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
    // 谱面级统计按 (收藏夹, 条目) 去重，图位组计数则统计每个图位行。
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
/// 背景图定位信息：客户端与资源 ID，前端据此请求实际图片。
#[derive(Debug, Clone, Serialize)]
pub struct CollectionArtwork {
    pub client: LocalClient,
    pub resource_id: String,
}

/// 本地资源元组：`(本地 MD5, 谱面摘要, 标签文本, 来源说明)`。
type Resource = (Option<String>, LocalBeatmapSummary, String, String);
/// 本地资源索引：按小写 MD5 与 beatmap_id 建倒排表，重复时保留最先出现的资源。
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
    /// 查找条目对应的本地资源：优先校验和精确匹配，无校验和时回退 beatmap_id。
    fn find(&self, entry: &CollectionEntry) -> Option<&Resource> {
        let index = if let Some(hash) = entry.checksum.as_ref().filter(|h| !h.is_empty()) {
            self.hashes.get(&hash.to_lowercase())
        } else {
            entry.beatmap_id.and_then(|id| self.bids.get(&id))
        };
        index.map(|i| &self.items[*i])
    }
}

/// 图位排序键：组别按 NM→HD→HR→DT→FM→其他→TB 的优先级，
/// 组内按数字大小自然排序；空图位排在最后。
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
/// 多关键字 AND 检索：所有分词都命中任一字段才算匹配；
/// 摘要从最早命中位置前约 30 字符起截取 180 字符。
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
/// 汇总条目匹配本地成绩可用的全部哈希：条目校验和、本地索引 MD5 与内容哈希。
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
/// 取出文件夹与条目；条目已被移除时报 `COLLECTION_ENTRY_NOT_FOUND`。
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
/// 浏览/检索核心：遍历收藏夹生成结果行，条目占多个图位时按槽位展开为多行。
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
            // 兼容旧式 external_id（`tournament:provider:season:category`）链接的图池。
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
            // 代表成绩 ID 仍能在笔记本记录或本地成绩中找到时才视为可用。
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
            // 同一谱面占据多个图位时为每个槽位生成一行；无图位信息时以单个 None 占位。
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
                // 用户手动指定的图位标签优先于图池原始标签。
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
    // 将 offset 限制并对齐到最后一个有效页的起点，避免越界产生空页。
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

/// Tauri 命令：执行收藏夹浏览/全局搜索，返回分页行、概览与图池上下文。
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
/// Tauri 命令：读取条目的个人记录（笔记、标签、成绩与代表成绩）。
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

/// Tauri 命令：保存条目的个人记录。
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
/// Tauri 命令：列出条目的本地成绩，按条目规则集过滤，可按玩家名过滤。
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
/// Tauri 命令：汇总收藏夹谱面背景（按资源去重），从 offset 起轮转返回至多 24 张。
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
    // 图位自然排序：数字按大小比较，未知组排在 FM 与 TB 之间，空值最后。
    #[test]
    fn natural_slots_unknowns_and_unassigned() {
        let mut slots = vec!["TB1", "NM10", "", "LZ2", "HD1", "NM2", "LZ1"];
        slots.sort_by_key(|s| slot_key(s));
        assert_eq!(slots, ["NM2", "NM10", "HD1", "LZ1", "LZ2", "TB1", ""]);
    }
    // 跨字段 AND 检索与摘要截取。
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
    // 全局搜索可命中其他收藏夹的笔记；重复图位各自成行且分页与序列化体积可控。
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
