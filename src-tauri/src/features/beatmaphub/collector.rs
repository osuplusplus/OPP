//! osu!Collector 直连适配：把社区收藏集合作为 PackHub 的补充来源。
//!
//! 仅做只读浏览与导入，不写入 osu!Collector。导入时按页拉取元数据
//! 并以约 1 请求/秒节流，避免对该公益站点造成压力；无法建立连接时
//! 返回明确的语言提示（该站点在部分地区需要国际网络）。

use std::{collections::BTreeSet, time::Duration};

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

/// 面向前端的集合卡片（recent 列表与详情共用）。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorCollection {
    pub id: i64,
    pub name: String,
    pub uploader: String,
    pub description: Option<String>,
    pub beatmap_count: u32,
    pub favourites: u32,
    pub last_modified: Option<DateTime<Utc>>,
}

/// recent 列表页；`next_cursor` 透传给下一次请求实现“加载更多”。
#[derive(Debug, Clone, Serialize)]
pub struct CollectorListPage {
    pub collections: Vec<CollectorCollection>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

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
    #[serde(default)]
    collections: Vec<ApiCollection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiCollection {
    id: i64,
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
}

#[derive(Debug, Deserialize)]
struct ApiUploader {
    #[serde(default)]
    username: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiDetail {
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

    /// 浏览最近上传的收藏集合；`cursor` 传入上一页的 `next_cursor`。
    pub async fn list(&self, cursor: Option<i64>) -> CommandResult<CollectorListPage> {
        let span = global().map(|logger| logger.operation("osu_collector", "list"));
        let result = self.list_inner(cursor).await;
        finish_span(span, result)
    }

    async fn list_inner(&self, cursor: Option<i64>) -> CommandResult<CollectorListPage> {
        let url = match cursor {
            Some(cursor) => {
                format!("{OSU_COLLECTOR_BASE_URL}/api/collections/recent?cursor={cursor}")
            }
            None => format!("{OSU_COLLECTOR_BASE_URL}/api/collections/recent"),
        };
        let page: ApiListPage = self.get_json(&url).await?;
        Ok(CollectorListPage {
            collections: page
                .collections
                .iter()
                .map(|item| {
                    summary(
                        item.id,
                        &item.name,
                        &item.uploader,
                        item.beatmap_count,
                        item.favourites,
                        item.description.clone(),
                        item.date_last_modified.as_ref(),
                    )
                })
                .collect(),
            next_cursor: page.next_page_cursor,
            has_more: page.has_more,
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
            collection: summary(
                collection_id,
                &detail.name,
                &detail.uploader,
                detail.beatmap_count,
                detail.favourites,
                detail.description.clone(),
                detail.date_last_modified.as_ref(),
            ),
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
        if detail.beatmap_count as usize > MAX_IMPORT_BEATMAPS {
            return Err(CommandError::new(
                "COLLECTOR_TOO_LARGE",
                format!(
                    "该集合包含 {} 张难度，超过单次导入上限 {MAX_IMPORT_BEATMAPS}",
                    detail.beatmap_count
                ),
            ));
        }
        let mut candidates: Vec<CollectionCandidate> =
            Vec::with_capacity(detail.beatmap_count as usize);
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
                    total_beatmaps: detail.beatmap_count,
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
            .uploader
            .map(|uploader| uploader.username)
            .filter(|name| !name.is_empty());
        let folder = state.collections.create(
            &format!("[OC] {}", detail.name),
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
#[allow(clippy::too_many_arguments)]
fn summary(
    id: i64,
    name: &str,
    uploader: &Option<ApiUploader>,
    beatmap_count: u32,
    favourites: u32,
    description: Option<String>,
    last_modified: Option<&ApiTimestamp>,
) -> CollectorCollection {
    CollectorCollection {
        id,
        name: name.to_string(),
        uploader: uploader
            .as_ref()
            .map(|uploader| uploader.username.clone())
            .filter(|username| !username.is_empty())
            .unwrap_or_else(|| "osu!Collector".into()),
        description,
        beatmap_count,
        favourites,
        last_modified: last_modified.map(ApiTimestamp::to_utc),
    }
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
    fn parses_the_recent_list_page() {
        let page: ApiListPage = serde_json::from_str(
            r#"{"nextPageCursor":24441,"hasMore":true,"collections":[{"id":24458,"name":"TCMG69 - Alt Mega Pack","uploader":{"username":"doza_smerti","id":16561135,"rank":1578980},"description":"","beatmapCount":8435,"favourites":0,"dateLastModified":{"_seconds":1794532561},"dateUploaded":{"_seconds":1794532561},"modes":{"osu":8435}}]}"#,
        )
        .unwrap();
        assert_eq!(page.next_page_cursor, Some(24441));
        assert!(page.has_more);
        let item = &page.collections[0];
        let card = summary(
            item.id,
            &item.name,
            &item.uploader,
            item.beatmap_count,
            item.favourites,
            item.description.clone(),
            item.date_last_modified.as_ref(),
        );
        assert_eq!(card.id, 24458);
        assert_eq!(card.uploader, "doza_smerti");
        assert_eq!(card.beatmap_count, 8435);
        assert_eq!(
            card.last_modified.map(|time| time.timestamp()),
            Some(1_794_532_561)
        );
    }

    #[test]
    fn parses_the_detail_reference_tree() {
        let detail: ApiDetail = serde_json::from_str(
            r#"{"id":24451,"name":"jump","description":null,"uploader":{"id":11426505,"username":"Lesulite","rank":101035},"beatmapCount":10,"favourites":0,"unsubmittedBeatmapCount":0,"beatmapsets":[{"id":1024321,"beatmaps":[{"id":2459394,"checksum":"a18e6ffbfbee0c0d63751960ff9aa7af"},{"id":2732387,"checksum":"f59d40edbcdf2d8cd63e944976073b80"}]},{"id":1814624,"beatmaps":[]}],"modes":{"osu":10}}"#,
        )
        .unwrap();
        assert_eq!(detail.beatmap_count, 10);
        assert_eq!(detail.unsubmitted_beatmap_count, 0);
        // 只消费集合 ID，难度树被忽略也能解析。
        let set_ids: Vec<i64> = detail.beatmapsets.iter().map(|set| set.id).collect();
        assert_eq!(set_ids, vec![1024321, 1814624]);
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
