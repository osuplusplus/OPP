use super::super::tests::{OSU_FIXTURE, fixture_service};
use super::*;

fn attach(app: &Path, service: &LocalAnalysisService) -> Arc<LocalDatabaseService> {
    let database = Arc::new(LocalDatabaseService::new(app));
    database
        .initialize(&app.join("数据库 数据集").to_string_lossy())
        .unwrap();
    service.attach_database(Arc::clone(&database)).unwrap();
    database
}

fn scan(service: &LocalAnalysisService) {
    service
        .run_scan(
            LocalClient::Stable,
            false,
            Arc::new(|_| {}),
            &AtomicBool::new(false),
        )
        .unwrap();
}

#[test]
fn migrates_legacy_cache_without_scanning_and_prefers_new_database_after_restart() {
    let (app, _stable, legacy, path) = fixture_service();
    fs::write(&path, OSU_FIXTURE.replace("BeatmapID:-1", "BeatmapID:42")).unwrap();
    scan(&legacy);
    let cache_path = service_data::index_path(&legacy.cache_dir, LocalClient::Stable);
    let original = fs::read(&cache_path).unwrap();
    let index_before = serde_json::to_value(
        legacy
            .current_index(LocalClient::Stable)
            .unwrap()
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    let database = attach(app.path(), &legacy);
    legacy.load_cached_indexes();
    assert_eq!(
        legacy.library_storage_status().unwrap()[0].storage,
        "database"
    );
    assert_eq!(fs::read(&cache_path).unwrap(), original);
    database
        .with_database("verify_migration", |db| {
            let snapshot = library::load(&db.connection, "stable")?.unwrap();
            assert_eq!(
                snapshot.entries.len(),
                index_before["entries"].as_array().unwrap().len()
            );
            assert_eq!(
                db.connection
                    .query_row(
                        "SELECT COUNT(*) FROM beatmap_metadata WHERE beatmap_id=42",
                        [],
                        |row| row.get::<_, i32>(0)
                    )
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
    let restored = LocalAnalysisService::new(app.path()).unwrap();
    restored.attach_database(Arc::clone(&database)).unwrap();
    restored.load_cached_indexes();
    assert_eq!(
        serde_json::to_value(
            restored
                .current_index(LocalClient::Stable)
                .unwrap()
                .unwrap()
                .as_ref()
        )
        .unwrap(),
        index_before
    );
    fs::write(
        &path,
        OSU_FIXTURE
            .replace("BeatmapID:-1", "BeatmapID:42")
            .replace("Title:Fixture", "Title:Changed in database"),
    )
    .unwrap();
    scan(&restored);
    assert_eq!(
        fs::read(&cache_path).unwrap(),
        original,
        "DB writes do not rewrite the legacy index"
    );
    // Resource metadata remains readable even without reopening the physical .osu file.
    fs::remove_file(&path).unwrap();
    let restarted = LocalAnalysisService::new(app.path()).unwrap();
    restarted.attach_database(database).unwrap();
    restarted.load_cached_indexes();
    assert_eq!(
        restarted
            .query_beatmaps(BeatmapQuery::default())
            .unwrap()
            .items[0]
            .title,
        "Changed in database"
    );
    assert_eq!(
        restarted.library_storage_status().unwrap()[0].storage,
        "database"
    );
}

#[test]
fn failed_sql_write_falls_back_and_old_database_cannot_win_on_restart() {
    let (app, _stable, service, path) = fixture_service();
    scan(&service);
    let database = attach(app.path(), &service);
    service.load_cached_indexes();
    database.with_database("inject_write_failure", |db| {
        db.connection.execute_batch("CREATE TRIGGER fail_write BEFORE UPDATE ON resource_entries BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        Ok(())
    }).unwrap();
    fs::write(
        &path,
        OSU_FIXTURE.replace("Title:Fixture", "Title:Updated while database cannot write"),
    )
    .unwrap();
    scan(&service);
    let report = service.library_storage_status().unwrap();
    assert_eq!(report[0].storage, "json");
    assert!(report[0].error.is_some());
    let restarted = LocalAnalysisService::new(app.path()).unwrap();
    restarted.attach_database(Arc::clone(&database)).unwrap();
    restarted.load_cached_indexes();
    assert_eq!(
        restarted
            .query_beatmaps(BeatmapQuery::default())
            .unwrap()
            .items[0]
            .title,
        "Updated while database cannot write"
    );
    assert_eq!(
        restarted.library_storage_status().unwrap()[0].storage,
        "json"
    );
    database
        .with_database("remove_write_failure", |db| {
            db.connection
                .execute_batch("DROP TRIGGER fail_write")
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        restarted.migrate_cached_indexes().unwrap()[0].storage,
        "database"
    );
    let final_restart = LocalAnalysisService::new(app.path()).unwrap();
    final_restart.attach_database(database).unwrap();
    final_restart.load_cached_indexes();
    assert_eq!(
        final_restart.library_storage_status().unwrap()[0].storage,
        "database"
    );
    assert_eq!(
        final_restart
            .query_beatmaps(BeatmapQuery::default())
            .unwrap()
            .items[0]
            .title,
        "Updated while database cannot write"
    );
}

#[test]
fn missing_latest_database_does_not_restore_stale_json() {
    let (app, _stable, service, path) = fixture_service();
    scan(&service);
    let database = attach(app.path(), &service);
    service.load_cached_indexes();
    fs::write(
        &path,
        OSU_FIXTURE.replace("Title:Fixture", "Title:Latest database revision"),
    )
    .unwrap();
    scan(&service);
    database.shutdown();
    let original = app.path().join("数据库 数据集");
    let offline = app.path().join("offline-database");
    fs::rename(&original, &offline).unwrap();
    let reopened_db = Arc::new(LocalDatabaseService::new(app.path()));
    let restarted = LocalAnalysisService::new(app.path()).unwrap();
    restarted.attach_database(Arc::clone(&reopened_db)).unwrap();
    restarted.load_cached_indexes();
    assert!(
        restarted
            .current_index(LocalClient::Stable)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        restarted
            .beatmap_presence(vec![42], Some(LocalClient::Stable))
            .unwrap()[0]
            .status,
        LocalPresenceState::Unknown
    );
    fs::rename(&offline, &original).unwrap();
    reopened_db.retry().unwrap();
    assert_eq!(
        restarted.migrate_cached_indexes().unwrap()[0].storage,
        "database"
    );
    assert_eq!(
        restarted
            .query_beatmaps(BeatmapQuery::default())
            .unwrap()
            .items[0]
            .title,
        "Latest database revision"
    );
}

#[test]
fn database_roundtrip_keeps_lazer_resource_manifest_skins_and_duplicate_maps() {
    let (app, _stable, service, path) = fixture_service();
    fs::write(&path, OSU_FIXTURE.replace("BeatmapID:-1", "BeatmapID:42")).unwrap();
    fs::copy(&path, path.with_file_name("duplicate.osu")).unwrap();
    scan(&service);
    let database = attach(app.path(), &service);
    let lazer = app.path().join("lazer 数据");
    fs::create_dir_all(lazer.join("files")).unwrap();
    fs::write(lazer.join("client.realm"), []).unwrap();
    let source = service.set_source(LocalClient::Lazer, &lazer).unwrap();
    let mut index = (*service.current_index(LocalClient::Stable).unwrap().unwrap()).clone();
    index.source_root = source.data_root.unwrap();
    index.summary.source_root = index.source_root.clone();
    index.summary.client = LocalClient::Lazer;
    for entry in &mut index.entries {
        entry.lazer_files = Some(vec![lazer_realm::LazerRealmFile {
            filename: "audio.mp3".into(),
            hash: "blob-hash".into(),
            size: 123,
        }]);
        if let IndexedData::Beatmap { summary, detail } = &mut entry.data {
            summary.resource.client = LocalClient::Lazer;
            summary.resource.logical_path = Some("realm/path".into());
            detail.summary = summary.clone();
        }
    }
    let expected = serde_json::to_value(&index).unwrap();
    service
        .store_and_publish(LocalClient::Lazer, index)
        .unwrap();
    service.migrate_cached_indexes().unwrap();
    let restarted = LocalAnalysisService::new(app.path()).unwrap();
    restarted.attach_database(Arc::clone(&database)).unwrap();
    restarted.load_cached_indexes();
    assert_eq!(
        serde_json::to_value(
            restarted
                .current_index(LocalClient::Lazer)
                .unwrap()
                .unwrap()
                .as_ref()
        )
        .unwrap(),
        expected
    );
    let result = restarted.beatmap_presence(vec![42], None).unwrap();
    assert_eq!(
        result[0].clients,
        vec![LocalClient::Stable, LocalClient::Lazer]
    );
    assert_eq!(
        restarted
            .beatmap_presence(vec![42], Some(LocalClient::Lazer))
            .unwrap()[0]
            .clients,
        vec![LocalClient::Lazer]
    );
    database
        .with_database("verify_duplicates", |db| {
            assert_eq!(
                db.connection
                    .query_row(
                        "SELECT COUNT(*) FROM beatmap_metadata WHERE beatmap_id=42",
                        [],
                        |r| r.get::<_, i32>(0)
                    )
                    .unwrap(),
                4
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn presence_distinguishes_partial_pending_unavailable_and_changed_sources() {
    let (app, stable, service, path) = fixture_service();
    fs::write(&path, OSU_FIXTURE.replace("BeatmapID:-1", "BeatmapID:42")).unwrap();
    let _database = attach(app.path(), &service);
    scan(&service);
    service.load_cached_indexes();
    let result = service
        .beatmap_presence(vec![42, 99, 42, -1], Some(LocalClient::Stable))
        .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].status, LocalPresenceState::Present);
    assert_eq!(result[1].status, LocalPresenceState::Missing);
    // A formerly automatic second source disappearing must not prove absence.
    let old_lazer = service.current_index(LocalClient::Stable).unwrap().unwrap();
    service
        .indexes
        .write()
        .unwrap()
        .insert(LocalClient::Lazer, old_lazer);
    assert_eq!(
        service.beatmap_presence(vec![99], None).unwrap()[0].status,
        LocalPresenceState::Unknown
    );
    service.indexes.write().unwrap().remove(&LocalClient::Lazer);
    service.update_client_status(LocalClient::Stable, |s| s.phase = "pending".into());
    assert_eq!(
        service
            .beatmap_presence(vec![99], Some(LocalClient::Stable))
            .unwrap()[0]
            .status,
        LocalPresenceState::Unknown
    );
    service.update_client_status(LocalClient::Stable, |s| s.phase = "watching".into());
    let mut index = (*service.current_index(LocalClient::Stable).unwrap().unwrap()).clone();
    index.summary.completeness = Completeness::Partial;
    service
        .store_and_publish(LocalClient::Stable, index)
        .unwrap();
    assert_eq!(
        service
            .beatmap_presence(vec![99], Some(LocalClient::Stable))
            .unwrap()[0]
            .status,
        LocalPresenceState::Unknown
    );
    fs::rename(
        stable.path().join("Songs"),
        stable.path().join("OfflineSongs"),
    )
    .unwrap();
    assert_eq!(
        service
            .beatmap_presence(vec![42], Some(LocalClient::Stable))
            .unwrap()[0]
            .status,
        LocalPresenceState::Unknown
    );
    let new_root = app.path().join("new-source");
    fs::create_dir_all(new_root.join("Songs")).unwrap();
    fs::write(new_root.join("osu!.exe"), []).unwrap();
    service.set_source(LocalClient::Stable, &new_root).unwrap();
    assert_eq!(
        service
            .beatmap_presence(vec![42], Some(LocalClient::Stable))
            .unwrap()[0]
            .status,
        LocalPresenceState::Unknown
    );
    assert!(service.beatmap_presence(vec![1; 1001], None).is_err());
}
