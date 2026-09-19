use super::{catalog::build_queue, models::QueueRequest, service::MusicRuntime};
use crate::features::{
    collections::CollectionEntry,
    local_analysis::{LocalClient, MusicAsset, MusicCandidate},
};

fn request() -> QueueRequest {
    serde_json::from_str("{}").unwrap()
}
fn candidate(
    client: LocalClient,
    set: &str,
    online: Option<i32>,
    map: &str,
    checksum: &str,
) -> MusicCandidate {
    MusicCandidate {
        client,
        resource_id: map.into(),
        set_key: set.into(),
        set_id: online,
        beatmap_id: None,
        checksum: Some(checksum.into()),
        title: "same title".into(),
        artist: "same artist".into(),
        matches: true,
        asset: Some(MusicAsset {
            client,
            resource_id: map.into(),
            root: "root".into(),
            audio: format!("root/{map}.mp3").into(),
            artwork: None,
            beatmap: format!("root/{map}.osu").into(),
        }),
    }
}
#[test]
fn merges_online_sets_across_clients_but_not_different_mappers() {
    let (tracks, _) = build_queue(
        vec![
            candidate(LocalClient::Stable, "a", Some(1), "easy", "1"),
            candidate(LocalClient::Lazer, "realm:a", Some(1), "hard", "2"),
            candidate(LocalClient::Stable, "b", Some(2), "same-song", "3"),
        ],
        &request(),
        None,
    );
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].assets.len(), 2);
    assert_eq!(tracks[0].info.clients.len(), 2);
}
#[test]
fn unsubmitted_sets_require_checksum_evidence_and_merge_transitively() {
    let (tracks, _) = build_queue(
        vec![
            candidate(LocalClient::Stable, "a", None, "a", "proof-a"),
            candidate(LocalClient::Lazer, "b", None, "b1", "proof-a"),
            candidate(LocalClient::Lazer, "b", None, "b2", "proof-b"),
            candidate(LocalClient::Stable, "c", None, "c", "proof-b"),
            candidate(LocalClient::Stable, "different", None, "d", "different"),
        ],
        &request(),
        None,
    );
    assert_eq!(tracks.len(), 2);
    assert!(tracks.iter().any(|t| t.assets.len() == 4));
}
#[test]
fn selected_difficulty_is_first_and_other_client_is_a_fallback() {
    let mut req = request();
    req.resource_id = Some("chosen".into());
    req.client = Some(LocalClient::Lazer);
    let (tracks, _) = build_queue(
        vec![
            candidate(LocalClient::Stable, "a", Some(1), "other", "1"),
            candidate(LocalClient::Lazer, "a", Some(1), "chosen", "2"),
        ],
        &req,
        None,
    );
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].assets[0].resource_id, "chosen");
    assert_eq!(tracks[0].assets[1].client, LocalClient::Stable);
}
#[test]
fn filtered_snapshot_keeps_unmatched_alternate_sources() {
    let a = candidate(LocalClient::Stable, "a", Some(1), "match", "1");
    let mut b = candidate(LocalClient::Lazer, "b", Some(1), "backup", "2");
    b.matches = false;
    let mut c = candidate(LocalClient::Stable, "c", Some(2), "excluded", "3");
    c.matches = false;
    let (tracks, _) = build_queue(vec![a, b, c], &request(), None);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].assets.len(), 2);
}
#[test]
fn collection_resolves_checksum_and_reports_unavailable_entries() {
    let entry = |hash: &str| CollectionEntry {
        id: hash.into(),
        beatmap_id: None,
        beatmapset_id: None,
        checksum: Some(hash.into()),
        ruleset: None,
        difficulty_name: "".into(),
        title: "".into(),
        artist: "".into(),
        creator: "".into(),
        resolved: false,
    };
    let (tracks, missing) = build_queue(
        vec![candidate(LocalClient::Stable, "a", Some(1), "a", "abcd")],
        &request(),
        Some(&[entry("ABCD"), entry("missing")]),
    );
    assert_eq!(tracks.len(), 1);
    assert_eq!(missing, 1);
}
#[test]
fn source_path_is_rechecked_and_cannot_escape_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    let mut asset = candidate(LocalClient::Stable, "a", None, "a", "a")
        .asset
        .unwrap();
    asset.root = root.path().into();
    assert!(asset.checked_path(outside.path()).is_err());
    let inside = root.path().join("audio.wav");
    std::fs::write(&inside, b"wave").unwrap();
    assert!(asset.checked_path(&inside).is_ok());
    std::fs::remove_file(&inside).unwrap();
    assert!(asset.checked_path(&inside).is_err());
}

#[test]
fn same_audio_within_a_set_is_only_one_source_but_distinct_sets_survive() {
    let a = candidate(LocalClient::Stable, "a", Some(1), "audio", "a");
    let mut b = a.clone();
    b.resource_id = "another-difficulty".into();
    let mut c = a.clone();
    c.set_id = Some(2);
    let (tracks, _) = build_queue(vec![a, b, c], &request(), None);
    assert_eq!(tracks.len(), 2);
    assert!(tracks.iter().all(|t| t.assets.len() == 1));
}

#[test]
fn set_only_collection_entry_matches_without_hiding_missing_difficulties() {
    let mut entry: CollectionEntry = serde_json::from_value(serde_json::json!({
        "id": "set", "beatmap_id": null, "beatmapset_id": 1, "checksum": null,
        "ruleset": null, "difficulty_name": "", "title": "", "artist": "",
        "creator": "", "resolved": false
    }))
    .unwrap();
    let maps = vec![candidate(LocalClient::Stable, "a", Some(1), "a", "a")];
    assert_eq!(
        build_queue(maps.clone(), &request(), Some(&[entry.clone()])).1,
        0
    );
    entry.checksum = Some("missing-difficulty".into());
    let (tracks, missing) = build_queue(maps, &request(), Some(&[entry]));
    assert!(tracks.is_empty());
    assert_eq!(missing, 1);
}
#[test]
fn restart_never_autoplays_and_recovers_from_corrupt_state() {
    let root = tempfile::tempdir().unwrap();
    let runtime = MusicRuntime::new(root.path()).unwrap();
    let mut state = runtime.snapshot().unwrap();
    state.playing = true;
    state.volume = 0.3;
    super::service::write_json(&runtime.directory.join("state.json"), &state).unwrap();
    let restored = MusicRuntime::new(root.path()).unwrap().snapshot().unwrap();
    assert!(!restored.playing);
    assert_eq!(restored.volume, 0.3);
    std::fs::write(runtime.directory.join("state.json"), b"broken").unwrap();
    assert!(
        !MusicRuntime::new(root.path())
            .unwrap()
            .snapshot()
            .unwrap()
            .playing
    );
}
