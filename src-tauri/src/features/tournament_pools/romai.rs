//! RomAI 图池来源：把 romai.dev 的公开图池库作为可浏览、可导入的比赛图池。
//!
//! 数据来自两个公开 JSON 端点：`/api/map-pools`（全量图池，含各模组的难度 ID）
//! 与 `/api/pools`（展示摘要，含每池最多 4 张封面的谱面集 ID）。全量数据约
//! 0.5 MiB 且更新低频，整体缓存在 `<app_data_dir>/cache/romai/` 下：一小时内的
//! 缓存直接复用，过期后带 ETag 条件请求，网络失败时回退到本地缓存，离线可浏览。
//! 按照项目约定，缓存属于运行时数据，只落应用数据目录、不进仓库。

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::models::{
    TournamentInfo, TournamentPool, TournamentPoolEntry, TournamentPoolRef, romai_tier,
};
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::{
        http::{HttpClientSpec, http_client},
        logging::{finish_span, global},
    },
};

/// 全量图池端点（裸 JSON 数组，约 0.5 MiB）。
const MAP_POOLS_URL: &str = "https://romai.dev/api/map-pools";
/// 展示摘要端点（含封面谱面集 ID）。
const DISPLAY_URL: &str = "https://romai.dev/api/pools";
/// 单端点响应大小上限（当前全量约 0.5 MiB，预留增长空间）。
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// 缓存新鲜窗口：窗口内直接复用本地缓存，不发起网络请求。
const FRESH_WINDOW_SECS: i64 = 3600;
/// 图池条目硬上限，防止异常数据拖垮收藏夹写入（正常图池不超过 30 图位）。
const MAX_POOL_ENTRIES: usize = 200;

/// 图位类型在图池内的固定展示顺序（与 rino 的排序保持一致，TB 收尾）。
const SELECTION_ORDER: [&str; 6] = ["NM", "HD", "HR", "DT", "FM", "TB"];

// —— RomAI 原始响应结构（camelCase；`_id` 为 Mongo ObjectId 字符串）——

/// `/api/map-pools` 的单个图池记录（wire 字段为 camelCase）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiPool {
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    elo: i32,
    #[serde(default)]
    maps: ApiPoolMaps,
}

/// 图池内各模组的难度 ID 列表；`tieBreaker` 为单个难度 ID（wire 字段为 camelCase）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiPoolMaps {
    #[serde(default)]
    no_mod: Vec<i64>,
    #[serde(default)]
    hidden: Vec<i64>,
    #[serde(default)]
    hard_rock: Vec<i64>,
    #[serde(default)]
    double_time: Vec<i64>,
    #[serde(default)]
    free_mod: Vec<i64>,
    #[serde(default)]
    tie_breaker: Option<i64>,
}

/// `/api/pools` 展示摘要的单个图池条目。
#[derive(Debug, Clone, Deserialize)]
struct ApiDisplayPool {
    #[serde(default)]
    name: String,
    #[serde(default)]
    elo: i32,
    #[serde(default)]
    covers: Vec<i64>,
}

/// `/api/pools` 响应外壳。
#[derive(Debug, Clone, Deserialize)]
struct ApiDisplay {
    #[serde(default)]
    pools: Vec<ApiDisplayPool>,
}

// —— 面向前端的浏览契约 ——

/// 各模组图位数量；`tb` 有 TieBreaker 时为 1。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RomaiModCounts {
    pub nm: u32,
    pub hd: u32,
    pub hr: u32,
    pub dt: u32,
    pub fm: u32,
    pub tb: u32,
}

/// 图池浏览卡片：名称、ELO 段位、图位统计与封面谱面集 ID。
#[derive(Debug, Clone, Serialize)]
pub struct RomaiPoolSummary {
    pub id: String,
    pub name: String,
    pub elo: i32,
    /// 段位 ID（candidate/silver/gold/platinum/diamond/atomos/cosmic/quantum）。
    pub tier: &'static str,
    pub map_count: u32,
    pub counts: RomaiModCounts,
    /// 封面谱面集 ID（最多 4 个，展示端点未覆盖时为空），用于拼 assets.ppy.sh 封面。
    pub covers: Vec<i64>,
}

