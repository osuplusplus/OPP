//! 生涯记录模块对前端暴露的数据契约：功能状态、抓取结果、逐日统计，
//! 以及日历与单日明细（含相对前一快照的差异）。

use serde::{Deserialize, Serialize};

use crate::domain::Ruleset;

/// 生涯记录功能的整体状态，供前端判断功能是否可用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerStatus {
    /// 本地数据库目录是否已配置（生涯库 `career.sqlite3` 随之可用）。
    pub configured: bool,
    /// `career.sqlite3` 的完整路径。
    pub path: Option<String>,
    /// 已保存的快照总数（跨所有账号与规则）。
    pub snapshot_count: u64,
    /// 最近一次快照的本地日期（`YYYY-MM-DD`，东八区）。
    pub latest_date: Option<String>,
    /// 最近一次错误信息（当前实现恒为 `None`，保留契约字段）。
    pub last_error: Option<String>,
}

/// 单次生涯快照抓取的结果摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerCaptureResult {
    /// 本次抓取归属的本地日期（`YYYY-MM-DD`，东八区）。
    pub local_date: String,
    /// `captured`（成绩与媒体齐全）或 `partial`（有缺口）。
    pub status: String,
    /// 是否真正写入/覆写了快照；当天已存在且未强制刷新时为 `false`。
    pub created: bool,
    /// 实际抓取时间（RFC3339 UTC）；直接命中已有快照时为 `None`。
    pub fetched_at: Option<String>,
    /// 玩家资料是否可用（成功进入抓取流程即视为可用）。
    pub profile_available: bool,
    /// Top 成绩是否获取成功。
    pub scores_available: bool,
    /// 本地回放/截图目录是否全部扫描成功。
    pub media_available: bool,
    /// 给用户看的提示语。
    pub message: String,
}

/// 单日快照保存的玩家统计，字段与 osu! API `statistics` 同名同口径。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CareerStats {
    pub pp: Option<f64>,
    pub global_rank: Option<f64>,
    pub country_rank: Option<f64>,
    pub ranked_score: Option<f64>,
    pub total_score: Option<f64>,
    /// 命中准确率，百分数值（98.76 表示 98.76%）。
    pub hit_accuracy: Option<f64>,
    pub play_count: Option<f64>,
    /// 累计游玩时长（osu! API 口径：分钟）。
    pub play_time: Option<f64>,
    pub total_hits: Option<f64>,
    pub maximum_combo: Option<f64>,
    /// 当前等级，取自 API 嵌套字段 `level.current`。
    pub level: Option<f64>,
    /// 当前等级内的升级进度（0-100），取自 API 嵌套字段 `level.progress`。
    pub level_progress: Option<f64>,
}

/// 日历单日条目：快照状态与统计，以及相对前一快照的差异计数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerCalendarDay {
    /// 本地日期（`YYYY-MM-DD`，东八区）。
    pub date: String,
    /// `captured` / `partial` / `unavailable` 之一。
    pub status: String,
    /// 快照时间（RFC3339 UTC）；无快照时为 `None`。
    pub captured_at: Option<String>,
    /// 当天快照的统计；无快照或 JSON 解析失败时为 `None`。
    pub stats: Option<CareerStats>,
    /// 当天抓取失败的原因。
    pub error: Option<String>,
    /// 是否有差异：成绩进出 Top、新奖牌或新回放/截图（不含 `changed_scores`）。
    pub has_diff: bool,
    /// 新进入 Top 榜的成绩数。
    pub added_scores: u32,
    /// 跌出 Top 榜的成绩数。
    pub removed_scores: u32,
    /// 榜内变化数（当前实现恒为 0）。
    pub changed_scores: u32,
    /// 新获得的奖牌数。
    pub added_medals: u32,
    /// 新增的回放文件数。
    pub added_replays: u32,
    /// 新增的截图文件数。
    pub added_screenshots: u32,
}

/// 日历查询结果；`days` 只含区间内有记录的日期，按日期升序。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerCalendar {
    pub ruleset: Ruleset,
    /// 查询区间起始日期（`YYYY-MM-DD`，原样回显）。
    pub start_date: String,
    /// 查询区间结束日期（`YYYY-MM-DD`，原样回显）。
    pub end_date: String,
    /// 区间内有记录（成功或失败）的逐日条目，无记录的日期不填充。
    pub days: Vec<CareerCalendarDay>,
}

/// 单条成绩差异：进入或离开 Top 榜；名次或成绩内容变化不产生差异。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerScoreDiff {
    /// `added` 或 `removed`。
    pub kind: String,
    /// 成绩去重键：优先服务器成绩 ID（`id:<id>`），否则按谱面+Mod+时间生成。
    pub key: String,
    /// 变化前的榜内名次（1 起）；新增成绩为 `None`。
    pub before_position: Option<u32>,
    /// 变化后的榜内名次（1 起）；移除成绩为 `None`。
    pub after_position: Option<u32>,
    /// 快照中保存的成绩原始 JSON。
    pub score: serde_json::Value,
}

/// 奖牌或媒体（回放/截图）的单条新增事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerEventItem {
    /// 事件类型，当前恒为 `added`。
    pub kind: String,
    /// 媒体所属客户端（`stable`/`lazer`）；奖牌事件为 `None`。
    pub client: Option<String>,
    /// 奖牌为成就键；媒体为文件名。
    pub name: String,
    /// 相对扫描根目录的路径；奖牌事件为 `None`。
    pub path: Option<String>,
    /// 文件字节数；奖牌事件为 `None`。
    pub size: Option<u64>,
    /// 文件修改时间（RFC3339）；奖牌事件为 `None`。
    pub modified_at: Option<String>,
    /// 奖牌为完整成就 JSON；媒体为 `{"kind": "replay"|"screenshot"}`。
    pub payload: Option<serde_json::Value>,
}

/// 单日明细：当天与前一快照的统计对比及各类差异事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerDayDetail {
    pub ruleset: Ruleset,
    pub date: String,
    /// `captured` / `partial` / `unavailable`，无记录时为 `missing`。
    pub status: String,
    pub captured_at: Option<String>,
    pub stats: Option<CareerStats>,
    /// 前一快照（快照 ID 更小的最近一条）的统计，用于对比。
    pub previous_stats: Option<CareerStats>,
    pub error: Option<String>,
    /// 相对前一快照的 Top 榜成绩进出差异，按名次排序。
    pub score_diffs: Vec<CareerScoreDiff>,
    /// 相对前一快照新获得的奖牌。
    pub medal_events: Vec<CareerEventItem>,
    /// 相对前一快照新增的回放/截图文件。
    pub media_events: Vec<CareerEventItem>,
}
