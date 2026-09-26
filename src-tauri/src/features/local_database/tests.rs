use super::*;
use crate::infrastructure::local_database::FILE_NAME;

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn initializes_once_restarts_and_keeps_existing_data_untouched() {
    let app = tempfile::tempdir().unwrap();
    let existing = app.path().join("local-analysis");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("stable-index.json"), b"existing index").unwrap();
    let service = LocalDatabaseService::new(app.path());
    let initial = service.status().unwrap();
    assert_eq!(initial.phase, DatabasePhase::Unconfigured);
    assert!(!service.recommended.exists());
    let directory = app.path().join("数据库 中文 with spaces");
    let status = service.initialize(&path_string(&directory)).unwrap();
    assert_eq!(status.phase, DatabasePhase::Ready);
    let uuid = status.database_uuid.clone();
    assert_eq!(
        service
            .initialize(&path_string(&directory))
            .unwrap()
            .database_uuid,
        uuid
    );
    assert_eq!(
        service
            .initialize(&path_string(app.path()))
            .unwrap_err()
            .code,
        "DATABASE_ALREADY_CONFIGURED"
    );
    assert_eq!(
        service.locate(&path_string(&directory)).unwrap_err().code,
        "DATABASE_ALREADY_CONFIGURED"
    );
    service.shutdown();
    let restarted = LocalDatabaseService::new(app.path());
    assert_eq!(restarted.status().unwrap().database_uuid, uuid);
    assert_eq!(restarted.status().unwrap().phase, DatabasePhase::Ready);
    assert_eq!(
        fs::read(existing.join("stable-index.json")).unwrap(),
        b"existing index"
    );
    assert!(!directory.join("career.sqlite3").exists());
}

#[test]
fn lost_directory_retains_identity_and_relocates_only_same_database() {
    let app = tempfile::tempdir().unwrap();
    let original = app.path().join("original");
    let relocated = app.path().join("relocated");
    let service = LocalDatabaseService::new(app.path());
    let status = service.initialize(&path_string(&original)).unwrap();
    let uuid = status.database_uuid;
    service.shutdown();
    fs::rename(&original, &relocated).unwrap();
    let restarted = LocalDatabaseService::new(app.path());
    let failure = restarted.status().unwrap();
    assert_eq!(failure.phase, DatabasePhase::Error);
    assert_eq!(failure.database_uuid, uuid);
    assert!(!failure.can_initialize);
    assert!(!original.exists());
    assert_eq!(restarted.retry().unwrap().phase, DatabasePhase::Error);
    let foreign = app.path().join("another-library");
    fs::create_dir(&foreign).unwrap();
    drop(local_database::open(&foreign, true, None).unwrap());
    assert_eq!(
        restarted.locate(&path_string(&foreign)).unwrap_err().code,
        "DATABASE_UUID_MISMATCH"
    );
    assert_eq!(restarted.status().unwrap().database_uuid, uuid);
    let recovered = restarted.locate(&path_string(&relocated)).unwrap();
    assert_eq!(recovered.phase, DatabasePhase::Ready);
    assert_eq!(recovered.database_uuid, uuid);
    assert_eq!(
        recovered.directory,
        Some(path_string(&fs::canonicalize(relocated).unwrap()))
    );
}

#[test]
fn missing_file_is_not_recreated_even_if_directory_exists() {
    let app = tempfile::tempdir().unwrap();
    let directory = app.path().join("data");
    let service = LocalDatabaseService::new(app.path());
    let uuid = service
        .initialize(&path_string(&directory))
        .unwrap()
        .database_uuid;
    service.shutdown();
    fs::remove_file(directory.join(FILE_NAME)).unwrap();
    let status = service.retry().unwrap();
    assert_eq!(status.phase, DatabasePhase::Error);
    assert_eq!(status.database_uuid, uuid);
    assert!(!directory.join(FILE_NAME).exists());
    assert!(service.initialize(&path_string(&directory)).is_err());
    assert!(!directory.join(FILE_NAME).exists());
}

#[test]
fn failed_config_write_leaves_database_reusable_for_retry() {
    let app = tempfile::tempdir().unwrap();
    let service = LocalDatabaseService::new(app.path());
    let temporary = service.config_path.with_extension("json.tmp");
    fs::create_dir(&temporary).unwrap(); // deterministic write failure on Windows and Linux
    let directory = app.path().join("data");
    assert!(service.initialize(&path_string(&directory)).is_err());
    assert_eq!(service.status().unwrap().phase, DatabasePhase::Error);
    assert!(service.status().unwrap().can_initialize);
    assert!(!service.config_path.exists());
    let uuid = local_database::open(&directory, false, None).unwrap().uuid;
    fs::remove_dir(&temporary).unwrap();
    assert_eq!(
        service
            .initialize(&path_string(&directory))
            .unwrap()
            .database_uuid,
        Some(uuid)
    );
}

