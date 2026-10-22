//! Read-only local score catalog. Game databases are never opened for writing.
// 支持 stable 的 `scores.db` 与 lazer 的 `client.realm`，按文件指纹缓存并增量刷新。
mod stable;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::{LocalAnalysisService, LocalClient, lazer_realm},
    infrastructure::logging::{finish_span, global},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use tauri::{AppHandle, Manager};

/// 一条本地成绩（stable 与 lazer 统一后的模型）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalScore {
    /// 来源内稳定唯一 ID（stable 形如 `stable:<内容 sha256 十六进制>`）。
    pub id: String,
    /// 来源标识：`stable` 或 `lazer`。
    pub source: String,
    pub player: String,
    /// 规则集：`osu`/`taiko`/`fruits`/`mania` 之一。
    pub ruleset: String,
    /// 计分制：`stable` 经典计分或 `score_v2`。
    pub scoring: String,
    /// 谱面 .osu 文件的 MD5，用于和本地谱库匹配。
    pub beatmap_hash: String,
    pub score: u64,
    /// 由判定数折算的 0..=1 准确率；缺少判定数据时为 `None`。
    pub accuracy: Option<f64>,
    pub combo: Option<u32>,
    /// 空格分隔的模组缩写。
    pub mods: String,
    /// RFC 3339 格式的游玩时间。
    pub played_at: Option<String>,
    /// 预留的备注字段。
    #[serde(default)]
    pub note: String,
}

/// 单个客户端的成绩缓存。
#[derive(Default)]
struct Catalog {
    /// 数据库指纹 `(路径, 大小, 修改时间)`；与上次一致则跳过重读。
    stamp: Option<(PathBuf, u64, SystemTime)>,
    /// 按 ID 排序去重后的成绩列表。
    scores: Vec<LocalScore>,
    /// 小写谱面哈希 → `scores` 下标列表，供按谱面查询。
    by_hash: HashMap<String, Vec<usize>>,
    /// 最近一次读取失败的错误；存在错误时下次刷新强制重读。
    error: Option<String>,
}

/// 本地成绩服务：按客户端维护缓存目录；刷新全程持锁串行执行。
#[derive(Default)]
pub struct LocalScoreService {
    catalogs: Mutex<HashMap<LocalClient, Catalog>>,
    // 串行化并发刷新，避免两个线程同时整读数据库。
    refresh_lock: Mutex<()>,
}

/// 刷新结果：出现的玩家名、默认玩家（与登录用户名忽略大小写匹配）、
/// 各来源错误与成绩总数。
#[derive(Serialize)]
pub struct ScoreStatus {
    pub players: Vec<String>,
    pub default_player: Option<String>,
    pub errors: Vec<String>,
    pub count: usize,
}

/// 读取数据库指纹（路径、大小、修改时间）。
fn stamp(path: &Path) -> CommandResult<(PathBuf, u64, SystemTime)> {
    let metadata = fs::metadata(path)?;
    Ok((path.to_path_buf(), metadata.len(), metadata.modified()?))
}

