use serde_json::{Value, json};

use super::{models::*, rino, service, uri};

fn reference() -> TournamentPoolRef {
    TournamentPoolRef {
        url: None,
        provider: "rino".into(),
        season: "s1".into(),
        category: "qualification".into(),
    }
}

fn selection(id: i32, mods: &str, position: u32) -> Value {
    json!({ "beatmapId": id, "selectedMods": mods, "modPosition": position,
        "season": "s1", "category": "qualification", "approved": true, "padding": true,
        "selectedBy": "123", "comment": "<script>comment</script>", "isCustome": true, "isOrigin": false })
}

#[test]
fn uri_accepts_all_published_seasons_and_stages() {
    for season in ["s1", "s2"] {
        for (category, _) in CATEGORIES {
            let parsed = uri::parse(&format!(
                "opp://mappool/rino?season={season}&category={category}"
            ))
            .unwrap();
            assert_eq!(parsed.season, season);
            assert_eq!(parsed.category, category);
        }
    }
}

#[tokio::test]
#[ignore = "requires the public Rino API; run explicitly for integration verification"]
async fn live_rino_api_accepts_all_supported_pool_references() {
    for season in ["s1", "s2"] {
        for (category, _) in CATEGORIES {
            let reference = TournamentPoolRef {
                url: None,
                provider: "rino".into(),
                season: season.into(),
                category: category.into(),
            };
            let pool = rino::fetch(&reference).await.unwrap();
            assert_eq!(pool.reference, reference);
            assert!(pool.entries.iter().all(|entry| entry.beatmap_id > 0));
        }
    }
}

#[test]
fn uri_rejects_ambiguous_or_external_inputs() {
    for raw in [
        "https://mappool/rino?season=s1&category=qualification",
        "opp://mappool/other?season=s1&category=qualification",
        "opp://user@mappool/rino?season=s1&category=qualification",
        "opp://mappool:123/rino?season=s1&category=qualification",
        "opp://mappool/rino?season=s1&category=qualification#fragment",
        "opp://mappool/rino?season=s1&category=qualification&season=s2",
        "opp://mappool/rino?season=s1&category=qualification&url=https://example.com",
        "opp://mappool/rino?season=s1&category=qualification&action=download",
        "opp://mappool/rino?season=s3&category=qualification",
        "opp://mappool/rino?season=s1&category=unknown",
        "opp://mappool/rino?season=s1",
    ] {
        assert!(uri::parse(raw).is_err(), "{raw}");
    }
}

#[test]
fn inbox_survives_readiness_and_deduplicates_without_losing_newer_links() {
    let mut inbox = uri::LinkInbox::default();
    let first = inbox.receive(reference());
    assert_eq!(inbox.receive(reference()).id, first.id);
    let mut second = reference();
    second.season = "s2".into();
    let newer = inbox.receive(second.clone());
    inbox.acknowledge(first.id);
    assert_eq!(inbox.receive(second.clone()).id, newer.id);
    inbox.acknowledge(newer.id);
    assert!(inbox.receive(second).id > newer.id);
}

#[test]
fn rino_keeps_only_pool_metadata_and_accepts_missing_names() {
    let bytes = serde_json::to_vec(
        &json!({"success": true, "data": [selection(10, "TB", 1), selection(11, "LZ", 2)]}),
    )
    .unwrap();
    let pool = rino::decode(&bytes, &reference()).unwrap();
    assert_eq!(pool.entries[0].selection_type, "LZ");
    assert_eq!(pool.entries[1].selection_type, "TB");
    assert!(pool.entries[0].is_custom);
    assert!(!pool.entries[0].is_original);
    assert!(pool.entries[0].selected_by_name.is_none());
    assert_eq!(pool.entries[0].comment, "<script>comment</script>");
    assert!(pool.entries[0].beatmap.is_none());
    let candidates = service::candidates(&pool);
    assert_eq!(candidates[0].beatmap_id, Some(11));
    assert_eq!(candidates[0].beatmapset_id, None);
}

