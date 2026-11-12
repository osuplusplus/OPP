//! collections 功能的集成测试：Stable/Lazer 刷新前置条件、collection.db 编解码、
//! 分享码往返、导入元数据解析、本地缓存失效与数据迁移行为。
use std::{
    collections::{BTreeMap, HashMap},
    fs,
};

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

/// lazer 数据目录损坏（client.realm 非法）时刷新应失败，且不改动已保存的
/// 收藏数据。显式覆盖为合法形态目录，屏蔽开发机上的 lazer 自动探测。
#[tokio::test]
async fn lazer_refresh_reports_invalid_realm_without_changing_collections() {
    let directory = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("client.realm"), b"broken realm").unwrap();
    fs::create_dir(source.path().join("files")).unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    state
        .local_analysis
        .set_source(
            crate::features::local_analysis::LocalClient::Lazer,
            source.path(),
        )
        .unwrap();
    state.collections.create("keep", "test").unwrap();
    let before = fs::read(directory.path().join("collections.json")).unwrap();
    let error = stable::refresh_impl(crate::features::local_analysis::LocalClient::Lazer, &state)
        .await
        .expect_err("broken realm must not appear to be a successful refresh");
    assert_eq!(error.code, "COLLECTION_PARSE_FAILED");
    assert_eq!(
        fs::read(directory.path().join("collections.json")).unwrap(),
        before
    );
}

/// lazer 来源在数据目录有效时可本地读取且只读。
#[test]
fn lazer_collection_source_is_available_and_read_only() {
    let directory = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("client.realm"), []).unwrap();
    fs::create_dir(source.path().join("files")).unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    state
        .local_analysis
        .set_source(
            crate::features::local_analysis::LocalClient::Lazer,
            source.path(),
        )
        .unwrap();
    let lazer = stable::source_statuses(&state)
        .into_iter()
        .find(|s| s.client == crate::features::local_analysis::LocalClient::Lazer)
        .unwrap();
    assert!(lazer.available);
    assert!(lazer.read_only);
    assert!(lazer.message.contains("只读"));
}

/// 构造最小可用的本地谱面摘要，供 lazer 收藏夹解析测试命中本地索引。
fn lazer_local_summary() -> crate::features::local_analysis::LocalBeatmapSummary {
    use crate::domain::Ruleset;
    use crate::features::local_analysis::{LocalBeatmapSummary, LocalResourceRef};
    LocalBeatmapSummary {
        resource: LocalResourceRef {
            resource_id: "lazer:fixture".into(),
            client: crate::features::local_analysis::LocalClient::Lazer,
            content_hash: "cafe".into(),
            logical_path: Some("fixture.osu".into()),
        },
        set_key: "online:2588665".into(),
        set_grouping_inferred: false,
        beatmap_id: Some(5775199),
        beatmap_set_id: Some(2588665),
        title: "tree".into(),
        title_unicode: "tree".into(),
        artist: "artist".into(),
        artist_unicode: "artist".into(),
        creator: "mapper".into(),
        difficulty_name: "Another".into(),
        ruleset: Ruleset::Osu,
        format_version: 14,
        stars: Some(5.25),
        max_pp: Some(300.0),
        max_combo: Some(900),
        bpm: 180.0,
        length_ms: 120_000.0,
        object_count: 800,
        cs: 4.0,
        ar: 9.2,
        od: 8.0,
        hp: 6.0,
        average_nps: 4.0,
        peak_nps: 6.0,
        modified_at: None,
        analysis_status: "ready".into(),
    }
}

