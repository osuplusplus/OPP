//! Compatibility adapter between the analysis index and row-based SQLite storage.
use super::super::models::{LocalBeatmapPresence, LocalLibraryStorageStatus, LocalPresenceState};
use super::*;
use crate::features::local_database::LocalDatabaseService;
use crate::infrastructure::{local_database::library, logging::finish_span};

#[cfg(test)]
#[path = "service_database_tests.rs"]
mod tests;

pub(super) fn state_error() -> CommandError {
    CommandError::new("LOCAL_INDEX_STATE_ERROR", "本地索引状态已损坏")
}

impl LocalAnalysisService {
    pub(super) fn report_storage_error(&self, client: LocalClient, error: CommandError) {
        if let Some(log) = global() {
            let mut span = log.operation("local_analysis.storage", "restore_index");
            span.finish_error(&error);
        }
        self.report(client, None, "unscanned", Some(error.message));
    }
    pub(crate) fn attach_database(&self, database: Arc<LocalDatabaseService>) -> CommandResult<()> {
        *self.database.write().map_err(|_| state_error())? = Some(database);
        Ok(())
    }

    fn database(&self) -> CommandResult<Option<Arc<LocalDatabaseService>>> {
        Ok(self.database.read().map_err(|_| state_error())?.clone())
    }

