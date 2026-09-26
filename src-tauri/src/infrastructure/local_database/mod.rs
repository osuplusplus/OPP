//! SQLite identity, connection settings and transactional schema migrations.
//! This library stores rebuildable indexes only; career history has its own database.

use std::{fs, path::Path, time::Duration};

use chrono::Utc;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::{finish_span, global},
};

pub(crate) const FILE_NAME: &str = "library.sqlite3";
pub(crate) const SCHEMA_VERSION: u32 = 2;
const APPLICATION_ID: i32 = 0x4f50504c; // OPPL
const MIGRATIONS: &[&str] = &[include_str!("schema_v1.sql"), include_str!("schema_v2.sql")];
pub(crate) mod library;

pub(crate) fn fs_io<T>(
    operation: &str,
    path: &Path,
    result: std::io::Result<T>,
) -> CommandResult<T> {
    let span = global().map(|logger| logger.operation("local_database.fs", operation));
    if let Some(span) = &span {
        span.fs_op(operation, path, &result);
    }
    finish_span(
        span,
        result.map_err(|error| CommandError::from_error("DATABASE_PATH_ERROR", error)),
    )
}

#[derive(Debug)]
pub(crate) struct LibraryDatabase {
    // Only the feature service's mutex may access this connection.
    pub(crate) connection: Connection,
    pub(crate) uuid: String,
    pub(crate) version: u32,
}

/// Full integrity scans are reserved for explicit adoption/recovery, not every startup.
fn verify_integrity(connection: &Connection) -> CommandResult<()> {
    let span = global().map(|logger| logger.operation("local_database", "verify_integrity"));
    let result = (|| {
        let check: String = connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(sql_error)?;
        if check != "ok" {
            return Err(CommandError::new(
                "DATABASE_CORRUPT",
                "数据库完整性检查失败，请保留原文件并恢复备份",
            ));
        }
        Ok(())
    })();
    finish_span(span, result)
}

pub(crate) fn open(
    directory: &Path,
    allow_create: bool,
    expected_uuid: Option<&str>,
) -> CommandResult<LibraryDatabase> {
    let span = global().map(|logger| logger.operation("local_database", "open_library"));
    let result = open_inner(directory, allow_create, expected_uuid, allow_create);
    finish_span(span, result)
}

pub(crate) fn open_for_recovery(
    directory: &Path,
    expected_uuid: &str,
) -> CommandResult<LibraryDatabase> {
    let span = global().map(|logger| logger.operation("local_database", "recover_library"));
    finish_span(
        span,
        open_inner(directory, false, Some(expected_uuid), true),
    )
}