#[test]
fn invalid_config_blocks_creation_and_interrupted_publish_recovers_backup() {
    let app = tempfile::tempdir().unwrap();
    let service = LocalDatabaseService::new(app.path());
    fs::write(&service.config_path, b"broken config").unwrap();
    let status = service.status().unwrap();
    assert_eq!(status.phase, DatabasePhase::Error);
    assert!(!status.can_initialize);
    assert!(
        service
            .initialize(&path_string(&service.recommended))
            .is_err()
    );
    assert!(!service.recommended.exists());
    fs::remove_file(&service.config_path).unwrap();
    service.retry().unwrap();
    let status = service
        .initialize(&path_string(&service.recommended))
        .unwrap();
    service.shutdown();
    fs::rename(
        &service.config_path,
        service.config_path.with_extension("json.bak"),
    )
    .unwrap();
    let restarted = LocalDatabaseService::new(app.path());
    assert_eq!(restarted.status().unwrap().phase, DatabasePhase::Ready);
    assert_eq!(
        restarted.status().unwrap().database_uuid,
        status.database_uuid
    );
    assert!(service.config_path.exists());
}

#[test]
fn path_errors_are_retryable_and_readonly_library_is_not_reported_ready() {
    let app = tempfile::tempdir().unwrap();
    let service = LocalDatabaseService::new(app.path());
    assert_eq!(
        service.initialize("relative").unwrap_err().code,
        "DATABASE_PATH_INVALID"
    );
    let blocked = app.path().join("file");
    fs::write(&blocked, b"keep").unwrap();
    assert!(service.initialize(&path_string(&blocked)).is_err());
    let directory = service.recommended.clone();
    assert_eq!(
        service.initialize(&path_string(&directory)).unwrap().phase,
        DatabasePhase::Ready
    );
    service.shutdown();
    let library = directory.join(FILE_NAME);
    let original = fs::metadata(&library).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&library, readonly).unwrap();
    let status = service.retry().unwrap();
    // Directory/file permission bits do not constrain root on Linux.
    #[cfg(windows)]
    assert_eq!(status.phase, DatabasePhase::Error);
    #[cfg(not(windows))]
    let _ = status;
    service.shutdown();
    fs::set_permissions(&library, original).unwrap();
    assert_eq!(service.retry().unwrap().phase, DatabasePhase::Ready);
}

#[test]
fn concurrent_initialization_shares_one_database_uuid() {
    let app = tempfile::tempdir().unwrap();
    let service = std::sync::Arc::new(LocalDatabaseService::new(app.path()));
    let directory = path_string(&service.recommended);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let service = service.clone();
            let directory = directory.clone();
            std::thread::spawn(move || {
                service
                    .initialize(&directory)
                    .unwrap()
                    .database_uuid
                    .unwrap()
            })
        })
        .collect();
    let uuids: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert!(uuids.iter().all(|uuid| uuid == &uuids[0]));
}

#[test]
fn busy_library_preserves_configuration_and_becomes_ready_after_unlock() {
    let app = tempfile::tempdir().unwrap();
    let service = LocalDatabaseService::new(app.path());
    let ready = service
        .initialize(&path_string(&service.recommended))
        .unwrap();
    service.shutdown();
    let before = fs::read(&service.config_path).unwrap();
    let blocker = rusqlite::Connection::open(service.recommended.join(FILE_NAME)).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    let unavailable = service.retry().unwrap();
    assert_eq!(unavailable.phase, DatabasePhase::Error);
    assert_eq!(unavailable.error.unwrap().code, "DATABASE_BUSY");
    assert_eq!(unavailable.database_uuid, ready.database_uuid);
    assert!(started.elapsed() >= std::time::Duration::from_secs(4));
    assert_eq!(fs::read(&service.config_path).unwrap(), before);
    blocker.execute_batch("ROLLBACK").unwrap();
    assert_eq!(service.retry().unwrap().phase, DatabasePhase::Ready);
}

#[test]
fn relocation_config_write_failure_keeps_original_location_and_identity() {
    let app = tempfile::tempdir().unwrap();
    let service = LocalDatabaseService::new(app.path());
    let ready = service
        .initialize(&path_string(&service.recommended))
        .unwrap();
    service.shutdown();
    let relocated = app.path().join("relocated");
    fs::rename(&service.recommended, &relocated).unwrap();
    service.retry().unwrap();
    let before = fs::read(&service.config_path).unwrap();
    let temporary = service.config_path.with_extension("json.tmp");
    fs::create_dir(&temporary).unwrap();
    assert!(service.locate(&path_string(&relocated)).is_err());
    let unavailable = service.status().unwrap();
    assert_eq!(unavailable.directory, ready.directory);
    assert_eq!(unavailable.database_uuid, ready.database_uuid);
    assert_eq!(fs::read(&service.config_path).unwrap(), before);
    fs::remove_dir(&temporary).unwrap();
    assert_eq!(
        service.locate(&path_string(&relocated)).unwrap().phase,
        DatabasePhase::Ready
    );
}

#[cfg(windows)]
#[test]
fn rejects_network_paths_before_creating_anything() {
    let app = tempfile::tempdir().unwrap();
    let service = LocalDatabaseService::new(app.path());
    assert_eq!(
        service
            .initialize(r"\\server\share\database")
            .unwrap_err()
            .code,
        "DATABASE_NETWORK_DIRECTORY"
    );
}
