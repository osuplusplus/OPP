//! Row-based index persistence. Domain conversion stays in local_analysis.
use super::sql_error;
use crate::error::{CommandError, CommandResult};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Snapshot {
    pub client: String,
    pub root: String,
    pub schema: u32,
    pub algorithm: String,
    pub summary_json: String,
    pub diagnostics_json: String,
    pub revision: String,
    pub completeness: String,
    pub entries: Vec<Entry>,
}
pub(crate) struct Entry {
    pub key: String,
    pub json: String,
    pub hash: String,
    pub beatmap: Option<Beatmap>,
}
pub(crate) struct Beatmap {
    pub physical_path: String,
    pub logical_path: Option<String>,
    pub bytes: i64,
    pub modified_ms: i64,
    pub content_hash: Option<String>,
    pub md5: Option<String>,
    pub id: Option<i32>,
    pub set_id: Option<i32>,
    pub ruleset: String,
    pub title: String,
    pub title_unicode: String,
    pub artist: String,
    pub artist_unicode: String,
    pub creator: String,
    pub difficulty: String,
}

pub(crate) fn load(connection: &Connection, client: &str) -> CommandResult<Option<Snapshot>> {
    let tx = connection.unchecked_transaction().map_err(sql_error)?;
    let snapshot = tx.query_row(
        "SELECT s.source_id, l.root_path, s.index_schema, s.difficulty_algorithm, s.summary_json, s.diagnostics_json, s.revision, l.completeness, s.entry_count
         FROM library_snapshots s JOIN library_sources l ON l.id=s.source_id WHERE s.client=?1",
        [client], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, usize>(8)?, Snapshot {
            client: client.into(), root: row.get(1)?, schema: row.get(2)?, algorithm: row.get(3)?,
            summary_json: row.get(4)?, diagnostics_json: row.get(5)?, revision: row.get(6)?,
            completeness: row.get(7)?, entries: Vec::new(),
        })),
    ).optional().map_err(sql_error)?;
    let Some((source, expected_count, mut snapshot)) = snapshot else {
        return Ok(None);
    };
    let mut statement = tx.prepare("SELECT entry_key, payload_json, payload_hash FROM resource_entries WHERE source_id=?1 ORDER BY entry_key").map_err(sql_error)?;
    snapshot.entries = statement
        .query_map([source], |row| {
            Ok(Entry {
                key: row.get(0)?,
                json: row.get(1)?,
                hash: row.get(2)?,
                beatmap: None,
            })
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(sql_error)?;
    if snapshot.entries.len() != expected_count {
        return Err(CommandError::new(
            "DATABASE_INDEX_INVALID",
            "数据库资源条目数量不匹配",
        ));
    }
    drop(statement);
    tx.commit().map_err(sql_error)?;
    Ok(Some(snapshot))
}

pub(crate) fn save(connection: &mut Connection, snapshot: &Snapshot) -> CommandResult<()> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    tx.execute("INSERT INTO library_sources(client, root_path, last_successful_scan_at, completeness) VALUES (?1,?2,?3,?4)
        ON CONFLICT(client,root_path) DO UPDATE SET last_successful_scan_at=excluded.last_successful_scan_at, completeness=excluded.completeness",
        params![snapshot.client, snapshot.root, snapshot.revision, snapshot.completeness]).map_err(sql_error)?;
    let source: i64 = tx
        .query_row(
            "SELECT id FROM library_sources WHERE client=?1 AND root_path=?2",
            params![snapshot.client, snapshot.root],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    let previous: Option<i64> = tx
        .query_row(
            "SELECT source_id FROM library_snapshots WHERE client=?1",
            [&snapshot.client],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some(previous) = previous.filter(|previous| *previous != source) {
        tx.execute("DELETE FROM library_sources WHERE id=?1", [previous])
            .map_err(sql_error)?;
    }
    let existing = {
        let mut statement = tx
            .prepare("SELECT entry_key,payload_hash FROM resource_entries WHERE source_id=?1")
            .map_err(sql_error)?;
        statement
            .query_map([source], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(sql_error)?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()
            .map_err(sql_error)?
    };
    let keys: BTreeSet<_> = snapshot
        .entries
        .iter()
        .map(|entry| entry.key.as_str())
        .collect();
    for key in existing.keys().filter(|key| !keys.contains(key.as_str())) {
        tx.execute(
            "DELETE FROM resource_entries WHERE source_id=?1 AND entry_key=?2",
            params![source, key],
        )
        .map_err(sql_error)?;
    }
    for entry in &snapshot.entries {
        if existing.get(&entry.key) == Some(&entry.hash) {
            continue;
        }
        tx.execute("INSERT INTO resource_entries(source_id,entry_key,payload_json,payload_hash) VALUES (?1,?2,?3,?4)
            ON CONFLICT(source_id,entry_key) DO UPDATE SET payload_json=excluded.payload_json,payload_hash=excluded.payload_hash",
            params![source,entry.key,entry.json,entry.hash]).map_err(sql_error)?;
        let resource: i64 = tx
            .query_row(
                "SELECT id FROM resource_entries WHERE source_id=?1 AND entry_key=?2",
                params![source, entry.key],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if let Some(map) = &entry.beatmap {
            tx.execute("INSERT INTO beatmap_files(source_id,source_key,physical_path,logical_path,size_bytes,modified_ms,content_hash,beatmap_md5,resource_entry_id)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(source_id,source_key) DO UPDATE SET physical_path=excluded.physical_path,
                logical_path=excluded.logical_path,size_bytes=excluded.size_bytes,modified_ms=excluded.modified_ms,content_hash=excluded.content_hash,
                beatmap_md5=excluded.beatmap_md5,resource_entry_id=excluded.resource_entry_id",
                params![source,entry.key,map.physical_path,map.logical_path,map.bytes,map.modified_ms,map.content_hash,map.md5,resource]).map_err(sql_error)?;
            let file: i64 = tx
                .query_row(
                    "SELECT id FROM beatmap_files WHERE source_id=?1 AND source_key=?2",
                    params![source, entry.key],
                    |r| r.get(0),
                )
                .map_err(sql_error)?;
            tx.execute("INSERT INTO beatmap_metadata(file_id,beatmap_id,beatmap_set_id,ruleset,title,title_unicode,artist,artist_unicode,creator,difficulty_name)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(file_id) DO UPDATE SET beatmap_id=excluded.beatmap_id,beatmap_set_id=excluded.beatmap_set_id,
                ruleset=excluded.ruleset,title=excluded.title,title_unicode=excluded.title_unicode,artist=excluded.artist,artist_unicode=excluded.artist_unicode,
                creator=excluded.creator,difficulty_name=excluded.difficulty_name",
                params![file,map.id,map.set_id,map.ruleset,map.title,map.title_unicode,map.artist,map.artist_unicode,map.creator,map.difficulty]).map_err(sql_error)?;
        } else {
            tx.execute(
                "DELETE FROM beatmap_files WHERE resource_entry_id=?1",
                [resource],
            )
            .map_err(sql_error)?;
        }
    }
    tx.execute("INSERT INTO library_snapshots(client,source_id,index_schema,difficulty_algorithm,summary_json,diagnostics_json,revision,entry_count) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
        ON CONFLICT(client) DO UPDATE SET source_id=excluded.source_id,index_schema=excluded.index_schema,difficulty_algorithm=excluded.difficulty_algorithm,
        summary_json=excluded.summary_json,diagnostics_json=excluded.diagnostics_json,revision=excluded.revision,entry_count=excluded.entry_count",
        params![snapshot.client,source,snapshot.schema,snapshot.algorithm,snapshot.summary_json,snapshot.diagnostics_json,snapshot.revision,snapshot.entries.len()]).map_err(sql_error)?;
    tx.commit().map_err(sql_error)
}

/// One indexed lookup per requested ID, using only the current matching source/revision.
pub(super) const PRESENCE_SQL: &str =
    "SELECT EXISTS(SELECT 1 FROM beatmap_metadata m INDEXED BY idx_beatmap_metadata_beatmap_id
    JOIN beatmap_files f ON f.id=m.file_id JOIN resource_entries e ON e.id=f.resource_entry_id
    WHERE m.beatmap_id=?1 AND f.source_id=?2)";

pub(crate) fn present_ids(
    connection: &Connection,
    client: &str,
    root: &str,
    revision: &str,
    ids: &[i32],
) -> CommandResult<Option<BTreeSet<i32>>> {
    let tx = connection.unchecked_transaction().map_err(sql_error)?;
    let source: Option<i64> = tx.query_row("SELECT s.source_id FROM library_snapshots s JOIN library_sources l ON l.id=s.source_id WHERE s.client=?1 AND l.root_path=?2 AND s.revision=?3", params![client,root,revision], |r| r.get(0)).optional().map_err(sql_error)?;
    let Some(source) = source else {
        return Ok(None);
    };
    let mut statement = tx.prepare(PRESENCE_SQL).map_err(sql_error)?;
    let mut present = BTreeSet::new();
    for id in ids {
        if statement
            .query_row(params![id, source], |r| r.get::<_, bool>(0))
            .map_err(sql_error)?
        {
            present.insert(*id);
        }
    }
    drop(statement);
    tx.commit().map_err(sql_error)?;
    Ok(Some(present))
}
