//! osu!Collector 直连适配：把社区收藏集合作为 PackHub 的补充来源。
//!
//! 仅做只读浏览与导入，不写入 osu!Collector。导入时按页拉取元数据
//! 并以约 1 请求/秒节流，避免对该公益站点造成压力；无法建立连接时
//! 返回明确的语言提示（该站点在部分地区需要国际网络）。

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::{
    error::{CommandError, CommandResult},
    features::collections::CollectionCandidate,
    infrastructure::http::{HttpClientSpec, http_client},
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

use super::models::ImportResult;

/// osu!Collector API 根地址。
pub const OSU_COLLECTOR_BASE_URL: &str = "https://osucollector.com";
/// 导入分页大小：`beatmapsV2` 单页上限 100。
const PAGE_SIZE: u32 = 100;
/// 导入条目硬上限，防止异常集合拖垮收藏夹写入（100 页 ≈ 1 万条，60 万条封顶防护）。
const MAX_IMPORT_BEATMAPS: usize = 60_000;
/// 导入分页之间的节流间隔。
const PAGE_THROTTLE: Duration = Duration::from_secs(1);

/// 面向前端的集合卡片（搜索列表与详情共用）。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorCollection {
    pub id: i64,
    pub name: String,
    pub uploader: String,
    /// 上传者 osu 用户 ID，前端用它拼 a.ppy.sh 头像；缺失为 None。
    pub uploader_id: Option<i64>,
    pub description: Option<String>,
    pub beatmap_count: u32,
    pub favourites: u32,
    pub last_modified: Option<DateTime<Utc>>,
    pub date_uploaded: Option<DateTime<Utc>>,
    /// 按整数星数分桶的难度分布（下标 0 对应 1★，共 10 桶）；旧数据缺失时为 None。
    pub difficulty_spread: Option<[u32; 10]>,
    /// 各模式的难度数量（只保留大于 0 的项，按 osu/taiko/fruits/mania 顺序）。
    pub modes: Vec<CollectorModeCount>,
}

/// 单个模式的难度数量，用于卡片上的模式分布展示。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollectorModeCount {
    pub mode: String,
    pub count: u32,
}

/// 搜索列表页；`next_cursor` 透传给下一次请求实现“加载更多”，`total` 为搜索命中总数。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorListPage {
    pub collections: Vec<CollectorCollection>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
    pub total: Option<u64>,
}

/// 搜索/浏览请求参数；全部可缺省，缺省组合等价于官网“最近上传”浏览。
#[derive(Debug, Clone, Default)]
pub struct CollectorSearch {
    /// 关键词，匹配集合名与描述；空串或 None 表示浏览全部。
    pub search: Option<String>,
    /// 排序字段，见 [`SEARCH_SORT_FIELDS`]；缺省按是否带关键词选“相关度”或“最新上传”。
    pub sort_by: Option<String>,
    /// 排序方向 `asc`/`desc`，缺省 `desc`。
    pub order_by: Option<String>,
    /// 上一页返回的游标。
    pub cursor: Option<i64>,
    /// 单页条数（1-50）。
    pub per_page: Option<u32>,
}

/// 搜索端点允许的排序字段，与 osu!Collector 官网浏览页一致（含按各模式数量排序）。
pub const SEARCH_SORT_FIELDS: &[&str] = &[
    "_text_match",
    "dateUploaded",
    "favourites",
    "osuCount",
    "taikoCount",
    "catchCount",
    "maniaCount",
];

/// 详情预览中的单个难度行，`local` 来自本地索引比对。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorBeatmap {
    pub beatmap_id: i64,
    pub beatmapset_id: i64,
    pub checksum: Option<String>,
    pub mode: String,
    pub difficulty_name: String,
    pub difficulty_rating: Option<f64>,
    pub artist: String,
    pub title: String,
    pub creator: String,
    pub local: bool,
}

/// 集合详情：元数据 + 本地已有/缺失统计 + 首页难度样本。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorCollectionDetail {
    pub collection: CollectorCollection,
    /// 集合内去重后的谱面集总数。
    pub total_sets: usize,
    pub local_sets: usize,
    pub missing_sets: usize,
    /// 未上架（无在线 ID）的难度数，这类难度无法导入或补齐。
    pub unsubmitted_beatmaps: u32,
    /// 首页样本难度行。
    pub preview: Vec<CollectorBeatmap>,
    pub has_more: bool,
}