/// 图池索引浏览结果：全量摘要 + 数据时间戳。
#[derive(Debug, Clone, Serialize)]
pub struct RomaiPoolIndex {
    pub pools: Vec<RomaiPoolSummary>,
    /// 最近一次成功拉取的 Unix 秒时间戳。
    pub fetched_at: i64,
    /// 本次调用是否未走网络（缓存命中，可能为过期回退）。
    pub from_cache: bool,
}

// —— 缓存文档 ——

/// 单个端点的缓存文档：ETag + 拉取时间 + 原始响应体。
#[derive(Debug, Clone, Deserialize, Serialize)]
struct CacheDoc {
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    fetched_at: i64,
    body: Value,
}

impl CacheDoc {
    fn now(body: Value, etag: Option<String>) -> Self {
        Self {
            etag,
            fetched_at: Utc::now().timestamp(),
            body,
        }
    }

    fn is_fresh(&self) -> bool {
        Utc::now().timestamp() - self.fetched_at < FRESH_WINDOW_SECS
    }
}

/// RomAI 图池客户端：随 `AppState` 共享，负责两个端点的条件拉取与缓存。
/// `index_refresh` 为单飞锁：并发的浏览/详情请求共享一次索引下载。
#[derive(Clone)]
pub struct RomaiPoolsClient {
    http: reqwest::Client,
    cache_dir: PathBuf,
    index_refresh: Arc<tokio::sync::Mutex<()>>,
}

