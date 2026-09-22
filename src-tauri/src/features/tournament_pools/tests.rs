use serde_json::{Value, json};

use super::{models::*, rino, service, uri};

fn reference() -> TournamentPoolRef {
    TournamentPoolRef {
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
        )
        .unwrap();
    assert_eq!(next.id, original.id);
    assert_eq!(next.name, original.name);
    assert_eq!(next.created_at, original.created_at);
    assert_eq!(next.entries.len(), 1);
    assert_eq!(next.entries[0].beatmap_id, Some(11));
    assert!(
        collections
            .replace_tournament_pool(&reference.source_id(), "empty", vec![])
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