impl LocalScoreService {
    /// 读取单个客户端的成绩数据库并更新缓存。
    ///
    /// 指纹未变时跳过重读（`force` 可强制）；读取前后各取一次指纹，
    /// 不一致说明游戏正在写入，本次放弃以免读到半截数据；stable 走二进制
    /// 解析、lazer 走 Realm 读取器；文件超过 2GB 直接拒绝。
    fn refresh_path(&self, client: LocalClient, path: &Path, force: bool) -> CommandResult<()> {
        let span = global().map(|log| log.operation("local_scores", "read_database"));
        let unchanged = {
            let mut catalogs = self
                .catalogs
                .lock()
                .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "成绩状态不可用"))?;
            let catalog = catalogs.entry(client).or_default();
            // 数据库换了路径：旧缓存整体作废。
            if catalog.stamp.as_ref().is_some_and(|old| old.0 != path) {
                *catalog = Catalog::default();
            }
            if force || catalog.error.is_some() {
                None
            } else {
                catalog.stamp.clone()
            }
        };
        let read = (|| {
            let before = stamp(path)?;
            if unchanged.as_ref() == Some(&before) {
                return Ok(None);
            }
            if before.1 > 2 * 1024 * 1024 * 1024 {
                return Err(CommandError::new(
                    "SCORE_DATABASE_TOO_LARGE",
                    "成绩数据库超过读取上限",
                ));
            }
            let scores = if client == LocalClient::Stable {
                stable::parse(&fs::read(path)?)?
            } else {
                lazer_realm::read_realm_scores(path)
                    .map_err(|error| CommandError::new("LAZER_SCORES_READ_FAILED", error))?
            };
            if stamp(path)? != before {
                return Err(CommandError::new(
                    "SCORE_DATABASE_CHANGED",
                    "游戏正在更新成绩，请稍后刷新",
                ));
            }
            Ok(Some((before, scores)))
        })();
        if let Some(log) = &span {
            log.fs_op("read", path, &read);
        }
        let mut catalogs = self
            .catalogs
            .lock()
            .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "成绩状态不可用"))?;
        let catalog = catalogs.entry(client).or_default();
        let result = match read {
            Ok(Some((stamp, mut scores))) => {
                // 按 ID 排序去重，并建立谱面哈希索引。
                scores.sort_by(|a, b| a.id.cmp(&b.id));
                scores.dedup_by(|a, b| a.id == b.id);
                let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
                for (index, score) in scores.iter().enumerate() {
                    by_hash
                        .entry(score.beatmap_hash.to_lowercase())
                        .or_default()
                        .push(index);
                }
                *catalog = Catalog {
                    stamp: Some(stamp),
                    scores,
                    by_hash,
                    error: None,
                };
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => {
                catalog.error = Some(format!("{client:?}: {}", error.message));
                Err(error)
            }
        };
        finish_span(span, result)
    }
    /// 刷新 stable 与 lazer 两个来源并汇总状态；无效来源移除其缓存。
    pub fn refresh(
        &self,
        local: &LocalAnalysisService,
        force: bool,
        username: Option<String>,
    ) -> CommandResult<ScoreStatus> {
        let span = global().map(|log| log.operation("local_scores", "refresh"));
        let result = (|| {
            let _refresh = self
                .refresh_lock
                .lock()
                .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "成绩读取状态不可用"))?;
            for client in [LocalClient::Stable, LocalClient::Lazer] {
                let source = local.source_status(client)?;
                let root = if client == LocalClient::Stable {
                    source.install_root
                } else {
                    source.data_root
                };
                let Some(root) = root.filter(|_| source.valid) else {
                    self.catalogs
                        .lock()
                        .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "成绩状态不可用"))?
                        .remove(&client);
                    continue;
                };
                let path = Path::new(&root).join(if client == LocalClient::Stable {
                    "scores.db"
                } else {
                    "client.realm"
                });
                // 单个来源失败只记录错误，不影响其它来源与已有缓存。
                // Per-source failures remain visible while other clients and cached results work.
                let _ = self.refresh_path(client, &path, force);
            }
            let catalogs = self
                .catalogs
                .lock()
                .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "成绩状态不可用"))?;
            let mut players: Vec<_> = catalogs
                .values()
                .flat_map(|c| c.scores.iter().map(|s| s.player.clone()))
                .filter(|name| !name.is_empty())
                .collect();
            players.sort();
            players.dedup();
            // 登录用户名与库中玩家名忽略大小写匹配，命中者作为默认玩家。
            let default_player = username.and_then(|name| {
                players
                    .iter()
                    .find(|p| p.eq_ignore_ascii_case(&name))
                    .cloned()
            });
            Ok(ScoreStatus {
                players,
                default_player,
                errors: catalogs.values().filter_map(|c| c.error.clone()).collect(),
                count: catalogs.values().map(|c| c.scores.len()).sum(),
            })
        })();
        finish_span(span, result)
    }

    /// 按谱面哈希集合（忽略大小写）与可选玩家名过滤成绩，按游玩时间倒序返回。
    pub fn matching(
        &self,
        hashes: &[String],
        player: Option<&str>,
    ) -> CommandResult<Vec<LocalScore>> {
        let catalogs = self
            .catalogs
            .lock()
            .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "数据状态不可用"))?;
        let hashes: std::collections::HashSet<_> = hashes
            .iter()
            .filter(|h| !h.is_empty())
            .map(|h| h.to_lowercase())
            .collect();
        let mut scores: Vec<_> = catalogs
            .values()
            .flat_map(|c| {
                hashes
                    .iter()
                    .filter_map(|h| c.by_hash.get(h))
                    .flatten()
                    .map(|i| &c.scores[*i])
            })
            .filter(|s| player.is_none_or(|p| s.player.eq_ignore_ascii_case(p)))
            .cloned()
            .collect();
        scores.sort_by(|a, b| b.played_at.cmp(&a.played_at).then(a.id.cmp(&b.id)));
        Ok(scores)
    }
}

/// 刷新本地成绩库；在阻塞 IO 线程执行，并用登录用户名挑选默认玩家。
#[tauri::command]
pub async fn refresh_local_scores(app: AppHandle, force: bool) -> CommandResult<ScoreStatus> {
    crate::infrastructure::tasks::blocking_io("refresh_local_scores", move || {
        let state = app.state::<AppState>();
        state.local_scores.refresh(
            &state.local_analysis,
            force,
            state.store.read(|s| s.username.clone())?,
        )
    })
    .await?
}

#[cfg(test)]
mod tests {
    // 覆盖只读刷新、大小写不敏感匹配与读取失败时保留旧缓存。
    use super::*;
    // 重复刷新不重读也不写库；半截数据库使刷新失败但旧缓存仍可查询。
    #[test]
    fn read_only_refresh_matching_and_failed_refresh_preserve_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scores.db");
        let bytes = stable::fixture();
        fs::write(&path, &bytes).unwrap();
        let service = LocalScoreService::default();
        service
            .refresh_path(LocalClient::Stable, &path, false)
            .unwrap();
        service
            .refresh_path(LocalClient::Stable, &path, false)
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let scores = service
            .matching(&["EXACT-MD5".into(), "exact-md5".into()], Some("player"))
            .unwrap();
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].player, "Player");
        assert!(
            service
                .matching(&["different-version".into()], None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            service.matching(&["exact-md5".into()], None).unwrap().len(),
            2
        );
        fs::write(&path, b"partially written").unwrap();
        assert!(
            service
                .refresh_path(LocalClient::Stable, &path, true)
                .is_err()
        );
        assert_eq!(
            service.matching(&["exact-md5".into()], None).unwrap().len(),
            2
        );
        assert_eq!(fs::read(&path).unwrap(), b"partially written");
    }
}
