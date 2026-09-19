//! Integration against a dataset built by lab's mania-mma-reanalyze and mania-mod-export.
use osu_difficulty_runtime::{ManiaDataset, ManiaGameMod, ManiaQueryOptions};

#[test]
#[ignore = "requires MMA_PACKAGED_DATASET containing a source-free export"]
fn packaged_dataset_queries_without_sources() {
    let root = std::path::PathBuf::from(
        std::env::var_os("MMA_PACKAGED_DATASET").expect("MMA_PACKAGED_DATASET"),
    );
    assert!(!root.join("beatmaps").exists());
    let db = rusqlite::Connection::open_with_flags(
        root.join("mania-metadata.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let ids: Vec<u64> = db
        .prepare("SELECT min(beatmap_id) FROM mania_beatmaps GROUP BY key_count, online_url = ''")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let dataset = ManiaDataset::open(&root).unwrap();
    for id in ids {
        for game_mod in ManiaGameMod::ALL {
            let target = dataset.target_for_id_with_mod(id, game_mod).unwrap();
            assert!(target.pattern.is_some());
            if id >= (1_u64 << 48) {
                assert!(target.metadata.online_url.is_empty());
            }
            let results = dataset
                .query(
                    &target,
                    &ManiaQueryOptions {
                        result_limit: 10,
                        include_same_set: true,
                        candidate_mods: vec![game_mod],
                    },
                )
                .unwrap();
            assert!(!results.is_empty());
            for result in results {
                assert_eq!(result.game_mod, game_mod);
                assert_eq!(result.record.key_count, target.record.key_count);
                let expected = mania_pattern::similarity::distance(
                    target.pattern.as_ref().unwrap(),
                    result.pattern.as_ref().unwrap(),
                );
                assert!((expected.total - result.final_distance as f64).abs() < 2e-6);
            }
        }
    }
}

#[test]
#[ignore = "requires MMA_TEST_DATASET produced by lab"]
fn all_mods_query_and_stored_patterns_agree_with_dataset() {
    let root =
        std::path::PathBuf::from(std::env::var_os("MMA_TEST_DATASET").expect("MMA_TEST_DATASET"));
    let dataset = ManiaDataset::open(&root).expect("dataset opens");
    assert!(dataset.has_pattern_records());
    let mut checked_keys = std::collections::HashSet::new();
    let mut checked = 0;
    let per_key_limit = std::env::var("MMA_QUERY_LIMIT_PER_KEY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let mut key_counts = std::collections::HashMap::<u8, usize>::new();
    for entry in std::fs::read_dir(root.join("beatmaps")).unwrap() {
        let path = entry.unwrap().path();
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
        else {
            continue;
        };
        if !dataset.contains(id) {
            continue;
        }
        let keys = dataset.target_for_id(id).unwrap().record.key_count;
        let count = key_counts.entry(keys).or_default();
        if *count >= per_key_limit {
            continue;
        }
        *count += 1;
        let bytes = std::fs::read(&path).unwrap();
        for game_mod in ManiaGameMod::ALL {
            let target = dataset
                .target_for_id_with_mod(id, game_mod)
                .expect("every variant exists");
            let stored = target.pattern.as_ref().expect("every variant has patterns");
            checked_keys.insert(target.record.key_count);
            assert_eq!(
                dataset.pattern_record(id, game_mod).as_ref(),
                Some(stored),
                "every variant reads its stored pattern"
            );
            let fresh = dataset
                .analyze_target_with_mod(&bytes, Some(9_000_000_000 + id), game_mod)
                .expect("local analysis");
            assert!(
                fresh.pattern.is_none(),
                "key patterns are read from the dataset only"
            );
            let mut expected_raw = target.record;
            expected_raw.beatmap_id = fresh.record.beatmap_id;
            assert_eq!(
                fresh.record, expected_raw,
                "lab and OPP variant features differ"
            );
            let results = dataset
                .query(
                    &target,
                    &ManiaQueryOptions {
                        result_limit: 5,
                        include_same_set: false,
                        candidate_mods: vec![game_mod],
                    },
                )
                .expect("single mod query");
            assert!(!results.is_empty(), "empty {} pool", game_mod.as_str());
            assert!(
                results
                    .iter()
                    .all(|r| r.game_mod == game_mod && r.pattern.is_some())
            );
            let mixed = dataset
                .query(
                    &target,
                    &ManiaQueryOptions {
                        result_limit: 150,
                        include_same_set: true,
                        candidate_mods: ManiaGameMod::ALL.to_vec(),
                    },
                )
                .unwrap();
            assert!(
                mixed
                    .iter()
                    .all(|r| ManiaGameMod::ALL.contains(&r.game_mod))
            );
            assert_eq!(
                mixed
                    .iter()
                    .map(|r| &r.metadata.checksum)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                mixed.len(),
                "mixed pool must deduplicate source hashes"
            );
            checked += 1;
        }
        // An edited file with the same ID must not silently reuse the stored pattern.
        let changed = String::from_utf8_lossy(&bytes);
        let changed = format!("{}\n64,192,999999,1,0,0:0:0:0:\n", changed.trim_end());
        let edited = dataset
            .analyze_target_with_mod(changed.as_bytes(), Some(id), ManiaGameMod::Nm)
            .unwrap();
        assert!(
            edited.pattern.is_none(),
            "an edited source must not reuse or rebuild the stored pattern"
        );
    }
    assert_eq!(checked_keys, std::collections::HashSet::from([4, 6, 7]));
    assert!(checked >= 9);
    eprintln!("verified {checked} variants, all key counts and Mod pools");
}
