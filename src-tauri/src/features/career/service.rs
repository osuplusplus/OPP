//! 生涯记录服务层：负责 `career.sqlite3` 的打开与迁移、每日快照写入、
//! 日历与单日明细查询，以及相对前一快照的差异统计。

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
};

use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{
    domain::{OwnProfile, Ruleset, Score},
    error::{CommandError, CommandResult},
    features::{
        game_session::media_roots, local_analysis::LocalClient,
        local_database::LocalDatabaseService,
    },
};

use super::models::*;

/// SQLite `application_id` 标识（ASCII "OPPC"），用于识别 OPP 生涯库文件。
const APPLICATION_ID: i64 = 0x4f505043;
/// 快照归属时区；本地日期统一按东八区（Asia/Shanghai）切分。
const TIME_ZONE: &str = "Asia/Shanghai";

/// 已打开的生涯库连接；记录路径以便切换数据库目录后重开。
struct CareerDatabase {
    connection: Connection,
    path: PathBuf,
}

/// `day` 查询的原始行：(状态, 错误, 快照时间, 统计 JSON, 快照 ID)。
type CareerDayRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

/// 生涯记录服务：持有惰性打开的 `career.sqlite3` 连接，提供状态查询、
/// 快照抓取、日历/单日查询与历史清理。
pub(crate) struct CareerService {
    /// 本地数据库服务，用于确定生涯库所在目录。
    local_database: std::sync::Arc<LocalDatabaseService>,
    /// 惰性连接，首次访问或目录切换时打开；互斥锁串行化多命令访问。
    runtime: Mutex<Option<CareerDatabase>>,
}

impl CareerService {
    pub(crate) fn new(local_database: std::sync::Arc<LocalDatabaseService>) -> Self {
        Self {
            local_database,
            runtime: Mutex::new(None),
        }
    }