/// 导入进度事件载荷，事件名 `collector-import-progress`。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorImportProgress {
    pub collection_id: i64,
    pub page: u32,
    pub beatmaps: usize,
    pub total_beatmaps: u32,
}

// —— osu!Collector 的原始响应结构（camelCase 字段 + Firebase 时间戳）——

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiListPage {
    #[serde(default)]
    next_page_cursor: Option<i64>,
    #[serde(default)]
    has_more: bool,
    /// 搜索命中总数（recent 端点没有该字段）。
    #[serde(default)]
    results: Option<u64>,
    #[serde(default)]
    collections: Vec<ApiCollection>,
}

/// 列表/详情共有的集合元数据；两者经 `serde(flatten)` 复用同一映射。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiCollectionCore {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    beatmap_count: u32,
    #[serde(default)]
    favourites: u32,
    #[serde(default)]
    uploader: Option<ApiUploader>,
    #[serde(default)]
    date_last_modified: Option<ApiTimestamp>,
    #[serde(default)]
    date_uploaded: Option<ApiTimestamp>,
    /// 整数星数 -> 难度数量的分桶表，键形如 "1".."10"。
    #[serde(default)]
    difficulty_spread: Option<BTreeMap<String, u32>>,
    /// 模式名 -> 难度数量，如 {"osu": 647, "taiko": 7}。
    #[serde(default)]
    modes: Option<BTreeMap<String, u32>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiCollection {
    id: i64,
    #[serde(flatten)]
    core: ApiCollectionCore,
}

#[derive(Debug, Deserialize)]
struct ApiUploader {
    #[serde(default)]
    username: String,
    #[serde(default)]
    id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiDetail {
    #[serde(flatten)]
    core: ApiCollectionCore,
    #[serde(default)]
    unsubmitted_beatmap_count: u32,
    #[serde(default)]
    beatmapsets: Vec<ApiBeatmapsetRef>,
}

/// 详情中的谱面集引用：仅消费集合 ID，难度级 ID/校验和以 beatmapsV2 分页为准。
#[derive(Debug, Deserialize)]
struct ApiBeatmapsetRef {
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiBeatmapsPage {
    #[serde(default)]
    next_page_cursor: Option<i64>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    beatmaps: Vec<ApiBeatmap>,
}

/// beatmapsV2 行对象：字段为 snake_case（与 recent/detail 的 camelCase 混用，实测如此）。
#[derive(Debug, Deserialize)]
struct ApiBeatmap {
    id: i64,
    beatmapset_id: i64,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    difficulty_rating: Option<f64>,
    #[serde(default)]
    beatmapset: Option<ApiBeatmapsetMeta>,
}

#[derive(Debug, Deserialize)]
struct ApiBeatmapsetMeta {
    #[serde(default)]
    artist: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    creator: String,
}

/// Firebase `Timestamp`：只取秒（纳秒精度对展示无意义）。
#[derive(Debug, Deserialize)]
struct ApiTimestamp {
    #[serde(rename = "_seconds")]
    seconds: i64,
}

impl ApiTimestamp {
    fn to_utc(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(self.seconds, 0)
            .single()
            .unwrap_or_default()
    }
}

/// osu!Collector 只读客户端，随 `AppState` 共享。
#[derive(Clone)]
pub struct OsuCollectorClient {
    client: reqwest::Client,
}

impl OsuCollectorClient {
    pub fn new() -> CommandResult<Self> {
        let client = http_client(HttpClientSpec {
            connect_timeout: Some(Duration::from_secs(10)),
            timeout: Some(Duration::from_secs(60)),
            ua_note: " (osu collector adapter)",
        })?;
        Ok(Self { client })
    }

    /// 按关键词与排序搜索收藏集合；空关键词即浏览全部（按最新上传排序），
    /// 与官网 `/all` 页同源。`cursor` 传入上一页的 `next_cursor`。
    pub async fn search(&self, query: CollectorSearch) -> CommandResult<CollectorListPage> {
        let span = global().map(|logger| logger.operation("osu_collector", "search"));
        let result = self.search_inner(query).await;
        finish_span(span, result)
    }

