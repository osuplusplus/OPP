//! Opt-in checks: only read the game library; all outputs live in a temporary app directory.
use super::*;
use std::io::Read;

#[test]
#[ignore = "requires OPP_TEST_LAZER_ROOT; reads real data and writes only temporary outputs"]
fn audits_real_lazer_resource_consumers() {
    let source = PathBuf::from(std::env::var("OPP_TEST_LAZER_ROOT").expect("explicit lazer root"));
    let realm_before = fs::read(source.join("client.realm")).unwrap();
    let app = tempfile::tempdir().unwrap();
    let service = LocalAnalysisService::new(app.path()).unwrap();
    service.set_source(LocalClient::Lazer, &source).unwrap();
    let summary = service
        .scan(LocalClient::Lazer, true, Arc::new(|_| {}))
        .unwrap();
    println!(
        "audit library: maps={}, sets={}, skins={}, diagnostics={}",
        summary.beatmap_count,
        summary.beatmap_set_count,
        summary.skin_count,
        summary.diagnostic_count
    );
    let mut sampled = 0;
    for ruleset in [
        Ruleset::Osu,
        Ruleset::Taiko,
        Ruleset::Fruits,
        Ruleset::Mania,
    ] {
        let query = BeatmapQuery {
            client: LocalClient::Lazer,
            rulesets: vec![ruleset],
            limit: 2,
            ..Default::default()
        };
        let page = service.query_beatmaps(query.clone()).unwrap();
        println!("audit mode {ruleset:?}: {} maps", page.total);
        if page.total == 0 {
            continue;
        }
        assert!(page.items.iter().all(|map| map.ruleset == ruleset));
        let next = service
            .query_beatmaps(BeatmapQuery {
                offset: 2,
                ..query.clone()
            })
            .unwrap();
        assert_eq!(next.total, page.total);
        assert!(page.items.iter().all(|map| {
            next.items
                .iter()
                .all(|other| map.resource.resource_id != other.resource.resource_id)
        }));
        let sets = service.query_beatmap_sets(query.clone()).unwrap();
        assert!(!sets.items.is_empty());
        for map in page.items {
            let id = &map.resource.resource_id;
            let detail = service.beatmap_detail(LocalClient::Lazer, id).unwrap();
            assert_eq!(detail.summary.ruleset, ruleset);
            assert!(detail.strains.is_some());
            let search = service
                .query_beatmaps(BeatmapQuery {
                    search: map.title.clone(),
                    ..query.clone()
                })
                .unwrap();
            assert!(search.total > 0);
            let staged = PathBuf::from(service.beatmap_file_path(LocalClient::Lazer, id).unwrap());
            assert!(staged.starts_with(app.path()));
            let bytes = fs::read(&staged).unwrap();
            let md5 = format!("{:x}", Md5::digest(&bytes));
            assert!(
                service
                    .find_beatmap_by_md5(LocalClient::Lazer, &md5)
                    .unwrap()
                    .is_some()
            );
            if let Some(online_id) = map
                .beatmap_id
                .filter(|id| *id > 0 && ruleset == Ruleset::Osu)
            {
                assert!(
                    service
                        .beatmap_path_by_id(LocalClient::Lazer, online_id)
                        .unwrap()
                        .is_some()
                );
            }
            if !detail.audio_file.is_empty() {
                let audio = service.beatmap_audio(LocalClient::Lazer, id).unwrap();
                assert!(!audio.bytes_base64.is_empty());
                assert!(
                    staged
                        .parent()
                        .unwrap()
                        .join(detail.audio_file.replace('\\', "/"))
                        .is_file()
                );
            }
            let background = service.beatmap_background(LocalClient::Lazer, id).unwrap();
            if !detail.background_file.is_empty() {
                assert!(background.is_some());
            }
            let parsed = rosu_pp::Beatmap::from_bytes(&bytes).unwrap();
            for lazer in [false, true] {
                let pp = rosu_pp::Difficulty::new()
                    .lazer(lazer)
                    .calculate(&parsed)
                    .performance()
                    .lazer(lazer)
                    .calculate()
                    .pp();
                assert!(pp.is_finite() && pp >= 0.0);
            }
            let archive = service
                .export_beatmap_set_osz(LocalClient::Lazer, &map.set_key, app.path())
                .unwrap();
            let mut zip = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
            let index = service.require_current_index(LocalClient::Lazer).unwrap();
            let entry = index.resource(id).unwrap();
            for file in entry.lazer_files.as_ref().unwrap() {
                let mut exported = Vec::new();
                zip.by_name(&file.filename.replace('\\', "/"))
                    .unwrap()
                    .read_to_end(&mut exported)
                    .unwrap();
                assert_eq!(sha256(&exported), file.hash);
            }
            if ruleset == Ruleset::Osu && sampled == 0 {
                let skin_page = service
                    .query_skins(SkinQuery {
                        client: LocalClient::Lazer,
                        limit: 1,
                        ..Default::default()
                    })
                    .unwrap();
                if let Some(skin) = skin_page.items.first() {
                    let (songs, skins, selected) = service
                        .stage_lazer_danser_resources(&md5, Some(&skin.name))
                        .unwrap();
                    assert!(songs.starts_with(app.path()));
                    assert!(skins.starts_with(app.path()));
                    assert!(skins.join(selected.unwrap()).join("skin.ini").is_file());
                    let skin_root = service
                        .materialize_lazer_skin(&skin.resource.resource_id)
                        .unwrap();
                    assert!(skin_root.starts_with(app.path()));
                    assert!(skin_root.join("skin.ini").is_file());
                }
                if let Ok(directory) = std::env::var("OPP_SIMILARITY_INDEX") {
                    let runtime = crate::features::similarity::SimilarityRuntime::default();
                    let dataset = runtime.standard_dataset(&directory).unwrap();
                    let target = dataset.analyze_target(&bytes).unwrap();
                    let results = dataset
                        .query(&target, &osu_difficulty_runtime::QueryOptions::default())
                        .unwrap();
                    assert!(!results.is_empty());
                    println!(
                        "audit standard similarity from lazer blob: {} results",
                        results.len()
                    );
                }
            }
            sampled += 1;
        }
    }
    assert!(sampled > 0);
    let candidates = service.music_candidates(None).unwrap();
    assert!(!candidates.is_empty());
    assert!(
        candidates
            .iter()
            .all(|item| item.client == LocalClient::Lazer)
    );
    let playable = candidates
        .iter()
        .filter_map(|c| c.asset.as_ref())
        .filter(|a| a.audio.is_file())
        .count();
    println!(
        "audit music: {} candidates, {playable} playable files",
        candidates.len()
    );
    assert!(playable > 0);
    let incremental = service
        .scan(LocalClient::Lazer, false, Arc::new(|_| {}))
        .unwrap();
    assert_eq!(incremental.beatmap_count, summary.beatmap_count);
    assert_eq!(incremental.skin_count, summary.skin_count);
    drop(service);
    let restored = LocalAnalysisService::new(app.path()).unwrap();
    restored.load_cached_indexes();
    let cached = restored.summary(LocalClient::Lazer).unwrap().unwrap();
    assert_eq!(cached.beatmap_count, summary.beatmap_count);
    assert!(restored.summary(LocalClient::Stable).unwrap().is_none());
    assert!(
        restored
            .query_beatmaps(BeatmapQuery {
                client: LocalClient::Lazer,
                search: "__opp_missing_resource__".into(),
                ..Default::default()
            })
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(fs::read(source.join("client.realm")).unwrap(), realm_before);
    println!(
        "audit complete: {sampled} maps, exports verified by SHA-256; incremental scan and cache restore passed; Realm unchanged"
    );
}