impl RomaiPoolsClient {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        Ok(Self {
            index_refresh: Arc::new(tokio::sync::Mutex::new(())),
            http: http_client(HttpClientSpec {
                connect_timeout: Some(Duration::from_secs(10)),
                timeout: Some(Duration::from_secs(60)),
                ua_note: " (romai pools adapter)",
            })?,
            cache_dir: app_data_dir.join("cache").join("romai"),
        })
    }

    /// 浏览全量图池索引：全量数据必须可得（缓存或网络），
    /// 展示摘要失败时退化为无封面卡片，不阻塞浏览。
    pub async fn summaries(&self) -> CommandResult<RomaiPoolIndex> {
        let span = global().map(|logger| logger.operation("romai_pools", "summaries"));
        let result = self.summaries_inner().await;
        finish_span(span, result)
    }

    async fn summaries_inner(&self) -> CommandResult<RomaiPoolIndex> {
        // 单飞 + 并行拉取：锁内同时请求两个端点，并发调用共享一次下载
        // （不重复拉 0.5 MiB 索引），首次加载只等较慢的那个。
        let (index_result, display_result) = {
            let _guard = self.index_refresh.lock().await;
            tokio::join!(
                self.ensure(MAP_POOLS_URL, "map-pools.json"),
                self.ensure(DISPLAY_URL, "display-pools.json")
            )
        };
        let (index_doc, from_cache) = index_result?;
        let pools: Vec<ApiPool> = serde_json::from_value(index_doc.body.clone()).map_err(|e| {
            CommandError::new("ROMAI_INVALID_DATA", format!("RomAI 图池数据无法解析：{e}"))
        })?;
        // 展示摘要软失败：拿不到封面时依旧返回可浏览的索引。
        let covers = display_result
            .ok()
            .map(|(doc, _)| decode_display(&doc.body))
            .unwrap_or_default();
        let mut summaries: Vec<RomaiPoolSummary> = pools
            .into_iter()
            .filter(|pool| !pool.name.is_empty())
            .map(|pool| {
                let counts = mod_counts(&pool.maps);
                let map_count =
                    counts.nm + counts.hd + counts.hr + counts.dt + counts.fm + counts.tb;
                let key = (pool.name.clone(), pool.elo);
                RomaiPoolSummary {
                    id: pool.id,
                    tier: romai_tier(pool.elo),
                    map_count,
                    counts,
                    covers: covers.get(&key).cloned().unwrap_or_default(),
                    name: pool.name,
                    elo: pool.elo,
                }
            })
            .collect();
        // 默认按 ELO 降序（高手池在前），前端可再切换排序。
        summaries.sort_by(|a, b| b.elo.cmp(&a.elo).then_with(|| a.name.cmp(&b.name)));
        Ok(RomaiPoolIndex {
            pools: summaries,
            fetched_at: index_doc.fetched_at,
            from_cache,
        })
    }

    /// 按引用拉取单个图池并转换为通用图池结构（谱面元数据由 service 层补全）。
    pub async fn fetch_pool(&self, reference: &TournamentPoolRef) -> CommandResult<TournamentPool> {
        let span = global().map(|logger| logger.operation("romai_pools", "fetch_pool"));
        let result = self.fetch_pool_inner(reference).await;
        finish_span(span, result)
    }

    async fn fetch_pool_inner(
        &self,
        reference: &TournamentPoolRef,
    ) -> CommandResult<TournamentPool> {
        reference.validate()?;
        let pool_id = reference.pool_id.clone().unwrap_or_default();
        let (index_doc, _) = self.ensure_index().await?;
        let pools: Vec<ApiPool> = serde_json::from_value(index_doc.body.clone()).map_err(|e| {
            CommandError::new("ROMAI_INVALID_DATA", format!("RomAI 图池数据无法解析：{e}"))
        })?;
        let pool = pools
            .iter()
            .find(|pool| pool.id == pool_id)
            .ok_or_else(|| CommandError::new("ROMAI_POOL_NOT_FOUND", "RomAI 图池不存在或已下架"))?;
        decode_pool(pool, reference)
    }

    /// 全量索引保障（单飞）：与浏览共用同一把刷新锁，浏览正在下载索引时
    /// 详情请求等待复用同一次结果，而不是并发再拉一份 0.5 MiB 数据。
    async fn ensure_index(&self) -> CommandResult<(CacheDoc, bool)> {
        let guard = self.index_refresh.lock().await;
        let result = self.ensure(MAP_POOLS_URL, "map-pools.json").await;
        drop(guard);
        result
    }

    /// 端点数据保障：缓存新鲜直接复用；过期带 ETag 条件请求（304 只刷新时间戳）；
    /// 网络失败时回退到任意本地缓存，完全没有缓存才报网络错误。
    async fn ensure(&self, url: &str, cache_file: &str) -> CommandResult<(CacheDoc, bool)> {
        let path = self.cache_dir.join(cache_file);
        let cached = load_cache(&path);
        if let Some(doc) = cached.as_ref()
            && doc.is_fresh()
        {
            return Ok((doc.clone(), true));
        }
        match self
            .fetch_conditional(url, cached.as_ref().and_then(|doc| doc.etag.as_deref()))
            .await
        {
            Ok(Some(doc)) => {
                let _ = std::fs::create_dir_all(&self.cache_dir);
                let _ = std::fs::write(&path, serde_json::to_vec(&doc).unwrap_or_default());
                Ok((doc, false))
            }
            // 304 Not Modified：内容未变，仅刷新时间戳后落盘。
            Ok(None) => {
                let Some(mut doc) = cached else {
                    return Err(CommandError::new(
                        "ROMAI_INVALID_DATA",
                        "RomAI 在没有本地缓存时返回 304",
                    ));
                };
                doc.fetched_at = Utc::now().timestamp();
                let _ = std::fs::write(&path, serde_json::to_vec(&doc).unwrap_or_default());
                Ok((doc, true))
            }
            Err(error) => match cached {
                Some(doc) => Ok((doc, true)),
                None => Err(error),
            },
        }
    }

    /// 条件 GET：200 返回新文档、304 返回 None、其余（含网络失败）上抛错误。
    async fn fetch_conditional(
        &self,
        url: &str,
        etag: Option<&str>,
    ) -> CommandResult<Option<CacheDoc>> {
        let mut request = self.http.get(url).header("Accept", "application/json");
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let response = request.send().await.map_err(map_network_error)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(CommandError::new(
                "ROMAI_REQUEST_FAILED",
                format!("RomAI 请求失败（{status}）"),
            ));
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(map_network_error)?;
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(CommandError::new(
                    "ROMAI_INVALID_DATA",
                    "RomAI 响应过大，已中止下载",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes).map_err(|e| {
            CommandError::new("ROMAI_INVALID_DATA", format!("RomAI 返回数据无法解析：{e}"))
        })?;
        Ok(Some(CacheDoc::now(body, etag)))
    }
}