    async fn search_inner(&self, query: CollectorSearch) -> CommandResult<CollectorListPage> {
        let url = build_search_url(&query)?;
        let page: ApiListPage = self.get_json(url.as_str()).await?;
        Ok(CollectorListPage {
            collections: page
                .collections
                .iter()
                .map(|item| summary(item.id, &item.core))
                .collect(),
            next_cursor: page.next_page_cursor,
            has_more: page.has_more,
            total: page.results,
        })
    }

    /// 集合详情：本地已有/缺失按集合 ID 与本地索引比对，样本行带本地标记。
    pub async fn detail(
        &self,
        state: &AppState,
        collection_id: i64,
    ) -> CommandResult<CollectorCollectionDetail> {
        let span = global()
            .map(|logger| logger.operation("osu_collector", format!("detail:{collection_id}")));
        let result = self.detail_inner(state, collection_id).await;
        finish_span(span, result)
    }

    async fn detail_inner(
        &self,
        state: &AppState,
        collection_id: i64,
    ) -> CommandResult<CollectorCollectionDetail> {
        let detail: ApiDetail = self
            .get_json(&format!(
                "{OSU_COLLECTOR_BASE_URL}/api/collections/{collection_id}"
            ))
            .await?;
        let set_ids: BTreeSet<i32> = detail.beatmapsets.iter().map(|set| set.id as i32).collect();
        let snapshots = state.local_analysis.local_sets_by_id(&set_ids)?;
        let preview: ApiBeatmapsPage = self
            .get_json(&format!(
                "{OSU_COLLECTOR_BASE_URL}/api/collections/{collection_id}/beatmapsV2?perPage=12"
            ))
            .await?;
        let rows = preview
            .beatmaps
            .iter()
            .map(|beatmap| beatmap.to_row(snapshots.contains_key(&(beatmap.beatmapset_id as i32))))
            .collect();
        let total_sets = set_ids.len();
        let local_sets = snapshots.len();
        Ok(CollectorCollectionDetail {
            collection: summary(collection_id, &detail.core),
            total_sets,
            local_sets,
            missing_sets: total_sets.saturating_sub(local_sets),
            unsubmitted_beatmaps: detail.unsubmitted_beatmap_count,
            preview: rows,
            has_more: preview.has_more,
        })
    }

    /// 把集合导入为新的本地收藏夹。
    ///
    /// 后端按页（100 条/页、页间隔 1 秒）拉取全部难度元数据后一次性写入；
    /// 每页广播 `collector-import-progress` 事件供前端展示进度；
    /// 写入失败时回滚删除新建的收藏夹。
    pub async fn import(
        &self,
        app: &AppHandle,
        state: &AppState,
        collection_id: i64,
    ) -> CommandResult<ImportResult> {
        let span = global()
            .map(|logger| logger.operation("osu_collector", format!("import:{collection_id}")));
        let result = self.import_inner(app, state, collection_id).await;
        finish_span(span, result)
    }