fn open_inner(
    directory: &Path,
    allow_create: bool,
    expected_uuid: Option<&str>,
    check_integrity: bool,
) -> CommandResult<LibraryDatabase> {
    if rusqlite::version_number() < 3_051_003 {
        return Err(CommandError::new(
            "DATABASE_RUNTIME_TOO_OLD",
            "SQLite 版本过旧，无法安全启用本地数据库",
        ));
    }
    let path = directory.join(FILE_NAME);
    if allow_create && !fs_io("exists", &path, path.try_exists())? {
        for suffix in ["-wal", "-shm"] {
            let sidecar = directory.join(format!("{FILE_NAME}{suffix}"));
            if fs_io("exists", &sidecar, sidecar.try_exists())? {
                return Err(CommandError::new(
                    "DATABASE_FILE_CONFLICT",
                    "目录内存在遗留的数据库日志文件，请保留文件并选择其他目录",
                ));
            }
        }
    }
    if fs_io("exists", &path, path.try_exists())? {
        let metadata = fs_io("symlink_metadata", &path, fs::symlink_metadata(&path))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(CommandError::new(
                "DATABASE_FILE_CONFLICT",
                "数据库文件名被其他文件类型占用，请选择其他目录",
            ));
        }
    }
    // Reserve a new file exclusively. Never let SQLite recreate a missing configured library.
    let created = if allow_create {
        let reserve = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path);
        if let Some(logger) = global() {
            logger.operation("local_database.fs", "reserve_file").fs_op(
                "create_new",
                &path,
                &reserve,
            );
        }
        match reserve {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(CommandError::from_error("DATABASE_PATH_ERROR", error)),
        }
    } else {
        let metadata = fs_io("metadata", &path, fs::metadata(&path)).map_err(|mut error| {
            error.code = "DATABASE_FILE_UNAVAILABLE".into();
            error
        })?;
        if !metadata.is_file() {
            return Err(CommandError::new(
                "DATABASE_FILE_UNAVAILABLE",
                "数据库位置不是文件",
            ));
        }
        false
    };
    let result = (|| {
        let mut connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sql_error)?;
        let uuid = if created {
            Uuid::new_v4().to_string()
        } else {
            validate_identity(&connection, expected_uuid)?
        };
        let version = if created {
            0
        } else {
            read_version(&connection)?
        };
        if version > SCHEMA_VERSION {
            return Err(CommandError::new(
                "DATABASE_VERSION_TOO_NEW",
                "数据库来自更新版本的 OPP，请更新软件后再打开",
            ));
        }
        if !created && version == SCHEMA_VERSION {
            validate_schema(&connection)?;
        }
        if !created && check_integrity {
            verify_integrity(&connection)?;
        }
        if connection
            .is_readonly(rusqlite::MAIN_DB)
            .map_err(sql_error)?
        {
            return Err(CommandError::new(
                "DATABASE_READ_ONLY",
                "数据库或保存目录不可写，请检查权限",
            ));
        }
        // Validate ownership before applying any mutating PRAGMA or migration.
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(sql_error)?;
        let mode: String = connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .map_err(sql_error)?;
        if mode != "wal" {
            return Err(CommandError::new(
                "DATABASE_WAL_UNAVAILABLE",
                "所选位置不支持 SQLite WAL，请选择本机磁盘目录",
            ));
        }
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(sql_error)?;
        connection
            .pragma_update(None, "wal_autocheckpoint", 1000)
            .map_err(sql_error)?;
        migrate(&mut connection, version, &uuid, MIGRATIONS)?;
        validate_schema(&connection)?;
        // An empty write transaction verifies WAL/directory access and honors lock timeout,
        // without changing any user data. Opening READ_WRITE alone can fall back to read-only.
        let write_check =
            global().map(|logger| logger.operation("local_database", "verify_write_access"));
        let writable = (|| {
            connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?
                .rollback()
                .map_err(sql_error)
        })();
        finish_span(write_check, writable)?;
        Ok(LibraryDatabase {
            connection,
            uuid,
            version: SCHEMA_VERSION,
        })
    })();
    if result.is_err() && created {
        // The connection has already dropped; remove only files created by this attempt.
        for owned in [
            path.clone(),
            path.with_file_name(format!("{FILE_NAME}-wal")),
            path.with_file_name(format!("{FILE_NAME}-shm")),
        ] {
            let removed = fs::remove_file(&owned);
            if let Some(logger) = global() {
                let span = logger.operation("local_database", "cleanup_failed_creation");
                span.fs_op("remove", &owned, &removed);
            }
        }
    }
    result
}

fn read_version(connection: &Connection) -> CommandResult<u32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(sql_error)
}

fn validate_identity(
    connection: &Connection,
    expected_uuid: Option<&str>,
) -> CommandResult<String> {
    let application_id: i32 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(sql_error)?;
    if application_id != APPLICATION_ID {
        return Err(CommandError::new(
            "DATABASE_NOT_OPP",
            "所选文件不是 OPP 本地索引数据库，未进行修改",
        ));
    }
    if read_version(connection)? > SCHEMA_VERSION {
        return Err(CommandError::new(
            "DATABASE_VERSION_TOO_NEW",
            "数据库来自更新版本的 OPP，请更新软件后再打开",
        ));
    }
    let (uuid, purpose): (String, String) = connection
        .query_row(
            "SELECT database_uuid, purpose FROM database_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if purpose != "opp_local_library" || Uuid::parse_str(&uuid).is_err() {
        return Err(CommandError::new(
            "DATABASE_IDENTITY_INVALID",
            "数据库身份信息无效",
        ));
    }
    if expected_uuid.is_some_and(|expected| expected != uuid) {
        return Err(CommandError::new(
            "DATABASE_UUID_MISMATCH",
            "这不是原先配置的数据库，请选择包含原数据库的目录",
        ));
    }
    Ok(uuid)
}