    /// 获取数据库连接的排他守卫；本地库未配置时返回 `None`。
    ///
    /// 首次访问或 `career.sqlite3` 路径变化时打开连接：先校验
    /// `application_id` 防止误用其他 SQLite 文件，再开启 WAL、外键
    /// 约束并执行建表迁移。
    fn database(&self) -> CommandResult<Option<std::sync::MutexGuard<'_, Option<CareerDatabase>>>> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| CommandError::new("CAREER_STATE_ERROR", "生涯数据库状态异常"))?;
        let Some(directory) = self.local_database.configured_directory()? else {
            return Ok(None);
        };
        let path = directory.join("career.sqlite3");
        // 首次访问或 career.sqlite3 路径变化（切换数据库目录）时才重新打开。
        let needs_open = runtime.as_ref().map(|db| db.path != path).unwrap_or(true);
        if needs_open {
            let connection = Connection::open(&path).map_err(sql_error)?;
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .map_err(sql_error)?;
            let application_id: i64 = connection
                .query_row("PRAGMA application_id", [], |row| row.get(0))
                .map_err(sql_error)?;
            // 非零且不匹配的 application_id 说明是其他程序的库，直接拒绝。
            if application_id != 0 && application_id != APPLICATION_ID {
                return Err(CommandError::new(
                    "CAREER_DATABASE_IDENTITY",
                    "所选 career.sqlite3 不是 OPP 生涯数据库",
                ));
            }
            connection
                .pragma_update(None, "journal_mode", "WAL")
                .map_err(sql_error)?;
            connection
                .pragma_update(None, "foreign_keys", true)
                .map_err(sql_error)?;
            connection
                .pragma_update(None, "application_id", APPLICATION_ID)
                .map_err(sql_error)?;
            connection.execute_batch(SCHEMA).map_err(sql_error)?;
            connection
                .pragma_update(None, "user_version", 1)
                .map_err(sql_error)?;
            *runtime = Some(CareerDatabase { connection, path });
        }
        drop(runtime);
        Ok(Some(self.runtime.lock().map_err(|_| {
            CommandError::new("CAREER_STATE_ERROR", "生涯数据库状态异常")
        })?))
    }

    /// 返回功能状态：快照总数与最近快照日期（跨所有账号与规则）。
    pub(crate) fn status(&self) -> CommandResult<CareerStatus> {
        let Some(mut guard) = self.database()? else {
            return Ok(CareerStatus {
                configured: false,
                path: None,
                snapshot_count: 0,
                latest_date: None,
                last_error: None,
            });
        };
        let db = guard.as_mut().expect("database initialized");
        let count: u64 = db
            .connection
            .query_row("SELECT COUNT(*) FROM career_snapshots", [], |r| r.get(0))
            .map_err(sql_error)?;
        let latest: Option<String> = db
            .connection
            .query_row("SELECT MAX(local_date) FROM career_snapshots", [], |r| {
                r.get(0)
            })
            .map_err(sql_error)?;
        Ok(CareerStatus {
            configured: true,
            path: Some(db.path.to_string_lossy().into_owned()),
            snapshot_count: count,
            latest_date: latest,
            last_error: None,
        })
    }

    /// 判断指定“账号+规则”今天（东八区）是否已有 `captured` 状态的快照。
    pub(crate) fn has_captured_today(&self, user_id: u64, ruleset: Ruleset) -> CommandResult<bool> {
        let Some(mut guard) = self.database()? else {
            return Ok(false);
        };
        let db = guard.as_mut().expect("database initialized");
        let date = local_date(Utc::now());
        let status: Option<String> = db
            .connection
            .query_row(
                "SELECT d.status FROM career_days d JOIN career_accounts a ON a.id=d.account_id WHERE a.user_id=?1 AND a.ruleset=?2 AND d.local_date=?3",
                params![user_id as i64, ruleset.to_string(), date],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        Ok(status.as_deref() == Some("captured"))
    }

    /// 写入或覆写当天（东八区）的生涯快照。
    ///
    /// `scores` 为 `None` 表示本次未获取成绩（不覆写已有成绩数据）；
    /// `sections` 为各分区抓取结果的 JSON 标记，这里只读取其中的 `media`。
    /// 当天已有快照且 `force` 为 `false` 时直接短路返回；成绩与媒体
    /// 分区都成功才标记 `captured`，否则为 `partial`。
    pub(crate) fn capture(
        &self,
        profile: &OwnProfile,
        ruleset: Ruleset,
        scores: Option<&[Score]>,
        media: &[MediaRecord],
        sections: &Value,
        force: bool,
    ) -> CommandResult<CareerCaptureResult> {
        let date = local_date(Utc::now());
        let Some(mut guard) = self.database()? else {
            return Err(CommandError::new(
                "CAREER_DATABASE_UNAVAILABLE",
                "请先配置本地数据库后启用生涯记录",
            ));
        };
        let db = guard.as_mut().unwrap();
        let tx = db.connection.transaction().map_err(sql_error)?;
        tx.execute("INSERT INTO career_accounts(user_id,ruleset,username,timezone) VALUES(?1,?2,?3,?4) ON CONFLICT(user_id,ruleset) DO UPDATE SET username=excluded.username", params![profile.id as i64, ruleset.to_string(), profile.username, TIME_ZONE]).map_err(sql_error)?;
        let account_id: i64 = tx
            .query_row(
                "SELECT id FROM career_accounts WHERE user_id=?1 AND ruleset=?2",
                params![profile.id as i64, ruleset.to_string()],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        // 外层 Option 表示当天是否有日历行，内层表示是否已挂快照。
        let existing: Option<Option<i64>> = tx
            .query_row(
                "SELECT snapshot_id FROM career_days WHERE account_id=?1 AND local_date=?2",
                params![account_id, date],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let existing_snapshot = existing.flatten();
        if existing_snapshot.is_some() && !force {
            tx.commit().map_err(sql_error)?;
            return Ok(CareerCaptureResult {
                local_date: date,
                status: "captured".into(),
                created: false,
                fetched_at: None,
                profile_available: true,
                scores_available: true,
                media_available: true,
                message: "今天的快照已经存在".into(),
            });
        }
        let captured_at = Utc::now().to_rfc3339();
        let stats = extract_stats(profile.statistics.as_ref().unwrap_or(&Value::Null));
        let profile_json = serde_json::to_string(profile)
            .map_err(|e| CommandError::new("CAREER_SERIALIZE_FAILED", e.to_string()))?;
        let stats_json = serde_json::to_string(&stats)
            .map_err(|e| CommandError::new("CAREER_SERIALIZE_FAILED", e.to_string()))?;
        // 成绩与媒体两个分区都拿到才算完整抓取。
        let capture_status = if scores.is_some()
            && sections
                .get("media")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            "captured"
        } else {
            "partial"
        };
        if let Some(snapshot_id) = existing_snapshot {
            if scores.is_some() {
                tx.execute(
                    "DELETE FROM career_scores WHERE snapshot_id=?1",
                    [snapshot_id],
                )
                .map_err(sql_error)?;
            }
            tx.execute(
                "DELETE FROM career_medals WHERE snapshot_id=?1",
                [snapshot_id],
            )
            .map_err(sql_error)?;
            if sections
                .get("media")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                tx.execute(
                    "DELETE FROM career_media WHERE snapshot_id=?1",
                    [snapshot_id],
                )
                .map_err(sql_error)?;
            }
            tx.execute("UPDATE career_snapshots SET captured_at=?1, profile_json=?2, stats_json=?3, sections_json=?4 WHERE id=?5", params![captured_at, profile_json, stats_json, sections.to_string(), snapshot_id]).map_err(sql_error)?;
            if let Some(scores) = scores {
                insert_scores(&tx, snapshot_id, scores)?;
            }
            insert_medals(
                &tx,
                snapshot_id,
                profile.user_achievements.as_deref().unwrap_or(&[]),
            )?;
            insert_media(&tx, snapshot_id, media)?;
            tx.execute("UPDATE career_days SET status=?1, attempted_at=?2, error=NULL WHERE account_id=?3 AND local_date=?4", params![capture_status, captured_at, account_id, date]).map_err(sql_error)?;
        } else {
            tx.execute("INSERT INTO career_snapshots(account_id,ruleset,local_date,captured_at,timezone,profile_json,stats_json,sections_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![account_id, ruleset.to_string(), date, captured_at, TIME_ZONE, profile_json, stats_json, sections.to_string()]).map_err(sql_error)?;
            let snapshot_id = tx.last_insert_rowid();
            if let Some(scores) = scores {
                insert_scores(&tx, snapshot_id, scores)?;
            }
            insert_medals(
                &tx,
                snapshot_id,
                profile.user_achievements.as_deref().unwrap_or(&[]),
            )?;
            insert_media(&tx, snapshot_id, media)?;
            tx.execute("INSERT INTO career_days(account_id,local_date,status,attempted_at,snapshot_id) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account_id,local_date) DO UPDATE SET status=excluded.status,attempted_at=excluded.attempted_at,error=NULL,snapshot_id=excluded.snapshot_id", params![account_id, date, capture_status, captured_at, snapshot_id]).map_err(sql_error)?;
        }
        tx.commit().map_err(sql_error)?;
        Ok(CareerCaptureResult {
            local_date: date,
            status: capture_status.into(),
            created: true,
            fetched_at: Some(captured_at),
            profile_available: true,
            scores_available: scores.is_some(),
            media_available: sections
                .get("media")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            message: "生涯快照已保存".into(),
        })
    }

    /// 记录当天抓取失败：当天无快照时状态置 `unavailable`，
    /// 已有快照则降级为 `partial`，错误信息写入日历行。
    pub(crate) fn record_failure(
        &self,
        user_id: Option<u64>,
        username: Option<&str>,
        ruleset: Ruleset,
        error: &str,
    ) -> CommandResult<()> {
        let Some(user_id) = user_id else {
            return Ok(());
        };
        let Some(mut guard) = self.database()? else {
            return Ok(());
        };
        let db = guard.as_mut().unwrap();
        let tx = db.connection.transaction().map_err(sql_error)?;
        let user_id = user_id as i64;
        tx.execute("INSERT INTO career_accounts(user_id,ruleset,username,timezone) VALUES(?1,?2,?3,?4) ON CONFLICT(user_id,ruleset) DO UPDATE SET username=COALESCE(excluded.username,career_accounts.username)", params![user_id, ruleset.to_string(), username, TIME_ZONE]).map_err(sql_error)?;
        let account_id: i64 = tx
            .query_row(
                "SELECT id FROM career_accounts WHERE user_id=?1 AND ruleset=?2",
                params![user_id, ruleset.to_string()],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        let date = local_date(Utc::now());
        tx.execute("INSERT INTO career_days(account_id,local_date,status,attempted_at,error) VALUES(?1,?2,'unavailable',?3,?4) ON CONFLICT(account_id,local_date) DO UPDATE SET status=CASE WHEN career_days.snapshot_id IS NULL THEN 'unavailable' ELSE 'partial' END, attempted_at=excluded.attempted_at, error=excluded.error", params![account_id, date, Utc::now().to_rfc3339(), error]).map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }

    /// 查询日历：逐日返回状态、统计与相对前一快照的差异计数。
    ///
    /// 只包含区间内有记录的日期；数据库未配置或账号不存在时返回空列表。
    pub(crate) fn calendar(
        &self,
        user_id: u64,
        ruleset: Ruleset,
        start: &str,
        end: &str,
    ) -> CommandResult<CareerCalendar> {
        let Some(mut guard) = self.database()? else {
            return Ok(CareerCalendar {
                ruleset,
                start_date: start.into(),
                end_date: end.into(),
                days: Vec::new(),
            });
        };
        let db = guard.as_mut().unwrap();
        let account_id: Option<i64> = db
            .connection
            .query_row(
                "SELECT id FROM career_accounts WHERE user_id=?1 AND ruleset=?2",
                params![user_id as i64, ruleset.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(account_id) = account_id else {
            return Ok(CareerCalendar {
                ruleset,
                start_date: start.into(),
                end_date: end.into(),
                days: Vec::new(),
            });
        };
        let mut stmt = db.connection.prepare("SELECT d.local_date,d.status,d.error,s.captured_at,s.stats_json,d.snapshot_id FROM career_days d LEFT JOIN career_snapshots s ON s.id=d.snapshot_id WHERE d.account_id=?1 AND d.local_date BETWEEN ?2 AND ?3 ORDER BY d.local_date").map_err(sql_error)?;
        let mut rows = stmt
            .query(params![account_id, start, end])
            .map_err(sql_error)?;
        let mut days = Vec::new();
        // 先缓冲全部行再逐日计算差异，避免持有结果集的同时复用连接。
        let mut raw = Vec::new();
        while let Some(row) = rows.next().map_err(sql_error)? {
            let stats_json: Option<String> = row.get(4).map_err(sql_error)?;
            let stats = stats_json.and_then(|v| serde_json::from_str(&v).ok());
            let date: String = row.get(0).map_err(sql_error)?;
            let snapshot_id: Option<i64> = row.get(5).map_err(sql_error)?;
            raw.push((
                date,
                row.get(1).map_err(sql_error)?,
                row.get(3).map_err(sql_error)?,
                stats,
                row.get(2).map_err(sql_error)?,
                snapshot_id,
            ));
        }
        drop(rows);
        drop(stmt);
        for (date, status, captured_at, stats, error, snapshot_id) in raw {
            let counts = snapshot_id
                .map(|id| self.diff_counts(&mut db.connection, account_id, id))
                .transpose()?
                .unwrap_or_default();
            days.push(CareerCalendarDay {
                date,
                status,
                captured_at,
                stats,
                error,
                has_diff: counts.0 + counts.1 + counts.3 + counts.4 + counts.5 > 0,
                added_scores: counts.0,
                removed_scores: counts.1,
                changed_scores: counts.2,
                added_medals: counts.3,
                added_replays: counts.4,
                added_screenshots: counts.5,
            });
        }
        Ok(CareerCalendar {
            ruleset,
            start_date: start.into(),
            end_date: end.into(),
            days,
        })
    }

    /// 计算快照相对前一快照的差异计数。
    ///
    /// 返回顺序为 (新增成绩, 移除成绩, 变动成绩, 新奖牌, 新回放, 新截图)，
    /// 其中“变动成绩”当前恒为 0；无前序快照时全部为 0。
    fn diff_counts(
        &self,
        connection: &mut Connection,
        account_id: i64,
        snapshot_id: i64,
    ) -> CommandResult<(u32, u32, u32, u32, u32, u32)> {
        // 以快照 ID 更小的最近一条作为对比基准（ID 随写入递增）。
        let previous: Option<i64> = connection.query_row("SELECT id FROM career_snapshots WHERE account_id=?1 AND id<?2 ORDER BY id DESC LIMIT 1", params![account_id, snapshot_id], |r| r.get(0)).optional().map_err(sql_error)?;
        let Some(previous) = previous else {
            return Ok((0, 0, 0, 0, 0, 0));
        };
        let current_scores = score_map(connection, snapshot_id)?;
        let previous_scores = score_map(connection, previous)?;
        let added = current_scores
            .keys()
            .filter(|k| !previous_scores.contains_key(*k))
            .count() as u32;
        let removed = previous_scores
            .keys()
            .filter(|k| !current_scores.contains_key(*k))
            .count() as u32;
        let current_medals = value_keys(connection, "career_medals", snapshot_id)?;
        let previous_medals = value_keys(connection, "career_medals", previous)?;
        let medals = current_medals.difference(&previous_medals).count() as u32;
        let current_media = value_keys(connection, "career_media", snapshot_id)?;
        let previous_media = value_keys(connection, "career_media", previous)?;
        // 按指纹中的类别前缀区分新回放与新截图。
        let replays = current_media
            .iter()
            .filter(|k| k.starts_with("replay:") && !previous_media.contains(*k))
            .count() as u32;
        let screenshots = current_media
            .iter()
            .filter(|k| k.starts_with("screenshot:") && !previous_media.contains(*k))
            .count() as u32;
        Ok((added, removed, 0, medals, replays, screenshots))
    }

    /// 查询单日明细：当天与前一快照的统计对比、成绩差异及奖牌/媒体事件。
    ///
    /// 无日历行或未配置数据库时返回 `missing` 占位。
    pub(crate) fn day(
        &self,
        user_id: u64,
        ruleset: Ruleset,
        date: &str,
    ) -> CommandResult<CareerDayDetail> {
        let Some(mut guard) = self.database()? else {
            return Ok(empty_day(ruleset, date));
        };
        let db = guard.as_mut().unwrap();
        let row: Option<CareerDayRow> = db.connection.query_row("SELECT d.status,d.error,s.captured_at,s.stats_json,d.snapshot_id FROM career_days d LEFT JOIN career_snapshots s ON s.id=d.snapshot_id JOIN career_accounts a ON a.id=d.account_id WHERE a.user_id=?1 AND a.ruleset=?2 AND d.local_date=?3", params![user_id as i64, ruleset.to_string(), date], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(sql_error)?;
        let Some((status, error, captured_at, stats_json, snapshot_id)) = row else {
            return Ok(empty_day(ruleset, date));
        };
        let Some(snapshot_id) = snapshot_id else {
            return Ok(CareerDayDetail {
                ruleset,
                date: date.into(),
                status,
                captured_at,
                stats: None,
                previous_stats: None,
                error,
                score_diffs: Vec::new(),
                medal_events: Vec::new(),
                media_events: Vec::new(),
            });
        };
        let account_id: i64 = db
            .connection
            .query_row(
                "SELECT id FROM career_accounts WHERE user_id=?1 AND ruleset=?2",
                params![user_id as i64, ruleset.to_string()],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        let previous: Option<i64> = db.connection.query_row("SELECT id FROM career_snapshots WHERE account_id=?1 AND id<?2 ORDER BY id DESC LIMIT 1", params![account_id,snapshot_id], |r| r.get(0)).optional().map_err(sql_error)?;
        let mut score_diffs = Vec::new();
        let mut medal_events = Vec::new();
        let mut media_events = Vec::new();
        if let Some(previous) = previous {
            let current = score_map(&db.connection, snapshot_id)?;
            let old = score_map(&db.connection, previous)?;
            score_diffs = score_diffs_from_maps(&current, &old);
            medal_events = event_diff(
                &db.connection,
                "career_medals",
                previous,
                snapshot_id,
                "added",
            )?;
            media_events = media_diff(&db.connection, previous, snapshot_id)?;
        }
        Ok(CareerDayDetail {
            ruleset,
            date: date.into(),
            status,
            captured_at,
            stats: stats_json.and_then(|v| serde_json::from_str(&v).ok()),
            previous_stats: previous
                .and_then(|id| {
                    db.connection
                        .query_row(
                            "SELECT stats_json FROM career_snapshots WHERE id=?1",
                            [id],
                            |r| r.get::<_, String>(0),
                        )
                        .ok()
                })
                .and_then(|v| serde_json::from_str(&v).ok()),
            error,
            score_diffs,
            medal_events,
            media_events,
        })
    }

    /// 清理历史：给定日期时仅删除该日期之前的快照与日历行，否则全清。
    pub(crate) fn clear(&self, before: Option<&str>) -> CommandResult<()> {
        let Some(mut guard) = self.database()? else {
            return Ok(());
        };
        let db = guard.as_mut().unwrap();
        if let Some(before) = before {
            db.connection
                .execute("DELETE FROM career_snapshots WHERE local_date<?1", [before])
                .map_err(sql_error)?;
            db.connection
                .execute("DELETE FROM career_days WHERE local_date<?1", [before])
                .map_err(sql_error)?;
        } else {
            db.connection
                .execute_batch("DELETE FROM career_snapshots; DELETE FROM career_days;")
                .map_err(sql_error)?;
        }
        Ok(())
    }
}

/// 本地扫描到的一条媒体文件（回放或截图）。
#[derive(Debug, Clone)]
pub(crate) struct MediaRecord {
    /// 内容指纹：“客户端:类别:相对路径:大小:修改时间”。
    pub key: String,
    /// 文件类别：`replay` 或 `screenshot`。
    pub kind: String,
    /// 来源客户端：`stable` 或 `lazer`。
    pub client: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified_at: Option<String>,
}

/// 扫描 Stable 与 Lazer 客户端的回放/截图目录。
///
/// 返回 (媒体记录, 是否完全成功)；任一目录枚举失败只把第二个值置为
/// `false`（对应快照的媒体分区不完整），不中断整体扫描。
pub(crate) fn scan_media(state: &crate::state::AppState) -> (Vec<MediaRecord>, bool) {
    let mut result = Vec::new();
    let mut ok = true;
    for client in [LocalClient::Stable, LocalClient::Lazer] {
        let roots = match media_roots(state, client) {
            Ok(v) => v,
            Err(_) => {
                ok = false;
                continue;
            }
        };
        for root in roots {
            let entries = walkdir::WalkDir::new(&root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file());
            for entry in entries {
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                // 只收录回放（.osr）与截图（png/jpg/jpeg/webp）。
                let kind = match ext.as_str() {
                    "osr" => "replay",
                    "png" | "jpg" | "jpeg" | "webp" => "screenshot",
                    _ => continue,
                };
                let meta = match entry.metadata() {
                    Ok(v) => v,
                    Err(_) => {
                        ok = false;
                        continue;
                    }
                };
                let modified = meta
                    .modified()
                    .ok()
                    .map(|v| DateTime::<Utc>::from(v).to_rfc3339());
                // 指纹含相对路径、大小与修改时间：位置或内容变化即视为新文件。
                let key = format!(
                    "{client}:{kind}:{}:{}:{}",
                    path.strip_prefix(&root)
                        .unwrap_or(path)
                        .display()
                        .to_string()
                        .to_ascii_lowercase(),
                    meta.len(),
                    modified.clone().unwrap_or_default()
                );
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(path)
                    .display()
                    .to_string();
                result.push(MediaRecord {
                    key,
                    kind: kind.into(),
                    client: client.to_string(),
                    name: path
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("")
                        .into(),
                    path: relative,
                    size: meta.len(),
                    modified_at: modified,
                });
            }
        }
    }
    (result, ok)
}

/// 从 osu! API 的 `statistics` JSON 抽取数值统计；缺失字段保持 `None`。
fn extract_stats(value: &Value) -> CareerStats {
    let n = |k: &str| value.get(k).and_then(Value::as_f64);
    let level = value.get("level");
    CareerStats {
        pp: n("pp"),
        global_rank: n("global_rank"),
        country_rank: n("country_rank"),
        ranked_score: n("ranked_score"),
        total_score: n("total_score"),
        hit_accuracy: n("hit_accuracy"),
        play_count: n("play_count"),
        play_time: n("play_time"),
        total_hits: n("total_hits"),
        maximum_combo: n("maximum_combo"),
        level: level.and_then(|v| v.get("current")).and_then(Value::as_f64),
        level_progress: level
            .and_then(|v| v.get("progress"))
            .and_then(Value::as_f64),
    }
}
/// 批量写入 Top 成绩；`position` 记录榜内名次（从 1 开始）。
fn insert_scores(tx: &rusqlite::Transaction<'_>, id: i64, scores: &[Score]) -> CommandResult<()> {
    for (index, score) in scores.iter().enumerate() {
        let value = serde_json::to_string(score)
            .map_err(|e| CommandError::new("CAREER_SERIALIZE_FAILED", e.to_string()))?;
        tx.execute("INSERT INTO career_scores(snapshot_id,score_key,position,score_json) VALUES(?1,?2,?3,?4)",params![id,score_key(score),index as i64+1,value]).map_err(sql_error)?;
    }
    Ok(())
}
/// 批量写入奖牌；键优先取 `achievement_id`（兼容 `id` 字段，缺省记 0）。
fn insert_medals(tx: &rusqlite::Transaction<'_>, id: i64, medals: &[Value]) -> CommandResult<()> {
    for medal in medals {
        let key = medal
            .get("achievement_id")
            .or_else(|| medal.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let value = serde_json::to_string(medal).unwrap_or_else(|_| "{}".into());
        tx.execute("INSERT OR REPLACE INTO career_medals(snapshot_id,achievement_key,payload_json) VALUES(?1,?2,?3)",params![id,key,value]).map_err(sql_error)?;
    }
    Ok(())
}
/// 批量写入媒体指纹记录。
fn insert_media(
    tx: &rusqlite::Transaction<'_>,
    id: i64,
    media: &[MediaRecord],
) -> CommandResult<()> {
    for item in media {
        tx.execute("INSERT INTO career_media(snapshot_id,fingerprint,kind,client,name,path,size,modified_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,item.key,item.kind,item.client,item.name,item.path,item.size as i64,item.modified_at]).map_err(sql_error)?;
    }
    Ok(())
}
/// 生成成绩去重键：优先服务器成绩 ID（`id:<id>`），
/// 否则用“谱面 ID + Mod + 结束（或创建）时间”兜底。
fn score_key(score: &Score) -> String {
    if let Some(id) = score.id {
        return format!("id:{id}");
    }
    let mods = serde_json::to_string(&score.mods).unwrap_or_default();
    format!(
        "map:{}:{}:{}",
        score
            .beatmap
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        mods,
        score
            .ended_at
            .clone()
            .or(score.created_at.clone())
            .unwrap_or_default()
    )
}
/// 读取快照全部成绩，映射为 `去重键 -> (榜内名次, 原始 JSON)`。
fn score_map(c: &Connection, id: i64) -> CommandResult<HashMap<String, (u32, Value)>> {
    let mut s = c
        .prepare("SELECT score_key,position,score_json FROM career_scores WHERE snapshot_id=?1")
        .map_err(sql_error)?;
    let mut rows = s.query([id]).map_err(sql_error)?;
    let mut out = HashMap::new();
    while let Some(r) = rows.next().map_err(sql_error)? {
        let key: String = r.get(0).map_err(sql_error)?;
        let pos: i64 = r.get(1).map_err(sql_error)?;
        let val: String = r.get(2).map_err(sql_error)?;
        out.insert(
            key,
            (
                pos as u32,
                serde_json::from_str(&val).unwrap_or(Value::Null),
            ),
        );
    }
    Ok(out)
}

/// 对比两份成绩映射，生成进入/离开 Top 榜的差异；
/// 名次或成绩内容的变化不视为差异。
fn score_diffs_from_maps(
    current: &HashMap<String, (u32, Value)>,
    previous: &HashMap<String, (u32, Value)>,
) -> Vec<CareerScoreDiff> {
    let mut diffs = Vec::new();
    for (key, (position, score)) in current {
        if !previous.contains_key(key) {
            diffs.push(CareerScoreDiff {
                kind: "added".into(),
                key: key.clone(),
                before_position: None,
                after_position: Some(*position),
                score: score.clone(),
            });
        }
    }
    for (key, (position, score)) in previous {
        if !current.contains_key(key) {
            diffs.push(CareerScoreDiff {
                kind: "removed".into(),
                key: key.clone(),
                before_position: Some(*position),
                after_position: None,
                score: score.clone(),
            });
        }
    }
    // 按变化后（或变化前）的名次排序，便于前端按榜单顺序展示。
    diffs.sort_by_key(|diff| {
        (
            diff.after_position
                .or(diff.before_position)
                .unwrap_or(u32::MAX),
            diff.kind.clone(),
            diff.key.clone(),
        )
    });
    diffs
}

/// 读取指定快照在奖牌表/媒体表中的键集合，供集合差集运算。
fn value_keys(c: &Connection, table: &str, id: i64) -> CommandResult<HashSet<String>> {
    let sql = if table == "career_medals" {
        "SELECT achievement_key FROM career_medals WHERE snapshot_id=?1"
    } else {
        "SELECT fingerprint FROM career_media WHERE snapshot_id=?1"
    };
    let mut s = c.prepare(sql).map_err(sql_error)?;
    let mut rows = s.query([id]).map_err(sql_error)?;
    let mut out = HashSet::new();
    while let Some(r) = rows.next().map_err(sql_error)? {
        out.insert(r.get::<_, String>(0).map_err(sql_error)?);
    }
    Ok(out)
}
/// 查询当前快照相对旧快照新增的奖牌；目前仅实现 `career_medals` 表。
fn event_diff(
    c: &Connection,
    table: &str,
    old: i64,
    current: i64,
    kind: &str,
) -> CommandResult<Vec<CareerEventItem>> {
    let sql = if table == "career_medals" {
        "SELECT achievement_key,payload_json FROM career_medals WHERE snapshot_id=?1 AND achievement_key NOT IN (SELECT achievement_key FROM career_medals WHERE snapshot_id=?2)"
    } else {
        ""
    };
    if sql.is_empty() {
        return Ok(Vec::new());
    }
    let mut s = c.prepare(sql).map_err(sql_error)?;
    let mut rows = s.query(params![current, old]).map_err(sql_error)?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().map_err(sql_error)? {
        let name: String = r.get(0).map_err(sql_error)?;
        let payload: String = r.get(1).map_err(sql_error)?;
        out.push(CareerEventItem {
            kind: kind.into(),
            client: None,
            name,
            path: None,
            size: None,
            modified_at: None,
            payload: serde_json::from_str(&payload).ok(),
        });
    }
    Ok(out)
}
/// 查询当前快照相对旧快照新增的媒体文件（按指纹差集判断）。
fn media_diff(c: &Connection, old: i64, current: i64) -> CommandResult<Vec<CareerEventItem>> {
    let mut s=c.prepare("SELECT kind,client,name,path,size,modified_at,fingerprint FROM career_media WHERE snapshot_id=?1 AND fingerprint NOT IN (SELECT fingerprint FROM career_media WHERE snapshot_id=?2)").map_err(sql_error)?;
    let mut rows = s.query(params![current, old]).map_err(sql_error)?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().map_err(sql_error)? {
        out.push(CareerEventItem {
            kind: "added".into(),
            client: Some(r.get(1).map_err(sql_error)?),
            name: r.get(2).map_err(sql_error)?,
            path: Some(r.get(3).map_err(sql_error)?),
            size: r.get::<_, i64>(4).map_err(sql_error)?.try_into().ok(),
            modified_at: r.get(5).map_err(sql_error)?,
            payload: Some(json!({"kind":r.get::<_,String>(0).map_err(sql_error)?})),
        });
    }
    Ok(out)
}
/// 把 UTC 时间换算为东八区日期（`YYYY-MM-DD`），作为快照归属日。
fn local_date(now: DateTime<Utc>) -> String {
    now.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
        .date_naive()
        .to_string()
}

/// 当前时刻的东八区日期字符串。
pub(crate) fn current_local_date() -> String {
    local_date(Utc::now())
}
/// 构造无记录日的 `missing` 占位明细。
fn empty_day(ruleset: Ruleset, date: &str) -> CareerDayDetail {
    CareerDayDetail {
        ruleset,
        date: date.into(),
        status: "missing".into(),
        captured_at: None,
        stats: None,
        previous_stats: None,
        error: None,
        score_diffs: Vec::new(),
        medal_events: Vec::new(),
        media_events: Vec::new(),
    }
}
fn sql_error(error: rusqlite::Error) -> CommandError {
    CommandError::new("CAREER_DATABASE_ERROR", error.to_string())
}

/// `career.sqlite3` 建表语句：账号、日历、快照、成绩、奖牌、媒体六张表，
/// 均带外键级联删除；快照按“账号+日期”唯一。
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS career_accounts(id INTEGER PRIMARY KEY,user_id INTEGER NOT NULL,ruleset TEXT NOT NULL,username TEXT NOT NULL,timezone TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,UNIQUE(user_id,ruleset));
CREATE TABLE IF NOT EXISTS career_days(account_id INTEGER NOT NULL REFERENCES career_accounts(id) ON DELETE CASCADE,local_date TEXT NOT NULL,status TEXT NOT NULL,attempted_at TEXT NOT NULL,error TEXT,snapshot_id INTEGER REFERENCES career_snapshots(id) ON DELETE SET NULL,PRIMARY KEY(account_id,local_date));
CREATE TABLE IF NOT EXISTS career_snapshots(id INTEGER PRIMARY KEY,account_id INTEGER NOT NULL REFERENCES career_accounts(id) ON DELETE CASCADE,ruleset TEXT NOT NULL,local_date TEXT NOT NULL,captured_at TEXT NOT NULL,timezone TEXT NOT NULL,profile_json TEXT NOT NULL,stats_json TEXT NOT NULL,sections_json TEXT NOT NULL,UNIQUE(account_id,local_date));
CREATE TABLE IF NOT EXISTS career_scores(snapshot_id INTEGER NOT NULL REFERENCES career_snapshots(id) ON DELETE CASCADE,score_key TEXT NOT NULL,position INTEGER NOT NULL,score_json TEXT NOT NULL,PRIMARY KEY(snapshot_id,score_key));
CREATE TABLE IF NOT EXISTS career_medals(snapshot_id INTEGER NOT NULL REFERENCES career_snapshots(id) ON DELETE CASCADE,achievement_key TEXT NOT NULL,payload_json TEXT NOT NULL,PRIMARY KEY(snapshot_id,achievement_key));
CREATE TABLE IF NOT EXISTS career_media(snapshot_id INTEGER NOT NULL REFERENCES career_snapshots(id) ON DELETE CASCADE,fingerprint TEXT NOT NULL,kind TEXT NOT NULL,client TEXT NOT NULL,name TEXT NOT NULL,path TEXT NOT NULL,size INTEGER NOT NULL,modified_at TEXT,PRIMARY KEY(snapshot_id,fingerprint));
CREATE INDEX IF NOT EXISTS idx_career_snapshots_account_date ON career_snapshots(account_id,local_date);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // 统计抽取：可选字段缺失时保持 None 而不是报错。
    #[test]
    fn extracts_requested_statistics_without_requiring_optional_fields() {
        let stats = extract_stats(&json!({
            "pp": 1234.5,
            "global_rank": 42,
            "hit_accuracy": 98.76,
            "level": { "current": 99, "progress": 12 }
        }));
        assert_eq!(stats.pp, Some(1234.5));
        assert_eq!(stats.global_rank, Some(42.0));
        assert_eq!(stats.level, Some(99.0));
        assert_eq!(stats.total_hits, None);
    }

    // 去重键优先服务器成绩 ID，无 ID 时退回内容键。
    #[test]
    fn score_key_prefers_server_score_id_and_falls_back_to_content() {
        let with_id: Score = serde_json::from_value(json!({
            "id": 77,
            "user_id": 1,
            "accuracy": 0.99,
            "rank": "S",
            "mods": [],
            "statistics": {}
        }))
        .unwrap();
        assert_eq!(score_key(&with_id), "id:77");

        let without_id: Score = serde_json::from_value(json!({
            "user_id": 1,
            "accuracy": 0.99,
            "rank": "S",
            "ended_at": "2026-09-27T00:00:00Z",
            "mods": ["HD"],
            "statistics": {},
            "beatmap": { "id": 123 }
        }))
        .unwrap();
        assert!(score_key(&without_id).starts_with("map:123:"));
    }

    // 已在榜成绩的名次或内容变化不产生差异。
    #[test]
    fn score_diff_ignores_position_and_payload_changes_for_existing_scores() {
        let previous = HashMap::from([
            ("id:1".into(), (1, json!({"pp": 100}))),
            ("id:2".into(), (2, json!({"pp": 90}))),
        ]);
        let current = HashMap::from([
            ("id:2".into(), (1, json!({"pp": 120}))),
            ("id:1".into(), (2, json!({"pp": 80}))),
        ]);

        assert!(score_diffs_from_maps(&current, &previous).is_empty());
    }

    // 差异只报告进出 Top 榜的成绩。
    #[test]
    fn score_diff_reports_only_scores_entering_or_leaving_top_200() {
        let previous = HashMap::from([("id:1".into(), (1, json!({"pp": 100})))]);
        let current = HashMap::from([("id:2".into(), (1, json!({"pp": 110})))]);

        let diffs = score_diffs_from_maps(&current, &previous);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0].kind, "added");
        assert_eq!(diffs[0].key, "id:2");
        assert_eq!(diffs[1].kind, "removed");
        assert_eq!(diffs[1].key, "id:1");
    }

    #[test]
    fn score_diff_is_empty_for_an_unchanged_snapshot() {
        let current = HashMap::from([("id:1".into(), (1, json!({"pp": 100})))]);

        assert!(score_diffs_from_maps(&current, &current).is_empty());
    }

    // 建表语句需能在外键开启时初始化出全部六张表。
    #[test]
    fn career_schema_can_initialize_with_foreign_keys_enabled() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'career_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 6);
    }
}