    async fn import_inner(
        &self,
        app: &AppHandle,
        state: &AppState,
        collection_id: i64,
    ) -> CommandResult<ImportResult> {
        let detail: ApiDetail = self
            .get_json(&format!(
                "{OSU_COLLECTOR_BASE_URL}/api/collections/{collection_id}"
            ))
            .await?;
        if detail.core.beatmap_count as usize > MAX_IMPORT_BEATMAPS {
            return Err(CommandError::new(
                "COLLECTOR_TOO_LARGE",
                format!(
                    "该集合包含 {} 张难度，超过单次导入上限 {MAX_IMPORT_BEATMAPS}",
                    detail.core.beatmap_count
                ),
            ));
        }
        let mut candidates: Vec<CollectionCandidate> =
            Vec::with_capacity(detail.core.beatmap_count as usize);
        let mut cursor: Option<i64> = None;
        let mut page_index = 0u32;
        loop {
            if page_index > 0 {
                tokio::time::sleep(PAGE_THROTTLE).await;
            }
            let url = match cursor {
                Some(cursor) => format!(
                    "{OSU_COLLECTOR_BASE_URL}/api/collections/{collection_id}/beatmapsV2?perPage={PAGE_SIZE}&cursor={cursor}"
                ),
                None => format!(
                    "{OSU_COLLECTOR_BASE_URL}/api/collections/{collection_id}/beatmapsV2?perPage={PAGE_SIZE}"
                ),
            };
            let page: ApiBeatmapsPage = self.get_json(&url).await?;
            page_index += 1;
            candidates.extend(page.beatmaps.iter().map(ApiBeatmap::to_candidate));
            // 进度事件发送失败静默忽略，不影响导入本身。
            let _ = app.emit(
                "collector-import-progress",
                CollectorImportProgress {
                    collection_id,
                    page: page_index,
                    beatmaps: candidates.len(),
                    total_beatmaps: detail.core.beatmap_count,
                },
            );
            if !page.has_more || page.next_page_cursor.is_none() {
                break;
            }
            cursor = page.next_page_cursor;
            if candidates.len() > MAX_IMPORT_BEATMAPS {
                return Err(CommandError::new(
                    "COLLECTOR_TOO_LARGE",
                    format!("分页条目超过单次导入上限 {MAX_IMPORT_BEATMAPS}，已中止"),
                ));
            }
        }
        let set_ids: BTreeSet<i32> = candidates
            .iter()
            .filter_map(|entry| entry.beatmapset_id)
            .collect();
        let uploader = detail
            .core
            .uploader
            .as_ref()
            .map(|uploader| uploader.username.clone())
            .filter(|name| !name.is_empty());
        let folder = state.collections.create(
            &format!("[OC] {}", detail.core.name),
            &uploader.unwrap_or_else(|| "osu!Collector".into()),
        )?;
        let added = match state.collections.add_entries_bulk(&folder.id, candidates) {
            Ok(added) => added,
            Err(error) => {
                let _ = state.collections.delete(&folder.id);
                return Err(error);
            }
        };
        Ok(ImportResult {
            folder_id: folder.id,
            imported_sets: set_ids.len(),
            imported_entries: added,
            unresolved_sets: 0,
        })
    }

    /// GET + JSON 反序列化，统一超时/不可达与状态码到错误契约。
    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> CommandResult<T> {
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(map_send_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status));
        }
        response.json().await.map_err(|error| {
            CommandError::new(
                "INVALID_DATA",
                format!("osu!Collector 返回数据无法解析：{error}"),
            )
        })
    }
}

/// 由 API 对象组装前端集合卡片；上传者缺失时回退站点名。
fn summary(id: i64, core: &ApiCollectionCore) -> CollectorCollection {
    CollectorCollection {
        id,
        name: core.name.clone(),
        uploader: core
            .uploader
            .as_ref()
            .map(|uploader| uploader.username.clone())
            .filter(|username| !username.is_empty())
            .unwrap_or_else(|| "osu!Collector".into()),
        uploader_id: core
            .uploader
            .as_ref()
            .and_then(|uploader| uploader.id)
            .filter(|id| *id > 0),
        description: core.description.clone(),
        beatmap_count: core.beatmap_count,
        favourites: core.favourites,
        last_modified: core.date_last_modified.as_ref().map(ApiTimestamp::to_utc),
        date_uploaded: core.date_uploaded.as_ref().map(ApiTimestamp::to_utc),
        difficulty_spread: difficulty_spread(&core.difficulty_spread),
        modes: mode_counts(&core.modes),
    }
}

/// 模式展示顺序（与游戏内一致）；只输出数量大于 0 的模式。
const MODE_ORDER: [&str; 4] = ["osu", "taiko", "fruits", "mania"];