fn migrate(
    connection: &mut Connection,
    current: u32,
    uuid: &str,
    migrations: &[&str],
) -> CommandResult<()> {
    if current as usize == migrations.len() {
        return Ok(());
    }
    let span = global().map(|logger| logger.operation("local_database", "migrate_schema"));
    let result = (|| {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        for (index, sql) in migrations.iter().enumerate().skip(current as usize) {
            transaction.execute_batch(sql).map_err(sql_error)?;
            if index == 0 {
                transaction
                    .execute(
                        "INSERT INTO database_meta VALUES (1, ?1, ?2, 'opp_local_library')",
                        [uuid, &Utc::now().to_rfc3339()],
                    )
                    .map_err(sql_error)?;
                transaction
                    .pragma_update(None, "application_id", APPLICATION_ID)
                    .map_err(sql_error)?;
            }
            transaction
                .execute(
                    "INSERT INTO schema_migrations VALUES (?1, ?2)",
                    rusqlite::params![index + 1, Utc::now().to_rfc3339()],
                )
                .map_err(sql_error)?;
            transaction
                .pragma_update(None, "user_version", index + 1)
                .map_err(sql_error)?;
        }
        transaction.commit().map_err(sql_error)
    })();
    finish_span(span, result)
}

fn validate_schema(connection: &Connection) -> CommandResult<()> {
    for sql in [
        "SELECT singleton, database_uuid, created_at, purpose FROM database_meta LIMIT 0",
        "SELECT version, applied_at FROM schema_migrations LIMIT 0",
        "SELECT id, client, root_path, last_successful_scan_at, completeness FROM library_sources LIMIT 0",
        "SELECT id, source_id, source_key, physical_path, logical_path, size_bytes, modified_ms, content_hash, beatmap_md5, resource_entry_id FROM beatmap_files LIMIT 0",
        "SELECT file_id, beatmap_id, beatmap_set_id, ruleset, title, title_unicode, artist, artist_unicode, creator, difficulty_name FROM beatmap_metadata LIMIT 0",
        "SELECT id, source_id, entry_key, payload_json, payload_hash FROM resource_entries LIMIT 0",
        "SELECT client, source_id, index_schema, difficulty_algorithm, summary_json, diagnostics_json, revision, entry_count FROM library_snapshots LIMIT 0",
    ] {
        connection.prepare(sql).map_err(sql_error)?;
    }
    let (count, max): (u32, Option<u32>) = connection
        .query_row(
            "SELECT COUNT(*), MAX(version) FROM schema_migrations",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if count != SCHEMA_VERSION || max != Some(SCHEMA_VERSION) {
        return Err(CommandError::new(
            "DATABASE_SCHEMA_INVALID",
            "数据库迁移记录不完整",
        ));
    }
    for index in [
        "idx_beatmap_files_md5",
        "idx_beatmap_metadata_beatmap_id",
        "idx_beatmap_metadata_set_id",
        "idx_beatmap_files_resource_entry",
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
                [index],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Err(CommandError::new(
                "DATABASE_SCHEMA_INVALID",
                "数据库索引结构不完整",
            ));
        }
    }
    Ok(())
}

pub(crate) fn sql_error(error: rusqlite::Error) -> CommandError {
    use rusqlite::ErrorCode;
    let (code, message) = match error.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => {
            ("DATABASE_BUSY", "数据库被占用，请稍后重试")
        }
        Some(ErrorCode::ReadOnly | ErrorCode::PermissionDenied) => {
            ("DATABASE_READ_ONLY", "数据库或保存目录不可写，请检查权限")
        }
        Some(ErrorCode::DiskFull) => ("DATABASE_DISK_FULL", "数据库所在磁盘空间不足"),
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => {
            ("DATABASE_CORRUPT", "数据库文件损坏或格式无效，请保留原文件")
        }
        _ => (
            "DATABASE_SQL_ERROR",
            "数据库操作失败，请检查文件与日志后重试",
        ),
    };
    let mut result = CommandError::from_error(code, error);
    result.message = message.into();
    result
}

#[cfg(test)]
mod tests;