/// lazer 收藏镜像：命中本地索引的条目带全元数据，未命中的沿用旧条目或生成占位；
/// 文件夹只读、不参与 stable 同步，ID 与条目跨刷新保持稳定。
#[test]
fn build_lazer_folders_resolves_entries_and_marks_read_only() {
    let realm_collections = vec![
        crate::features::local_analysis::lazer_realm::LazerRealmCollection {
            id: "abc123".into(),
            name: "favourites".into(),
            md5_hashes: vec!["FEDCBA".into(), "012345".into(), "UNKNOWN".into()],
            last_modified: Some("2026-01-02T03:04:05+00:00".into()),
        },
    ];
    let mut resolved = BTreeMap::new();
    resolved.insert("fedcba".into(), lazer_local_summary());
    let mut previous_by_id = HashMap::new();
    previous_by_id.insert(
        "lazer:abc123".into(),
        vec![CollectionEntry {
            id: "keep".into(),
            beatmap_id: Some(42),
            beatmapset_id: Some(7),
            checksum: Some("012345".into()),
            ruleset: Some("osu".into()),
            difficulty_name: "Old".into(),
            title: "Known song".into(),
            artist: "Known artist".into(),
            creator: "Known mapper".into(),
            resolved: true,
        }],
    );

    let folders = lazer::build_lazer_folders(
        &realm_collections,
        &resolved,
        &previous_by_id,
        "2026-02-03T00:00:00+00:00",
    );

    assert_eq!(folders.len(), 1);
    let folder = &folders[0];
    assert_eq!(folder.id, "lazer:abc123");
    assert_eq!(folder.name, "favourites");
    assert_eq!(folder.source, CollectionSource::Lazer);
    assert!(folder.read_only);
    assert!(!folder.participates_in_stable());
    assert_eq!(folder.updated_at, "2026-01-02T03:04:05+00:00");
    assert_eq!(folder.entries.len(), 3);
    // 本地命中：填全元数据并标记已解析。
    let hit = &folder.entries[0];
    assert_eq!(hit.beatmap_id, Some(5775199));
    assert_eq!(hit.title, "tree");
    assert!(hit.resolved);
    // 未命中但旧条目存在：沿用旧条目并刷新校验和大小写。
    let preserved = &folder.entries[1];
    assert_eq!(preserved.id, "keep");
    assert_eq!(preserved.title, "Known song");
    assert_eq!(preserved.checksum.as_deref(), Some("012345"));
    // 两者皆无：生成占位条目。
    let placeholder = &folder.entries[2];
    assert!(!placeholder.resolved);
    assert_eq!(placeholder.checksum.as_deref(), Some("unknown"));
}
/// collection.db 二进制编解码需保留 Unicode 收藏名。
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
/// 本地索引过期时刷新应沿用既有条目的元数据，仅更新 checksum。
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
/// 分享码编解码往返应保留基本信息。
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
/// 紧凑格式下线上条目仅靠谱面/难度 ID 即可往返还原。
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

/// 下载的 .osu 元数据应产出完整候选（含内容 MD5 checksum 与 Unicode 标题）。
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

/// 仅有线上 checksum 的候选不得被误标为本地已解析。
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