/// 读取缓存文档；文件缺失或损坏（半截写入/格式变化）一律视为无缓存，下次重拉。
fn load_cache(path: &Path) -> Option<CacheDoc> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// 解析展示摘要，建立“(名称, ELO) → 封面谱面集 ID”对照表。
fn decode_display(body: &Value) -> HashMap<(String, i32), Vec<i64>> {
    serde_json::from_value::<ApiDisplay>(body.clone())
        .map(|display| {
            display
                .pools
                .into_iter()
                .filter(|pool| !pool.name.is_empty())
                .map(|pool| ((pool.name, pool.elo), pool.covers))
                .collect()
        })
        .unwrap_or_default()
}

/// 统计各模组图位数量；TieBreaker 存在时计 1。
fn mod_counts(maps: &ApiPoolMaps) -> RomaiModCounts {
    RomaiModCounts {
        nm: maps.no_mod.len() as u32,
        hd: maps.hidden.len() as u32,
        hr: maps.hard_rock.len() as u32,
        dt: maps.double_time.len() as u32,
        fm: maps.free_mod.len() as u32,
        tb: u32::from(maps.tie_breaker.is_some()),
    }
}

/// 把 RomAI 图池记录转换为通用图池：各模组列表按下标生成 `NM1…` 图位，
/// TieBreaker 固定为 `TB1`；非法难度 ID（≤0）丢弃，超上限拒绝。
fn decode_pool(pool: &ApiPool, reference: &TournamentPoolRef) -> CommandResult<TournamentPool> {
    let mut entries: Vec<TournamentPoolEntry> = Vec::new();
    let mut push = |selection_type: &str, ids: &[i64]| {
        for (index, id) in ids.iter().enumerate() {
            if *id > 0 && *id <= i32::MAX as i64 {
                entries.push(slot(*id as i32, selection_type, index as u32 + 1));
            }
        }
    };
    push("NM", &pool.maps.no_mod);
    push("HD", &pool.maps.hidden);
    push("HR", &pool.maps.hard_rock);
    push("DT", &pool.maps.double_time);
    push("FM", &pool.maps.free_mod);
    if let Some(id) = pool.maps.tie_breaker
        && (0 < id && id <= i32::MAX as i64)
    {
        entries.push(slot(id as i32, "TB", 1));
    }
    if entries.is_empty() {
        return Err(CommandError::new(
            "ROMAI_POOL_EMPTY",
            "该图池没有任何有效图位",
        ));
    }
    if entries.len() > MAX_POOL_ENTRIES {
        return Err(CommandError::new(
            "ROMAI_POOL_TOO_LARGE",
            format!(
                "该图池包含 {} 个图位，超过上限 {MAX_POOL_ENTRIES}",
                entries.len()
            ),
        ));
    }
    entries.sort_by(|a, b| {
        selection_rank(&a.selection_type)
            .cmp(&selection_rank(&b.selection_type))
            .then(a.position.cmp(&b.position))
    });
    Ok(TournamentPool {
        reference: reference.clone(),
        title: pool.name.clone(),
        info: TournamentInfo {
            tournament: Some("RomAI".into()),
            season: None,
            category: None,
        },
        entries,
    })
}

/// 单个图位；RomAI 数据不含选图人与备注，全部留空。
fn slot(beatmap_id: i32, selection_type: &str, position: u32) -> TournamentPoolEntry {
    TournamentPoolEntry {
        beatmap_id,
        selection_type: selection_type.to_string(),
        position,
        selected_by: None,
        selected_by_name: None,
        comment: String::new(),
        is_custom: false,
        is_original: false,
        beatmap: None,
        resolution_error: None,
    }
}

fn selection_rank(selection_type: &str) -> usize {
    SELECTION_ORDER
        .iter()
        .position(|item| *item == selection_type)
        .unwrap_or(SELECTION_ORDER.len())
}

/// 连接类失败给出明确的网络提示，与 osu!Collector 适配的文案口径一致。
fn map_network_error(error: reqwest::Error) -> CommandError {
    if error.is_timeout() || error.is_connect() {
        CommandError::new(
            "ROMAI_UNREACHABLE",
            "无法连接 RomAI：请检查网络连接（该站点在部分地区需要国际网络）后重试",
        )
    } else {
        CommandError::network(error.to_string())
    }
}