/// 把 API 的 `modes` 映射表压成按游戏顺序排列的非零模式列表。
fn mode_counts(map: &Option<BTreeMap<String, u32>>) -> Vec<CollectorModeCount> {
    map.as_ref()
        .map(|map| {
            MODE_ORDER
                .iter()
                .filter_map(|mode| {
                    map.get(*mode)
                        .filter(|count| **count > 0)
                        .map(|count| CollectorModeCount {
                            mode: (*mode).to_string(),
                            count: *count,
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 把 `difficultySpread` 字符串键映射表规整为固定 10 桶数组（下标 0 = 1★）；
/// 键越界或缺表时对应桶记 0，整表缺失返回 None。
fn difficulty_spread(map: &Option<BTreeMap<String, u32>>) -> Option<[u32; 10]> {
    let map = map.as_ref()?;
    let mut bins = [0u32; 10];
    for (key, value) in map {
        if let Ok(bin) = key.parse::<usize>()
            && (1..=10).contains(&bin)
        {
            bins[bin - 1] = *value;
        }
    }
    Some(bins)
}

/// 校验参数并拼搜索 URL；关键词与游标经 `Url::parse_with_params` 百分号编码。
fn build_search_url(query: &CollectorSearch) -> CommandResult<reqwest::Url> {
    // 服务端要求 search 参数必须存在（空值 = 浏览全部），整个省略会得到 500；
    // 与官网前端一致：始终携带该参数，仅做去空白处理。
    let search = query.search.as_deref().map(str::trim).unwrap_or("");
    let sort_by = match query.sort_by.as_deref() {
        None | Some("") => None,
        Some(field) if SEARCH_SORT_FIELDS.contains(&field) => Some(field),
        Some(field) => {
            return Err(CommandError::new(
                "INVALID_DATA",
                format!("不支持的 osu!Collector 排序字段：{field}"),
            ));
        }
    };
    // 缺省排序跟随官网行为：有关键词按相关度，纯浏览按最新上传。
    let sort_by = sort_by.unwrap_or(if search.is_empty() {
        "dateUploaded"
    } else {
        "_text_match"
    });
    let order_by = match query.order_by.as_deref() {
        None | Some("") | Some("desc") => "desc",
        Some("asc") => "asc",
        Some(other) => {
            return Err(CommandError::new(
                "INVALID_DATA",
                format!("不支持的 osu!Collector 排序方向：{other}"),
            ));
        }
    };
    let per_page = query.per_page.unwrap_or(24).clamp(1, 50);
    let mut params: Vec<(&str, String)> = Vec::with_capacity(5);
    params.push(("search", search.to_string()));
    params.push(("sortBy", sort_by.to_string()));
    params.push(("orderBy", order_by.to_string()));
    params.push(("perPage", per_page.to_string()));
    if let Some(cursor) = query.cursor {
        params.push(("cursor", cursor.to_string()));
    }
    reqwest::Url::parse_with_params(
        &format!("{OSU_COLLECTOR_BASE_URL}/api/collections/search"),
        params,
    )
    .map_err(|error| CommandError::network(error.to_string()))
}

/// 连接类失败（超时/DNS/拒绝）给出明确的网络提示，其余按通用网络错误上抛。
fn map_send_error(error: reqwest::Error) -> CommandError {
    if error.is_timeout() || error.is_connect() {
        CommandError::new(
            "COLLECTOR_UNREACHABLE",
            "无法连接 osu!Collector：请检查网络连接（该站点在部分地区需要国际网络）后重试",
        )
    } else {
        CommandError::network(error.to_string())
    }
}

/// HTTP 状态到错误码的映射：404 集合不存在、429 限速、其余归为请求失败。
fn status_error(status: reqwest::StatusCode) -> CommandError {
    if status == reqwest::StatusCode::NOT_FOUND {
        CommandError::new(
            "COLLECTOR_NOT_FOUND",
            "osu!Collector 收藏集合不存在或已删除",
        )
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        CommandError::new("RATE_LIMITED", "osu!Collector 请求过于频繁，请稍后重试")
    } else {
        CommandError::new(
            "COLLECTOR_REQUEST_FAILED",
            format!("osu!Collector 请求失败（{status}）"),
        )
    }
}

impl ApiBeatmap {
    /// 转为收藏夹候选条目；元数据缺失的字段回退到占位文本。
    fn to_candidate(&self) -> CollectionCandidate {
        let meta = self.beatmapset.as_ref();
        CollectionCandidate {
            beatmap_id: Some(self.id as i32),
            beatmapset_id: Some(self.beatmapset_id as i32),
            checksum: self.checksum.clone(),
            ruleset: self.mode.clone(),
            difficulty_name: self.version.clone().unwrap_or_else(|| "未知难度".into()),
            title: meta
                .map(|meta| meta.title.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| format!("Beatmapset #{}", self.beatmapset_id)),
            artist: meta.map(|meta| meta.artist.clone()).unwrap_or_default(),
            creator: meta.map(|meta| meta.creator.clone()).unwrap_or_default(),
            local_client: None,
            local_resource_id: None,
        }
    }

    /// 转为详情预览行。
    fn to_row(&self, local: bool) -> CollectorBeatmap {
        let meta = self.beatmapset.as_ref();
        CollectorBeatmap {
            beatmap_id: self.id,
            beatmapset_id: self.beatmapset_id,
            checksum: self.checksum.clone(),
            mode: self.mode.clone().unwrap_or_else(|| "osu".into()),
            difficulty_name: self.version.clone().unwrap_or_else(|| "未知难度".into()),
            difficulty_rating: self.difficulty_rating,
            artist: meta.map(|meta| meta.artist.clone()).unwrap_or_default(),
            title: meta
                .map(|meta| meta.title.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| format!("Beatmapset #{}", self.beatmapset_id)),
            creator: meta.map(|meta| meta.creator.clone()).unwrap_or_default(),
            local,
        }
    }
}

// 线上实测响应样本的解析与映射回归测试（2026-11 采集，字段如有变化先更新样本）。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_search_list_page() {
        let page: ApiListPage = serde_json::from_str(
            r#"{"nextPageCursor":2,"hasMore":true,"results":642,"collections":[{"id":2213,"name":"Aim Control/Tech/Alt/Read","uploader":{"username":"Dimension Shift","id":9044476,"rank":124178},"description":"Maps I picked to practice.","beatmapCount":658,"favourites":627,"dateLastModified":{"_seconds":1639231037},"dateUploaded":{"_seconds":1639231015},"difficultySpread":{"1":35,"2":78,"3":92,"4":116,"5":186,"6":117,"7":26,"8":8,"9":0,"10":0},"bpmSpread":{"150":206,"160":85},"modes":{"osu":647,"taiko":7,"mania":1,"fruits":3}}]}"#,
        )
        .unwrap();
        assert_eq!(page.next_page_cursor, Some(2));
        assert!(page.has_more);
        assert_eq!(page.results, Some(642));
        let card = summary(page.collections[0].id, &page.collections[0].core);
        assert_eq!(card.id, 2213);
        assert_eq!(card.uploader, "Dimension Shift");
        assert_eq!(card.uploader_id, Some(9_044_476));
        assert_eq!(card.beatmap_count, 658);
        assert_eq!(card.favourites, 627);
        assert_eq!(
            card.last_modified.map(|time| time.timestamp()),
            Some(1_639_231_037)
        );
        assert_eq!(
            card.date_uploaded.map(|time| time.timestamp()),
            Some(1_639_231_015)
        );
        assert_eq!(
            card.difficulty_spread,
            Some([35, 78, 92, 116, 186, 117, 26, 8, 0, 0])
        );
        // 模式按游戏内顺序输出且过滤零值。
        assert_eq!(
            card.modes,
            vec![
                CollectorModeCount {
                    mode: "osu".into(),
                    count: 647
                },
                CollectorModeCount {
                    mode: "taiko".into(),
                    count: 7
                },
                CollectorModeCount {
                    mode: "fruits".into(),
                    count: 3
                },
                CollectorModeCount {
                    mode: "mania".into(),
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn keeps_cards_usable_when_stats_are_missing() {
        // 旧集合或异常响应可能缺 difficultySpread / modes / uploader.id。
        let item: ApiCollection = serde_json::from_str(
            r#"{"id":24458,"name":"TCMG69 - Alt Mega Pack","uploader":{"username":"doza_smorti","id":16561135},"description":"","beatmapCount":8435,"favourites":0,"dateLastModified":{"_seconds":1794532561}}"#,
        )
        .unwrap();
        let card = summary(item.id, &item.core);
        assert_eq!(card.difficulty_spread, None);
        assert!(card.modes.is_empty());
        assert_eq!(card.date_uploaded, None);
    }

    #[test]
    fn parses_the_detail_reference_tree() {
        let detail: ApiDetail = serde_json::from_str(
            r#"{"id":24451,"name":"jump","description":null,"uploader":{"id":11426505,"username":"Lesulite","rank":101035},"beatmapCount":10,"favourites":0,"unsubmittedBeatmapCount":0,"beatmapsets":[{"id":1024321,"beatmaps":[{"id":2459394,"checksum":"a18e6ffbfbee0c0d63751960ff9aa7af"}]},{"id":1814624,"beatmaps":[]}],"difficultySpread":{"5":4,"6":6},"modes":{"osu":10}}"#,
        )
        .unwrap();
        assert_eq!(detail.core.beatmap_count, 10);
        assert_eq!(detail.unsubmitted_beatmap_count, 0);
        // 只消费集合 ID，难度树被忽略也能解析。
        let set_ids: Vec<i64> = detail.beatmapsets.iter().map(|set| set.id).collect();
        assert_eq!(set_ids, vec![1024321, 1814624]);
        let card = summary(24_451, &detail.core);
        assert_eq!(card.difficulty_spread, Some([0, 0, 0, 0, 4, 6, 0, 0, 0, 0]));
    }

    #[test]
    fn builds_search_urls_with_site_defaults() {
        // 纯浏览：按最新上传降序；search 参数必须始终携带（空值=浏览全部，
        // 整个省略会被服务端拒绝为 500）。
        let url = build_search_url(&CollectorSearch::default()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://osucollector.com/api/collections/search?search=&sortBy=dateUploaded&orderBy=desc&perPage=24"
        );
        // 有关键词：缺省排序切到相关度，关键词被百分号编码。
        let url = build_search_url(&CollectorSearch {
            search: Some("alt stream".into()),
            cursor: Some(3),
            per_page: Some(50),
            ..CollectorSearch::default()
        })
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://osucollector.com/api/collections/search?search=alt+stream&sortBy=_text_match&orderBy=desc&perPage=50&cursor=3"
        );
        // 显式排序透传。
        let url = build_search_url(&CollectorSearch {
            search: Some("nm1".into()),
            sort_by: Some("favourites".into()),
            order_by: Some("asc".into()),
            ..CollectorSearch::default()
        })
        .unwrap();
        assert!(
            url.as_str()
                .ends_with("search=nm1&sortBy=favourites&orderBy=asc&perPage=24")
        );
    }

    #[test]
    fn rejects_unknown_sort_params() {
        assert_eq!(
            build_search_url(&CollectorSearch {
                sort_by: Some("drop table".into()),
                ..CollectorSearch::default()
            })
            .unwrap_err()
            .code,
            "INVALID_DATA"
        );
        assert_eq!(
            build_search_url(&CollectorSearch {
                order_by: Some("random".into()),
                ..CollectorSearch::default()
            })
            .unwrap_err()
            .code,
            "INVALID_DATA"
        );
    }

    #[test]
    fn maps_a_beatmaps_page_row_to_a_candidate_with_checksum() {
        let page: ApiBeatmapsPage = serde_json::from_str(
            r#"{"nextPageCursor":5187911,"hasMore":true,"beatmaps":[{"beatmapset_id":2395248,"id":5192101,"checksum":"c2d40107e3fddd513a7726c0ad55c072","version":"FAITO'S HEART ATTACK","mode":"osu","difficulty_rating":6.49,"beatmapset":{"id":2395248,"creator":"Tarrasky","artist":"Lil Texas","title":"Die Young"}}]}"#,
        )
        .unwrap();
        let candidate = page.beatmaps[0].to_candidate();
        assert_eq!(candidate.beatmap_id, Some(5192101));
        assert_eq!(candidate.beatmapset_id, Some(2395248));
        assert_eq!(
            candidate.checksum.as_deref(),
            Some("c2d40107e3fddd513a7726c0ad55c072")
        );
        assert_eq!(candidate.difficulty_name, "FAITO'S HEART ATTACK");
        assert_eq!(candidate.title, "Die Young");
        let row = page.beatmaps[0].to_row(true);
        assert!(row.local);
        assert_eq!(row.difficulty_rating, Some(6.49));
    }

    #[test]
    fn missing_metadata_falls_back_to_placeholders() {
        let beatmap: ApiBeatmap = serde_json::from_str(r#"{"id":1,"beatmapset_id":7}"#).unwrap();
        let candidate = beatmap.to_candidate();
        assert_eq!(candidate.title, "Beatmapset #7");
        assert_eq!(candidate.difficulty_name, "未知难度");
        assert_eq!(candidate.ruleset, None);
    }
}