/// 批量插入按 beatmap_id → 校验和 → 谱面集 ID 三级去重：
/// 难度条目登记难度 ID 与校验和，占位条目只按谱面集 ID 匹配占位条目。
#[test]
fn add_entries_bulk_deduplicates_across_key_tiers() {
    let directory = tempfile::tempdir().unwrap();
    let state = crate::state::AppState::new(directory.path()).unwrap();
    let folder = state.collections.create("恢复", "test").unwrap();
    let candidate =
        |beatmap_id: Option<i32>, checksum: Option<&str>, set_id: Option<i32>, difficulty: &str| {
            CollectionCandidate {
                beatmap_id,
                beatmapset_id: set_id,
                checksum: checksum.map(str::to_string),
                ruleset: None,
                difficulty_name: difficulty.into(),
                title: "T".into(),
                artist: "A".into(),
                creator: "C".into(),
                local_client: None,
                local_resource_id: None,
            }
        };
    let added = state
        .collections
        .add_entries_bulk(
            &folder.id,
            vec![
                candidate(Some(1), Some("AAA"), Some(10), "Easy"),
                candidate(Some(1), None, Some(10), "重复：按 beatmap_id"),
                candidate(None, Some("aaa"), Some(10), "重复：按校验和忽略大小写"),
                candidate(None, None, Some(11), "占位条目"),
                candidate(None, None, Some(11), "重复：按谱面集 ID 的占位条目"),
                candidate(Some(2), None, Some(10), "Another"),
            ],
        )
        .unwrap();
    assert_eq!(added, 3);
    assert_eq!(
        state.collections.folder(&folder.id).unwrap().entries.len(),
        3
    );
    // 对已有条目再次批量插入：占位与难度各命中一级，仅新的占位集合计入。
    assert_eq!(
        state
            .collections
            .add_entries_bulk(
                &folder.id,
                vec![
                    candidate(None, None, Some(11), "再次占位"),
                    candidate(Some(2), None, Some(10), "再次难度"),
                    candidate(None, None, Some(12), "新的占位集合"),
                ],
            )
            .unwrap(),
        1
    );
}

/// 本地存在性缓存以扫描批次为界，新扫描开始后旧结果作废。
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

/// 收藏数据应迁移为按文件夹分片的原子存储，主文件不再包含条目明细。
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

/// OPP 图池收藏默认不写入 collection.db，显式开启同步后才参与。
#[test]
fn opp_only_pools_are_excluded_from_stable_bytes_until_explicitly_enabled() {
    let directory = tempfile::tempdir().unwrap();
    let service = CollectionService::new(directory.path()).unwrap();
    let ordinary = service.create("Ordinary", "user").unwrap();
    let pool = service
        .replace_tournament_pool(
            "tournament:opp:https://example.com/pool",
            "Pool",
            vec![CollectionCandidate {
                beatmap_id: Some(10),
                beatmapset_id: Some(20),
                checksum: Some("abc".into()),
                ruleset: Some("osu".into()),
                difficulty_name: "Hard".into(),
                title: "Song".into(),
                artist: "Artist".into(),
                creator: "Mapper".into(),
                local_client: None,
                local_resource_id: None,
            }],
            None,
        )
        .unwrap();
    service.rename(&pool.id, "My pool").unwrap();
    assert!(!service.folder(&pool.id).unwrap().pending_write);
    let db = stable::stable_db_for_write(&service.value.lock().unwrap()).0;
    let written = parse_stable_db(&encode_stable_db(&db).unwrap()).unwrap();
    assert_eq!(written.folders.len(), 1);
    assert_eq!(written.folders[0].name, ordinary.name);
    service.enable_stable_sync(&pool.id).unwrap();
    assert!(service.folder(&pool.id).unwrap().pending_write);
    let db = stable::stable_db_for_write(&service.value.lock().unwrap()).0;
    let written = parse_stable_db(&encode_stable_db(&db).unwrap()).unwrap();
    assert_eq!(written.folders.len(), 2);
    assert_eq!(written.folders[1].checksums, vec!["abc"]);
    drop(service);
    let reloaded = CollectionService::new(directory.path()).unwrap();
    assert!(reloaded.folder(&pool.id).unwrap().stable_sync);
}

/// 反序列化缺失 `stable_sync`/`pool` 字段的旧数据时保持既有同步行为。
#[test]
fn legacy_collections_default_to_existing_sync_behavior() {
    let directory = tempfile::tempdir().unwrap();
    let service = CollectionService::new(directory.path()).unwrap();
    let folder = service.create("Legacy", "user").unwrap();
    let mut value = serde_json::to_value(folder).unwrap();
    value.as_object_mut().unwrap().remove("stable_sync");
    value.as_object_mut().unwrap().remove("pool");
    let legacy: CollectionFolder = serde_json::from_value(value).unwrap();
    assert!(legacy.participates_in_stable());
    assert!(legacy.pool.is_none());
}
