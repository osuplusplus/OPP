use super::*;
use rusqlite::params;

#[test]
fn upgrades_v1_in_place_without_changing_identity_or_existing_maps() {
    let directory = tempfile::tempdir().unwrap();
    let uuid = Uuid::new_v4().to_string();
    let mut connection = Connection::open(directory.path().join(FILE_NAME)).unwrap();
    migrate(&mut connection, 0, &uuid, &MIGRATIONS[..1]).unwrap();
    connection.execute_batch("INSERT INTO library_sources(id,client,root_path) VALUES(1,'stable','/old'); INSERT INTO beatmap_files(source_id,source_key,size_bytes) VALUES(1,'unsubmitted.osu',10);").unwrap();
    drop(connection);
    let database = open(directory.path(), false, Some(&uuid)).unwrap();
    assert_eq!(database.version, SCHEMA_VERSION);
    assert_eq!(database.uuid, uuid);
    assert_eq!(
        database
            .connection
            .query_row("SELECT source_key FROM beatmap_files", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "unsubmitted.osu"
    );
}

#[test]
fn snapshot_sync_skips_unchanged_rows_and_rolls_back_removals_on_failure() {
    use super::library::{self, Entry, Snapshot};
    let directory = tempfile::tempdir().unwrap();
    let mut db = open(directory.path(), true, None).unwrap();
    let plan = db
        .connection
        .prepare(&format!("EXPLAIN QUERY PLAN {}", library::PRESENCE_SQL))
        .unwrap()
        .query_map(params![42, 1], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(
        plan.iter().any(|line| line
            .contains("SEARCH m USING COVERING INDEX idx_beatmap_metadata_beatmap_id")
            || line.contains("SEARCH m USING INDEX idx_beatmap_metadata_beatmap_id")),
        "{plan:?}"
    );
    let entry = |key: &str, value: &str| Entry {
        key: key.into(),
        json: value.into(),
        hash: value.into(),
        beatmap: None,
    };
    let mut snapshot = Snapshot {
        client: "stable".into(),
        root: "/source".into(),
        schema: 7,
        algorithm: "test".into(),
        summary_json: "{}".into(),
        diagnostics_json: "[]".into(),
        revision: "old".into(),
        completeness: "complete".into(),
        entries: vec![entry("a", "old-a"), entry("b", "old-b")],
    };
    library::save(&mut db.connection, &snapshot).unwrap();
    db.connection.execute_batch("CREATE TABLE writes(key TEXT); CREATE TRIGGER audit_rows AFTER UPDATE ON resource_entries BEGIN INSERT INTO writes VALUES(new.entry_key); END;").unwrap();
    library::save(&mut db.connection, &snapshot).unwrap();
    assert_eq!(
        db.connection
            .query_row("SELECT COUNT(*) FROM writes", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        0
    );
    snapshot.entries[0] = entry("a", "updated-a");
    snapshot.revision = "new".into();
    library::save(&mut db.connection, &snapshot).unwrap();
    assert_eq!(
        db.connection
            .query_row("SELECT key FROM writes", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "a"
    );
    db.connection.execute_batch("CREATE TRIGGER fail_new BEFORE INSERT ON resource_entries WHEN new.entry_key='bad' BEGIN SELECT RAISE(ABORT,'fail'); END;").unwrap();
    snapshot.entries = vec![entry("bad", "bad")];
    snapshot.revision = "failed".into();
    assert!(library::save(&mut db.connection, &snapshot).is_err());
    let restored = library::load(&db.connection, "stable").unwrap().unwrap();
    assert_eq!(restored.revision, "new");
    assert_eq!(restored.entries.len(), 2);
    assert_eq!(restored.entries[0].json, "updated-a");
    assert!(
        library::present_ids(&db.connection, "stable", "/different", "new", &[42])
            .unwrap()
            .is_none()
    );
    snapshot.entries = vec![entry("b", "old-b")];
    snapshot.revision = "removed".into();
    library::save(&mut db.connection, &snapshot).unwrap();
    assert_eq!(
        library::load(&db.connection, "stable")
            .unwrap()
            .unwrap()
            .entries
            .len(),
        1
    );
}

#[test]
fn creates_empty_versioned_library_and_reopens_with_identity() {
    assert!(rusqlite::version_number() >= 3_051_003);
    let directory = tempfile::tempdir().unwrap();
    let database = open(directory.path(), true, None).unwrap();
    let uuid = database.uuid.clone();
    assert_eq!(database.version, SCHEMA_VERSION);
    for table in ["library_sources", "beatmap_files", "beatmap_metadata"] {
        assert_eq!(
            database
                .connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        database
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        database
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        5000
    );
    drop(database);
    assert_eq!(
        open(directory.path(), false, Some(&uuid)).unwrap().uuid,
        uuid
    );
    assert!(!directory.path().join("career.sqlite3").exists());
}

#[test]
fn rejects_foreign_newer_corrupt_and_mismatched_files_without_overwriting() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    let foreign = Connection::open(&path).unwrap();
    foreign
        .execute_batch(
            "CREATE TABLE unrelated (value TEXT); INSERT INTO unrelated VALUES ('keep');",
        )
        .unwrap();
    drop(foreign);
    let before = fs::read(&path).unwrap();
    assert_eq!(
        open(directory.path(), true, None).unwrap_err().code,
        "DATABASE_NOT_OPP"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::remove_file(&path).unwrap();
    let database = open(directory.path(), true, None).unwrap();
    let uuid = database.uuid.clone();
    assert_eq!(
        open(directory.path(), false, Some(&Uuid::new_v4().to_string()))
            .unwrap_err()
            .code,
        "DATABASE_UUID_MISMATCH"
    );
    database
        .connection
        .pragma_update(None, "user_version", 99)
        .unwrap();
    drop(database);
    let before = fs::read(&path).unwrap();
    assert_eq!(
        open(directory.path(), true, Some(&uuid)).unwrap_err().code,
        "DATABASE_VERSION_TOO_NEW"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(&path, b"not a sqlite database").unwrap();
    assert_eq!(
        open(directory.path(), true, None).unwrap_err().code,
        "DATABASE_CORRUPT"
    );
    assert_eq!(fs::read(&path).unwrap(), b"not a sqlite database");
}

#[test]
fn failed_migration_rolls_back_schema_identity_and_version() {
    let mut connection = Connection::open_in_memory().unwrap();
    let migrations = [
        MIGRATIONS[0],
        "CREATE TABLE partially_added(id INTEGER); INVALID SQL;",
    ];
    assert!(migrate(&mut connection, 0, &Uuid::new_v4().to_string(), &migrations).is_err());
    assert_eq!(read_version(&connection).unwrap(), 0);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get::<_, i32>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("PRAGMA application_id", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        0
    );
    migrate(
        &mut connection,
        0,
        &Uuid::new_v4().to_string(),
        &MIGRATIONS[..1],
    )
    .unwrap();
    let uuid = validate_identity(&connection, None).unwrap();
    migrate(
        &mut connection,
        1,
        &uuid,
        &[MIGRATIONS[0], "CREATE TABLE next_version(id INTEGER);"],
    )
    .unwrap();
    assert_eq!(read_version(&connection).unwrap(), 2);
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row
                .get::<_, i32>(0))
            .unwrap(),
        2
    );
}

#[test]
fn lock_timeout_returns_actionable_error_and_does_not_commit_migration() {
    let directory = tempfile::tempdir().unwrap();
    let mut database = open(directory.path(), true, None).unwrap();
    let blocker = Connection::open(directory.path().join(FILE_NAME)).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    // Use a short timeout here; production's five-second setting is checked separately.
    database
        .connection
        .busy_timeout(Duration::from_millis(30))
        .unwrap();
    let migrations = [
        MIGRATIONS[0],
        MIGRATIONS[1],
        "CREATE TABLE next_version(id INTEGER);",
    ];
    assert_eq!(
        migrate(
            &mut database.connection,
            SCHEMA_VERSION,
            &database.uuid,
            &migrations
        )
        .unwrap_err()
        .code,
        "DATABASE_BUSY"
    );
    assert_eq!(read_version(&database.connection).unwrap(), SCHEMA_VERSION);
    blocker.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn supports_unsubmitted_maps_duplicate_ids_and_hashes_across_clients_with_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let database = open(directory.path(), true, None).unwrap();
    let connection = &database.connection;
    connection.execute("INSERT INTO library_sources(id, client, root_path) VALUES (1, 'stable', '/stable'), (2, 'lazer', '/lazer')", []).unwrap();
    for (file_id, source, key, beatmap_id) in [
        (1, 1, "a.osu", Some(42)),
        (2, 1, "b.osu", Some(42)),
        (3, 2, "hash-key", Some(42)),
        (4, 1, "unsubmitted.osu", None),
    ] {
        connection.execute("INSERT INTO beatmap_files(id, source_id, source_key, size_bytes, beatmap_md5) VALUES (?1, ?2, ?3, 100, 'same-md5')", params![file_id, source, key]).unwrap();
        connection.execute("INSERT INTO beatmap_metadata(file_id, beatmap_id, beatmap_set_id, ruleset, title, artist, creator, difficulty_name) VALUES (?1, ?2, 99, 'osu', '标题', 'artist', 'mapper', 'hard')", params![file_id, beatmap_id]).unwrap();
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM beatmap_metadata WHERE beatmap_id=42",
                [],
                |row| row.get::<_, i32>(0)
            )
            .unwrap(),
        3
    );
    assert!(connection.execute("INSERT INTO beatmap_files(source_id, source_key, size_bytes) VALUES (1, 'a.osu', 0)", []).is_err());
    assert!(connection.execute("INSERT INTO beatmap_files(source_id, source_key, size_bytes) VALUES (999, 'a.osu', 0)", []).is_err());
    for (query, index) in [
        (
            "SELECT file_id FROM beatmap_metadata WHERE beatmap_id=42",
            "idx_beatmap_metadata_beatmap_id",
        ),
        (
            "SELECT file_id FROM beatmap_metadata WHERE beatmap_set_id=99",
            "idx_beatmap_metadata_set_id",
        ),
        (
            "SELECT id FROM beatmap_files WHERE beatmap_md5='same-md5'",
            "idx_beatmap_files_md5",
        ),
        (
            "SELECT id FROM beatmap_files WHERE source_id=1",
            "sqlite_autoindex_beatmap_files",
        ),
    ] {
        let detail: String = connection
            .query_row(&format!("EXPLAIN QUERY PLAN {query}"), [], |row| row.get(3))
            .unwrap();
        assert!(detail.contains(index), "{detail}");
    }
}

#[test]
fn does_not_recreate_missing_database_and_rejects_empty_conflicting_file() {
    let directory = tempfile::tempdir().unwrap();
    assert!(open(directory.path(), false, None).is_err());
    assert!(!directory.path().join(FILE_NAME).exists());
    fs::write(directory.path().join(FILE_NAME), []).unwrap();
    assert_eq!(
        open(directory.path(), true, None).unwrap_err().code,
        "DATABASE_NOT_OPP"
    );
}

#[test]
fn preserves_orphaned_sidecars_and_reports_missing_schema_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let sidecar = directory.path().join(format!("{FILE_NAME}-wal"));
    fs::write(&sidecar, b"keep orphaned WAL").unwrap();
    assert_eq!(
        open(directory.path(), true, None).unwrap_err().code,
        "DATABASE_FILE_CONFLICT"
    );
    assert!(!directory.path().join(FILE_NAME).exists());
    assert_eq!(fs::read(&sidecar).unwrap(), b"keep orphaned WAL");
    fs::remove_file(&sidecar).unwrap();
    let database = open(directory.path(), true, None).unwrap();
    database
        .connection
        .execute_batch("DROP INDEX idx_beatmap_files_md5")
        .unwrap();
    drop(database);
    assert_eq!(
        open(directory.path(), false, None).unwrap_err().code,
        "DATABASE_SCHEMA_INVALID"
    );
}
