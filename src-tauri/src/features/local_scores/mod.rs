//! Read-only local score catalog. Game databases are never opened for writing.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalScore {
    pub id: String,
    pub source: String,
    pub player: String,
    pub ruleset: String,
    pub scoring: String,
    pub beatmap_hash: String,
    pub score: u64,
    pub accuracy: Option<f64>,
    pub combo: Option<u32>,
    pub mods: String,
    pub played_at: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Default)]
struct Catalog {
    stamp: Option<(PathBuf, u64, SystemTime)>,
    scores: Vec<LocalScore>,
    by_hash: HashMap<String, Vec<usize>>,
    error: Option<String>,
}

#[derive(Default)]
pub struct LocalScoreService {
    catalogs: Mutex<HashMap<LocalClient, Catalog>>,
    refresh_lock: Mutex<()>,
}

#[derive(Serialize)]
pub struct ScoreStatus {
    pub players: Vec<String>,
    pub default_player: Option<String>,
    pub errors: Vec<String>,
    pub count: usize,
}

fn stamp(path: &Path) -> CommandResult<(PathBuf, u64, SystemTime)> {
    let metadata = fs::metadata(path)?;
    Ok((path.to_path_buf(), metadata.len(), metadata.modified()?))
}

impl LocalScoreService {
    fn refresh_path(&self, client: LocalClient, path: &Path, force: bool) -> CommandResult<()> {
        let span = global().map(|log| log.operation("local_scores", "read_database"));
        let unchanged = {
            let mut catalogs = self
                .catalogs
                .lock()
                .map_err(|_| CommandError::new("STATE_LOCK_FAILED", "成绩状态不可用"))?;
            let catalog = catalogs.entry(client).or_default();
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
    use super::*;
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