#[test]
fn rino_accepts_empty_but_rejects_failures_invalid_ids_and_mismatched_stages() {
    assert!(
        rino::decode(br#"{"success":true,"data":[]}"#, &reference())
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(rino::decode(br#"{"success":false,"data":[]}"#, &reference()).is_err());
    assert!(rino::decode(b"not json", &reference()).is_err());
    for (field, value) in [
        ("beatmapId", json!(0)),
        ("category", json!("finals")),
        ("approved", json!(false)),
        ("padding", json!(false)),
    ] {
        let mut entry = selection(10, "NM", 1);
        entry[field] = value;
        assert!(
            rino::decode(
                &serde_json::to_vec(&json!({"success":true,"data":[entry]})).unwrap(),
                &reference()
            )
            .is_err()
        );
    }
}

#[test]
fn metadata_must_match_the_selected_standard_difficulty() {
    let value = json!({"id":10,"mode":"osu","beatmapset_id":20,"version":"Expert","checksum":"abc",
        "beatmapset":{"id":20,"title":"Song","artist":"Artist","creator":"Mapper"}});
    assert_eq!(
        service::decode_beatmap(10, value.clone())
            .unwrap()
            .beatmapset_id,
        20
    );
    assert!(service::decode_beatmap(11, value.clone()).is_err());
    let mut mania = value;
    mania["mode"] = json!("mania");
    assert!(service::decode_beatmap(10, mania).is_err());
}

#[tokio::test]
async fn imported_online_stats_survive_restart_without_claiming_local_files() {
    let directory = tempfile::tempdir().unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let metadata = json!({"id":10,"mode":"osu","beatmapset_id":20,"version":"Expert",
        "difficulty_rating":6.25,"bpm":175,"total_length":164,"ar":9.5,"accuracy":9,"cs":3.8,"drain":5,
        "count_circles":300,"count_sliders":150,"count_spinners":1,"max_combo":1000,
        "beatmapset":{"id":20,"title":"Song","artist":"Artist","creator":"Mapper"}});
    let pool = service::enrich(standard_pool(standard_document()), |id| {
        let value = metadata.clone();
        async move { service::decode_beatmap(id, value) }
    })
    .await;
    let saved = service::save(pool, &state).unwrap();
    drop(state);
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let folder = state.collections.folder(&saved.folder_id).unwrap();
    assert_eq!(
        folder.pool.unwrap().slots[0].metadata.as_ref(),
        Some(&metadata)
    );
    assert!(!folder.entries[0].resolved);
    assert!(!folder.pending_write);
    assert!(!folder.stable_sync);
}

#[test]
fn old_pool_metadata_repair_preserves_membership_records_and_sync_state() {
    let directory = tempfile::tempdir().unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let saved = service::save(standard_pool(standard_document()), &state).unwrap();
    let original = state.collections.folder(&saved.folder_id).unwrap();
    // Simulate the older layout where the pool snapshot lived beside the notebook.
    state
        .collections
        .notebooks
        .save_pool(&saved.folder_id, original.pool.clone().unwrap())
        .unwrap();
    state
        .collections
        .replace_tournament_pool(
            &saved.pool.reference.source_id(),
            &saved.pool.title,
            service::candidates(&saved.pool),
            None,
        )
        .unwrap();
    state
        .collections
        .notebooks
        .save(
            &saved.folder_id,
            &original.entries[0],
            crate::features::collections::notebook::PersonalRecord {
                note: "保留我的练习笔记".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let maps = std::collections::HashMap::from([(
        10,
        json!({"id":10,"beatmapset_id":20,"version":"Expert",
        "difficulty_rating":6.25,"bpm":175,"total_length":164,
        "beatmapset":{"id":20,"title":"Song","artist":"Artist","creator":"Mapper"}}),
    )]);
    assert_eq!(
        state
            .collections
            .save_pool_metadata(&saved.folder_id, &maps)
            .unwrap(),
        1
    );
    let updated = state.collections.folder(&saved.folder_id).unwrap();
    assert_eq!(updated.name, original.name);
    assert_eq!(updated.created_at, original.created_at);
    assert_eq!(updated.entries[0].id, original.entries[0].id);
    assert_eq!(updated.entries[0].title, "Song");
    assert_eq!(updated.entries[0].checksum, original.entries[0].checksum);
    assert_eq!(updated.entries.len(), original.entries.len());
    assert_eq!(updated.stable_sync, original.stable_sync);
    assert_eq!(updated.pending_write, original.pending_write);
    assert_eq!(
        state
            .collections
            .save_pool_metadata(&saved.folder_id, &maps)
            .unwrap(),
        0
    );
    // A later source sync with failed metadata resolution retains the last successful snapshot.
    service::save(standard_pool(standard_document()), &state).unwrap();
    assert_eq!(
        state
            .collections
            .folder(&saved.folder_id)
            .unwrap()
            .pool
            .unwrap()
            .slots[0]
            .metadata,
        updated.pool.unwrap().slots[0].metadata
    );
    drop(state);
    let state = crate::state::AppState::new(directory.path()).unwrap();
    assert_eq!(
        state.collections.folder(&saved.folder_id).unwrap().entries[0].title,
        "Song"
    );
    use sha2::{Digest, Sha256};
    let notes = directory.path().join("collection-notes").join(format!(
        "{:x}.json",
        Sha256::digest(saved.folder_id.as_bytes())
    ));
    let notes: Value = serde_json::from_slice(&std::fs::read(notes).unwrap()).unwrap();
    assert_eq!(notes["records"]["bid:10"]["note"], "保留我的练习笔记");
}

#[test]
fn collection_sync_strictly_replaces_only_the_linked_folder_and_survives_reload() {
    use crate::features::collections::CollectionService;
    let directory = tempfile::tempdir().unwrap();
    let collections = CollectionService::new(directory.path()).unwrap();
    let reference = reference();
    let unrelated = collections.create(&reference.title(), "user").unwrap();
    let pool = |data| {
        rino::decode(
            &serde_json::to_vec(&json!({"success":true,"data":data})).unwrap(),
            &reference,
        )
        .unwrap()
    };
    let original = collections
        .replace_tournament_pool(
            &reference.source_id(),
            &reference.title(),
            service::candidates(&pool(vec![selection(10, "NM", 1), selection(10, "HD", 1)])),
            None,
        )
        .unwrap();
    assert_ne!(original.id, unrelated.id);
    assert_eq!(original.entries.len(), 1);
    collections
        .add_entries(
            &original.id,
            service::candidates(&pool(vec![selection(99, "TB", 1)])),
        )
        .unwrap();
    let next = collections
        .replace_tournament_pool(
            &reference.source_id(),
            "new title",
            service::candidates(&pool(vec![selection(11, "NM", 1)])),
            None,
        )
        .unwrap();
    assert_eq!(next.id, original.id);
    assert_eq!(next.name, original.name);
    assert_eq!(next.created_at, original.created_at);
    assert_eq!(next.entries.len(), 1);
    assert_eq!(next.entries[0].beatmap_id, Some(11));
    assert!(
        collections
            .replace_tournament_pool(&reference.source_id(), "empty", vec![], None)
            .is_err()
    );
    assert_eq!(
        collections.folder(&original.id).unwrap().entries,
        next.entries
    );
    drop(collections);
    let reloaded = CollectionService::new(directory.path()).unwrap();
    assert_eq!(reloaded.folder(&original.id).unwrap().entries, next.entries);
    assert!(reloaded.folder(&unrelated.id).unwrap().entries.is_empty());
}

#[tokio::test]
async fn metadata_failure_is_isolated_and_duplicate_bids_resolve_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = AtomicUsize::new(0);
    let bytes = serde_json::to_vec(&json!({"success":true,"data":[selection(10,"NM",1), selection(10,"HD",1), selection(11,"TB",1)]})).unwrap();
    let pool = service::enrich(rino::decode(&bytes, &reference()).unwrap(), |id| {
        calls.fetch_add(1, Ordering::Relaxed);
        async move {
            if id == 11 {
                return Err(crate::error::CommandError::new("LOOKUP_FAILED", "offline"));
            }
            service::decode_beatmap(
                id,
                json!({"id":id,"mode":"osu","beatmapset_id":20,"version":"Expert",
                "beatmapset":{"id":20,"title":"Song","artist":"Artist","creator":"Mapper"}}),
            )
        }
    })
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(pool.entries[0].beatmap.is_some());
    assert!(pool.entries[1].beatmap.is_some());
    assert_eq!(pool.entries[2].resolution_error.as_deref(), Some("offline"));
    assert_eq!(service::candidates(&pool)[2].beatmap_id, Some(11));
}

#[test]
fn standard_uri_normalizes_source_and_rejects_ambiguous_parameters() {
    let parsed = super::uri::parse("opp://mappool/import?url=https%3A%2F%2FEXAMPLE.com%3A443%2Fpool.json%3Fstage%3Dfinal%26season%3D1").unwrap();
    assert_eq!(
        parsed.url.as_deref(),
        Some("https://example.com/pool.json?stage=final&season=1")
    );
    assert_eq!(
        parsed.source_id(),
        "tournament:opp:https://example.com/pool.json?stage=final&season=1"
    );
    for raw in [
        "opp://mappool/import",
        "opp://mappool/import?url=https://example.com/pool&url=https://example.com/other",
        "opp://mappool/import?url=https://example.com/pool&action=download",
        "opp://mappool/import?url=http://example.com/pool",
        "opp://mappool/import?url=https://user:password@example.com/pool",
        "opp://mappool/import?url=https://127.0.0.1/pool",
        "opp://mappool/import?url=https://example.com/pool%23fragment",
    ] {
        assert!(super::uri::parse(raw).is_err(), "{raw}");
    }
}

fn standard_reference() -> TournamentPoolRef {
    TournamentPoolRef {
        provider: "opp".into(),
        url: Some("https://example.com/pool.json".into()),
        season: String::new(),
        category: String::new(),
    }
}
fn standard_document() -> serde_json::Value {
    json!({"version":1,"title":"通用图池","entries":[{"beatmap_id":10,"selection_type":"NM","position":1}]})
}
fn standard_pool(document: serde_json::Value) -> TournamentPool {
    super::standard::decode(
        &serde_json::to_vec(&document).unwrap(),
        &standard_reference(),
    )
    .unwrap()
}

#[test]
fn standard_minimum_optional_fields_and_multiple_slots() {
    let mut doc = standard_document();
    doc["entries"].as_array_mut().unwrap().push(json!({"beatmap_id":10,"selection_type":"LZ","position":1,"comment":"特殊图位","is_custom":true}));
    let pool = standard_pool(doc.clone());
    assert_eq!(pool.entries.len(), 2);
    assert_eq!(pool.entries[0].comment, "");
    assert!(pool.entries[0].selected_by.is_none());
    assert!(!pool.entries[0].is_custom);
    assert!(pool.entries[1].is_custom);
    doc["entries"][1]["selection_type"] = json!("NM");
    assert!(
        super::standard::decode(&serde_json::to_vec(&doc).unwrap(), &standard_reference()).is_err()
    );
    for (field, value) in [
        ("version", json!(2)),
        ("ruleset", json!("mania")),
        ("title", json!("")),
        ("action", json!("download")),
    ] {
        let mut doc = standard_document();
        doc[field] = value;
        assert!(
            super::standard::decode(&serde_json::to_vec(&doc).unwrap(), &standard_reference())
                .is_err()
        );
    }
    for (field, value) in [
        ("beatmap_id", json!(0)),
        ("position", json!(0)),
        ("selection_type", json!(" ")),
    ] {
        let mut doc = standard_document();
        doc["entries"][0][field] = value;
        assert!(
            super::standard::decode(&serde_json::to_vec(&doc).unwrap(), &standard_reference())
                .is_err()
        );
    }
}

#[test]
fn public_sources_exclude_private_special_and_mapped_addresses() {
    for address in [
        "0.0.0.0",
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "198.18.0.1",
        "192.0.2.1",
        "224.0.0.1",
        "::1",
        "::",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "2001:db8::1",
        "2002:7f00:1::1",
    ] {
        assert!(
            !super::standard::public_ip(address.parse().unwrap()),
            "{address}"
        );
    }
    for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
        assert!(super::standard::public_ip(address.parse().unwrap()));
    }
    for source in [
        "https://localhost/pool",
        "https://host.local/pool",
        "https://example.com:8443/pool",
        "https://[::1]/pool",
        "file:///pool.json",
    ] {
        assert!(super::standard::source_url(source).is_err());
    }
}

#[tokio::test]
async fn uri_import_survives_restart_and_reopens_without_loading_remote_data() {
    let directory = tempfile::tempdir().unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let opened = service::open_or_import(&standard_reference(), &state, || async {
        Ok(standard_pool(standard_document()))
    })
    .await
    .unwrap();
    assert!(!opened.existing);
    let folder = state.collections.folder(&opened.folder_id).unwrap();
    assert!(!folder.stable_sync);
    assert!(!folder.pending_write);
    assert_eq!(folder.pool.as_ref().unwrap().title, "通用图池");
    drop(state);
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let reopened = service::open_or_import(&standard_reference(), &state, || async {
        panic!("reopening must not call the network")
    })
    .await
    .unwrap();
    assert!(reopened.existing);
    assert_eq!(reopened.folder_id, opened.folder_id);
    assert_eq!(state.collections.folder(&opened.folder_id).unwrap(), folder);
}

#[test]
fn sync_commits_membership_and_snapshot_together_and_preserves_notes_on_failure() {
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir().unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let first = service::save(standard_pool(standard_document()), &state).unwrap();
    let original = state.collections.folder(&first.folder_id).unwrap();
    state
        .collections
        .notebooks
        .save(
            &first.folder_id,
            &original.entries[0],
            crate::features::collections::notebook::PersonalRecord {
                note: "练习记录".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let mut changed = standard_document();
    changed["title"] = json!("更新标题");
    changed["entries"][0]["comment"] = json!("新的评语");
    service::save(standard_pool(changed.clone()), &state).unwrap();
    let updated = state.collections.folder(&first.folder_id).unwrap();
    assert_eq!(updated.name, original.name);
    assert_eq!(updated.entries[0].id, original.entries[0].id);
    assert_eq!(updated.pool.as_ref().unwrap().slots[0].comment, "新的评语");
    let temp = directory.path().join("collections-data").join(format!(
        "{:x}.json.tmp",
        Sha256::digest(first.folder_id.as_bytes())
    ));
    std::fs::create_dir(&temp).unwrap();
    changed["entries"][0]["beatmap_id"] = json!(20);
    assert!(service::save(standard_pool(changed), &state).is_err());
    assert_eq!(state.collections.folder(&first.folder_id).unwrap(), updated);
    let mut empty = standard_document();
    empty["entries"] = json!([]);
    assert!(service::save(standard_pool(empty), &state).is_err());
    drop(state);
    let state = crate::state::AppState::new(directory.path()).unwrap();
    assert_eq!(state.collections.folder(&first.folder_id).unwrap(), updated);
    let notes = directory.path().join("collection-notes").join(format!(
        "{:x}.json",
        Sha256::digest(first.folder_id.as_bytes())
    ));
    let notes: serde_json::Value = serde_json::from_slice(&std::fs::read(notes).unwrap()).unwrap();
    assert_eq!(notes["records"]["bid:10"]["note"], "练习记录");
}