// 手写小样本解析回归（非真实数据快照，字段变化时先更新样本）。
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pool_json() -> Value {
        serde_json::json!([
            {
                "_id": "6649b30ad5efb507e7b8d7b5",
                "name": "Sample Cup 2026 (Finals)",
                "elo": 1676,
                "maps": {
                    "noMod": [3299389, 3299406, 1722011],
                    "hidden": [1843575, 2623967],
                    "hardRock": [2633812],
                    "doubleTime": [546514, 53340],
                    "freeMod": [],
                    "tieBreaker": 3299464
                },
                "__v": 0
            }
        ])
    }

    #[test]
    fn maps_wire_fields_to_mod_slots() {
        let pools: Vec<ApiPool> = serde_json::from_value(sample_pool_json()).unwrap();
        let counts = mod_counts(&pools[0].maps);
        assert_eq!(
            counts,
            RomaiModCounts {
                nm: 3,
                hd: 2,
                hr: 1,
                dt: 2,
                fm: 0,
                tb: 1
            }
        );
    }

    #[test]
    fn decodes_a_pool_into_sorted_labeled_slots() {
        let pools: Vec<ApiPool> = serde_json::from_value(sample_pool_json()).unwrap();
        let reference = TournamentPoolRef {
            provider: "romai".into(),
            season: String::new(),
            category: String::new(),
            url: None,
            pool_id: Some(pools[0].id.clone()),
        };
        let pool = decode_pool(&pools[0], &reference).unwrap();
        assert_eq!(pool.title, "Sample Cup 2026 (Finals)");
        let labels: Vec<String> = pool
            .entries
            .iter()
            .map(|entry| format!("{}{}", entry.selection_type, entry.position))
            .collect();
        assert_eq!(
            labels,
            vec![
                "NM1", "NM2", "NM3", "HD1", "HD2", "HR1", "DT1", "DT2", "TB1"
            ]
        );
        assert_eq!(pool.entries[0].beatmap_id, 3299389);
        // 最后一项为 TieBreaker。
        assert_eq!(pool.entries.last().unwrap().beatmap_id, 3299464);
    }

    #[test]
    fn rejects_pools_without_valid_slots() {
        let pools: Vec<ApiPool> = serde_json::from_value(serde_json::json!([
            { "_id": "6649b30ad5efb507e7b8d7b6", "name": "Empty", "elo": 100 }
        ]))
        .unwrap();
        let reference = TournamentPoolRef {
            provider: "romai".into(),
            season: String::new(),
            category: String::new(),
            url: None,
            pool_id: Some(pools[0].id.clone()),
        };
        assert_eq!(
            decode_pool(&pools[0], &reference).unwrap_err().code,
            "ROMAI_POOL_EMPTY"
        );
    }

    #[test]
    fn merges_display_covers_by_name_and_elo() {
        let display = serde_json::json!({
            "pools": [
                { "name": "Sample Cup 2026 (Finals)", "elo": 1676, "mapCount": 9,
                  "counts": { "NM": 3, "HD": 2, "HR": 1, "DT": 2, "FM": 0, "TB": 1 },
                  "covers": [1616036, 1616046] }
            ]
        });
        let covers = decode_display(&display);
        assert_eq!(
            covers.get(&("Sample Cup 2026 (Finals)".into(), 1676)),
            Some(&vec![1_616_036, 1_616_046])
        );
    }

    #[test]
    fn cache_round_trips_and_reports_freshness() {
        let doc = CacheDoc::now(serde_json::json!([1]), Some("W/\"abc\"".into()));
        let bytes = serde_json::to_vec(&doc).unwrap();
        let loaded: CacheDoc = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(loaded.etag.as_deref(), Some("W/\"abc\""));
        assert!(loaded.is_fresh());
        // 损坏缓存按缺失处理，触发下次重拉。
        assert!(serde_json::from_slice::<CacheDoc>(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn tiers_follow_elo_thresholds() {
        assert_eq!(romai_tier(983), "candidate");
        assert_eq!(romai_tier(1399), "candidate");
        assert_eq!(romai_tier(1400), "silver");
        assert_eq!(romai_tier(1699), "platinum");
        assert_eq!(romai_tier(1700), "diamond");
        assert_eq!(romai_tier(1999), "cosmic");
        assert_eq!(romai_tier(2282), "quantum");
    }
}