    fn expected_revision(&self, client: LocalClient) -> CommandResult<Option<String>> {
        let span = global().map(|log| log.operation("local_analysis.storage", "read_revision"));
        let path = self.cache_dir.join(format!("{client}-index-revision.json"));
        let result = match fs::read_to_string(&path) {
            Ok(value) => Ok(Some(serde_json::from_str(&value)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A interrupted Windows replace may leave only the backup.
                match fs::read_to_string(path.with_extension("json.bak")) {
                    Ok(value) => Ok(Some(serde_json::from_str(&value)?)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(error.into()),
                }
            }
            Err(error) => Err(error.into()),
        };
        finish_span(span, result)
    }

    fn record_revision(&self, client: LocalClient, revision: &str) -> CommandResult<()> {
        let path = self.cache_dir.join(format!("{client}-index-revision.json"));
        let tmp = path.with_extension("json.tmp");
        let backup = path.with_extension("json.bak");
        let span = global().map(|log| log.operation("local_analysis.storage", "write_revision"));
        finish_span(
            span,
            (|| {
                use std::io::Write;
                let mut file = fs::File::create(&tmp)?;
                file.write_all(&serde_json::to_vec(revision)?)?;
                file.sync_all()?;
                drop(file);
                if path.exists() {
                    if backup.exists() {
                        fs::remove_file(&backup)?;
                    }
                    fs::rename(&path, &backup)?;
                }
                if let Err(error) = fs::rename(&tmp, &path) {
                    if backup.exists() {
                        let _ = fs::rename(&backup, &path);
                    }
                    return Err(error.into());
                }
                if backup.exists() {
                    let _ = fs::remove_file(backup);
                }
                Ok(())
            })(),
        )
    }

    fn report(
        &self,
        client: LocalClient,
        index: Option<&LocalIndex>,
        storage: &str,
        error: Option<String>,
    ) {
        if let Ok(mut reports) = self.storage_reports.write() {
            reports.insert(
                client,
                LocalLibraryStorageStatus {
                    client,
                    storage: storage.into(),
                    revision: index.map(|i| i.summary.scanned_at.clone()),
                    entry_count: index.map_or(0, |i| i.entries.len()),
                    beatmap_count: index.map_or(0, |i| i.summary.beatmap_count),
                    error,
                },
            );
        }
    }

    pub(super) fn restore_library(&self, client: LocalClient) -> CommandResult<Option<LocalIndex>> {
        let expected = self.expected_revision(client)?;
        let mut warning = None;
        if let Some(database) = self.database()? {
            let loaded = database.with_database("load_library_snapshot", |db| {
                let Some(snapshot) = library::load(&db.connection, &client.to_string())? else {
                    return Ok(None);
                };
                if snapshot.schema != INDEX_SCHEMA
                    || snapshot.algorithm != DIFFICULTY_ALGORITHM
                    || expected
                        .as_ref()
                        .is_some_and(|revision| revision != &snapshot.revision)
                {
                    return Err(CommandError::new(
                        "LOCAL_INDEX_INCOMPATIBLE",
                        "数据库索引版本不匹配，将尝试兼容的 JSON 索引；必要时请重新扫描",
                    ));
                }
                let summary: LocalLibrarySummary = serde_json::from_str(&snapshot.summary_json)?;
                let entries = snapshot
                    .entries
                    .into_iter()
                    .map(|entry| {
                        if sha256(entry.json.as_bytes()) != entry.hash {
                            return Err(CommandError::new(
                                "DATABASE_INDEX_INVALID",
                                "索引条目校验失败",
                            ));
                        }
                        let value: IndexedEntry = serde_json::from_str(&entry.json)?;
                        if value.key != entry.key {
                            return Err(CommandError::new(
                                "DATABASE_INDEX_INVALID",
                                "索引条目标识不匹配",
                            ));
                        }
                        Ok(value)
                    })
                    .collect::<CommandResult<Vec<_>>>()?;
                if summary.client != client
                    || summary.source_root != snapshot.root
                    || summary.scanned_at != snapshot.revision
                {
                    return Err(CommandError::new(
                        "DATABASE_INDEX_INVALID",
                        "索引来源或版本不匹配",
                    ));
                }
                let mut index = LocalIndex {
                    schema: snapshot.schema,
                    difficulty_algorithm: snapshot.algorithm,
                    source_root: snapshot.root,
                    summary,
                    diagnostics: serde_json::from_str(&snapshot.diagnostics_json)?,
                    entries,
                    search_fields: Vec::new(),
                    resource_lookup: BTreeMap::new(),
                    beatmap_id_lookup: BTreeMap::new(),
                    set_id_lookup: BTreeSet::new(),
                    beatmap_md5_lookup: BTreeMap::new(),
                    beatmap_sets: BTreeMap::new(),
                    beatmap_orders: BTreeMap::new(),
                    skin_orders: BTreeMap::new(),
                };
                index.rebuild_runtime_indexes();
                Ok(Some(index))
            });
            match loaded {
                Ok(Some(Some(index))) => {
                    self.report(client, Some(&index), "database", None);
                    return Ok(Some(index));
                }
                Err(error) => warning = Some(error.message),
                _ => {}
            }
        }
        let span = global().map(|log| log.operation("local_analysis.storage", "load_legacy_index"));
        let index = load_index(&self.cache_dir, client).filter(|index| {
            expected
                .as_ref()
                .is_none_or(|revision| revision == &index.summary.scanned_at)
        });
        finish_span(span, Ok(()))?;
        if let Some(index) = &index {
            // Migrate only compatible cached data. No source scan or resource reads.
            match self.save_database(client, index) {
                Ok(true) => {
                    self.record_revision(client, &index.summary.scanned_at)?;
                    self.report(client, Some(index), "database", None);
                }
                Ok(false) => self.report(client, Some(index), "json", warning),
                Err(error) => self.report(client, Some(index), "json", Some(error.message)),
            }
        } else {
            self.report(client, None, "unscanned", warning);
        }
        Ok(index)
    }

    fn save_database(&self, client: LocalClient, index: &LocalIndex) -> CommandResult<bool> {
        let Some(database) = self.database()? else {
            return Ok(false);
        };
        database
            .with_database("save_library_snapshot", |db| {
                let entries = index
                    .entries
                    .iter()
                    .map(|entry| {
                        let json = serde_json::to_string(entry)?;
                        let beatmap = if let IndexedData::Beatmap { summary, .. } = &entry.data {
                            Some(library::Beatmap {
                                physical_path: entry.physical_path.to_string_lossy().into(),
                                logical_path: summary.resource.logical_path.clone(),
                                bytes: i64::try_from(entry.stamp.bytes).map_err(|_| {
                                    CommandError::new(
                                        "INDEX_SIZE_OVERFLOW",
                                        "文件大小超出数据库范围",
                                    )
                                })?,
                                modified_ms: i64::try_from(entry.stamp.modified_ms).map_err(
                                    |_| {
                                        CommandError::new(
                                            "INDEX_TIME_OVERFLOW",
                                            "文件时间超出数据库范围",
                                        )
                                    },
                                )?,
                                content_hash: entry.content_hash.clone(),
                                md5: entry.beatmap_md5.clone(),
                                id: summary.beatmap_id,
                                set_id: summary.beatmap_set_id,
                                ruleset: summary.ruleset.to_string(),
                                title: summary.title.clone(),
                                title_unicode: summary.title_unicode.clone(),
                                artist: summary.artist.clone(),
                                artist_unicode: summary.artist_unicode.clone(),
                                creator: summary.creator.clone(),
                                difficulty: summary.difficulty_name.clone(),
                            })
                        } else {
                            None
                        };
                        Ok(library::Entry {
                            key: entry.key.clone(),
                            hash: sha256(json.as_bytes()),
                            json,
                            beatmap,
                        })
                    })
                    .collect::<CommandResult<Vec<_>>>()?;
                library::save(
                    &mut db.connection,
                    &library::Snapshot {
                        client: client.to_string(),
                        root: index.source_root.clone(),
                        schema: index.schema,
                        algorithm: index.difficulty_algorithm.clone(),
                        summary_json: serde_json::to_string(&index.summary)?,
                        diagnostics_json: serde_json::to_string(&index.diagnostics)?,
                        revision: index.summary.scanned_at.clone(),
                        completeness: match index.summary.completeness {
                            Completeness::Complete => "complete",
                            Completeness::Partial => "partial",
                        }
                        .into(),
                        entries,
                    },
                )
            })
            .map(|value| value.is_some())
    }

    pub(super) fn store_and_publish(
        &self,
        client: LocalClient,
        index: LocalIndex,
    ) -> CommandResult<()> {
        let _guard = self.persistence.lock().map_err(|_| state_error())?;
        // The small intent file prevents an older DB snapshot winning after JSON fallback.
        self.record_revision(client, &index.summary.scanned_at)?;
        let saved = self.save_database(client, &index);
        match saved {
            Ok(true) => self.report(client, Some(&index), "database", None),
            other => {
                let span = global()
                    .map(|log| log.operation("local_analysis.storage", "save_legacy_index"));
                finish_span(span, persist_index(&self.cache_dir, client, &index))?;
                self.report(client, Some(&index), "json", other.err().map(|e| e.message));
            }
        }
        self.indexes
            .write()
            .map_err(|_| state_error())?
            .insert(client, Arc::new(index));
        Ok(())
    }

    pub fn library_storage_status(&self) -> CommandResult<Vec<LocalLibraryStorageStatus>> {
        let reports = self.storage_reports.read().map_err(|_| state_error())?;
        Ok([LocalClient::Stable, LocalClient::Lazer]
            .into_iter()
            .map(|client| {
                reports
                    .get(&client)
                    .cloned()
                    .unwrap_or(LocalLibraryStorageStatus {
                        client,
                        storage: "unscanned".into(),
                        revision: None,
                        entry_count: 0,
                        beatmap_count: 0,
                        error: None,
                    })
            })
            .collect())
    }

    pub fn migrate_cached_indexes(&self) -> CommandResult<Vec<LocalLibraryStorageStatus>> {
        let _guard = self.persistence.lock().map_err(|_| state_error())?;
        // Recovery may bring back a DB whose latest snapshot had no JSON fallback.
        // Restore missing clients before syncing; never replace an already loaded newer scan.
        for client in [LocalClient::Stable, LocalClient::Lazer] {
            if self.current_index(client)?.is_none() {
                match self.restore_library(client) {
                    Ok(Some(index)) => {
                        self.update_client_status(client, |status| {
                            status.last_scan_at = Some(index.summary.scanned_at.clone());
                        });
                        self.indexes
                            .write()
                            .map_err(|_| state_error())?
                            .insert(client, Arc::new(index));
                    }
                    Ok(None) => {}
                    Err(error) => self.report_storage_error(client, error),
                }
            }
        }
        let indexes = self.indexes.read().map_err(|_| state_error())?.clone();
        for (client, index) in indexes {
            match self.save_database(client, &index) {
                Ok(true) => {
                    self.record_revision(client, &index.summary.scanned_at)?;
                    self.report(client, Some(&index), "database", None);
                }
                Ok(false) => self.report(
                    client,
                    Some(&index),
                    "json",
                    Some("请先初始化本地数据库".into()),
                ),
                Err(error) => self.report(client, Some(&index), "json", Some(error.message)),
            }
        }
        self.library_storage_status()
    }

    pub fn beatmap_presence(
        &self,
        ids: Vec<i32>,
        client: Option<LocalClient>,
    ) -> CommandResult<Vec<LocalBeatmapPresence>> {
        if ids.len() > 1000 {
            return Err(CommandError::new(
                "LOCAL_PRESENCE_LIMIT",
                "每次最多查询 1000 张谱面",
            ));
        }
        let ids = ids
            .into_iter()
            .filter(|id| *id > 0)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let status = self.index_load_status()?;
        let mut known = status.phase == LocalIndexLoadPhase::Ready;
        let mut usable = false;
        let mut found = BTreeMap::<i32, Vec<LocalClient>>::new();
        for source_client in [LocalClient::Stable, LocalClient::Lazer]
            .into_iter()
            .filter(|c| client.is_none_or(|selected| selected == *c))
        {
            let source = self.sources.resolve(source_client)?;
            let cached = self.current_index(source_client)?;
            if source.status.data_root.is_none() && source.status.configured_path.is_none() {
                // An automatically detected installation may have gone offline.
                // A previously indexed client cannot be treated as never installed.
                if cached.is_some() {
                    known = false;
                }
                continue;
            }
            let Some(index) = cached.filter(|index| source_matches(&source, &index.source_root))
            else {
                known = false;
                continue;
            };
            usable = true;
            let phase = status
                .clients
                .get(&source_client)
                .map(|s| s.phase.as_str())
                .unwrap_or("idle");
            if index.summary.completeness != Completeness::Complete
                || matches!(phase, "pending" | "scanning" | "error")
                || index.diagnostics.iter().any(|d| {
                    matches!(
                        d.code.as_str(),
                        "BEATMAP_PARSE_ERROR"
                            | "RESOURCE_READ_ERROR"
                            | "DISCOVERY_ERROR"
                            | "METADATA_ERROR"
                    )
                })
            {
                known = false;
            }
            let present = self
                .database()?
                .and_then(|db| {
                    db.with_database("query_beatmap_presence", |database| {
                        library::present_ids(
                            &database.connection,
                            &source_client.to_string(),
                            &index.source_root,
                            &index.summary.scanned_at,
                            &ids,
                        )
                    })
                    .ok()
                    .flatten()
                    .flatten()
                })
                .unwrap_or_else(|| {
                    ids.iter()
                        .filter(|id| index.beatmap_id_lookup.contains_key(id))
                        .copied()
                        .collect()
                });
            for id in present {
                found.entry(id).or_default().push(source_client);
            }
        }
        Ok(ids
            .into_iter()
            .map(|beatmap_id| {
                let clients = found.remove(&beatmap_id).unwrap_or_default();
                LocalBeatmapPresence {
                    beatmap_id,
                    status: if !clients.is_empty() {
                        LocalPresenceState::Present
                    } else if known && usable {
                        LocalPresenceState::Missing
                    } else {
                        LocalPresenceState::Unknown
                    },
                    clients,
                }
            })
            .collect())
    }
}
