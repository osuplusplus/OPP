use std::{collections::HashMap, fs};

use md5::{Digest, Md5};

use super::*;
use super::{
    downloads::parse_downloaded_beatmap,
    service::{LocalPresenceCacheEntry, cached_local_presence, candidate_to_entry},
    share::{SharePayload, decode_share, encode_share},
    stable::{
        StableCollection, StableDb, encode_stable_db, parse_stable_db, stable_collection_entry,
    },
};
#[test]
fn stable_db_round_trip_unicode() {
    let db = StableDb {
        version: 20200101,
        folders: vec![StableCollection {
            name: "测试合集".into(),
            checksums: vec!["abc".into()],
        }],
    };
    assert_eq!(
        parse_stable_db(&encode_stable_db(&db).unwrap())
            .unwrap()
            .folders[0]
            .name,
        "测试合集"
    );
}
#[test]
fn stable_refresh_preserves_known_entry_when_local_index_is_stale() {
    let previous = CollectionEntry {
        id: "entry-1".into(),
        beatmap_id: Some(123),
        beatmapset_id: Some(456),
        checksum: Some("ABCDEF".into()),
        ruleset: Some("osu".into()),
        difficulty_name: "Insane".into(),
        title: "Known song".into(),
        artist: "Known artist".into(),
        creator: "Known mapper".into(),
        resolved: true,
    };

    let refreshed = stable_collection_entry("abcdef".into(), None, Some(&previous));

    assert_eq!(refreshed.id, previous.id);
    assert_eq!(refreshed.title, previous.title);
    assert_eq!(refreshed.checksum.as_deref(), Some("abcdef"));
    assert!(refreshed.resolved);
}
#[test]
fn share_round_trip() {
    let payload = SharePayload {
        version: 1,
        name: "A".into(),
        creator: "B".into(),
        created_at: "x".into(),
        exported_at: "y".into(),
        entries: Vec::new(),
    };
    assert_eq!(
        decode_share(&encode_share(&payload).unwrap()).unwrap().name,
        "A"
    );
}
#[test]
fn compact_share_preserves_online_difficulty_ids() {
    let entry = CollectionEntry {
        id: "local".into(),
        beatmap_id: Some(1_234_567),
        beatmapset_id: Some(765_432),
        checksum: None,
        ruleset: Some("osu".into()),
        difficulty_name: "Ignored in compact form".into(),
        title: "Large repeated display data is omitted".into(),
        artist: "Artist".into(),
        creator: "Mapper".into(),
        resolved: true,
    };
    let payload = SharePayload {
        version: 1,
        name: "Massive list".into(),
        creator: "OPP".into(),
        created_at: "x".into(),
        exported_at: "y".into(),
        entries: vec![entry],
    };
    let decoded = decode_share(&encode_share(&payload).unwrap()).unwrap();
    assert_eq!(decoded.entries[0].beatmap_id, Some(1_234_567));
    assert_eq!(decoded.entries[0].beatmapset_id, Some(765_432));
}

#[test]
fn downloaded_beatmap_metadata_supplies_collection_checksum() {
    let bytes = br#"osu file format v14

[General]
Mode:3

[Metadata]
Title:Test Song
TitleUnicode:Unicode Title
Artist:Test Artist
Creator:Mapper
Version:Another
BeatmapID:456
BeatmapSetID:123
"#;
    let (beatmap_id, parsed) = parse_downloaded_beatmap(bytes).unwrap();
    assert_eq!(beatmap_id, 456);
    assert_eq!(parsed.beatmapset_id, Some(123));
    assert_eq!(parsed.ruleset.as_deref(), Some("mania"));
    assert_eq!(parsed.title, "Unicode Title");
    assert_eq!(parsed.difficulty_name, "Another");
    assert_eq!(parsed.checksum, format!("{:x}", Md5::digest(bytes)));
}

#[test]
fn online_checksum_does_not_claim_the_beatmap_is_local() {
    let online = candidate_to_entry(CollectionCandidate {
        beatmap_id: Some(5775199),
        beatmapset_id: Some(2588665),
        checksum: Some("05b5b08930762a1952f37db991e16c62".into()),
        ruleset: Some("osu".into()),
        difficulty_name: "test".into(),
        title: "tree".into(),
        artist: "artist".into(),
        creator: "mapper".into(),
        local_client: None,
        local_resource_id: None,
    });
    assert!(!online.resolved);
}

#[test]
fn local_presence_cache_expires_after_a_new_scan() {
    let mut cache = HashMap::new();
    cache.insert(
        "abc".into(),
        LocalPresenceCacheEntry {
            present: true,
            scan_at: Some("scan-1".into()),
        },
    );
    assert_eq!(
        cached_local_presence(&cache, "abc", &Some("scan-1".into())),
        Some(true)
    );
    assert_eq!(
        cached_local_presence(&cache, "abc", &Some("scan-2".into())),
        None
    );
}

#[test]
fn collections_are_migrated_to_atomic_folder_shards() {
    let directory = tempfile::tempdir().expect("app data");
    let service = CollectionService::new(directory.path()).expect("service");
    let folder = service
        .create("Large library", "tester")
        .expect("create folder");
    let second = service.create("Second", "tester").expect("second folder");
    let metadata = fs::read_to_string(directory.path().join("collections.json")).expect("metadata");
    assert!(!metadata.contains("Large library"));
    assert_eq!(
        fs::read_dir(directory.path().join("collections-data"))
            .expect("shards")
            .count(),
        2
    );

    let reloaded = CollectionService::new(directory.path()).expect("reload");
    let snapshot = reloaded.snapshot(Vec::new()).expect("snapshot");
    assert_eq!(snapshot.folders, vec![folder, second]);
}
