use super::*;

#[test]
fn progress_parser_accepts_danser_percent_lines() {
    assert_eq!(parse_progress("Recording progress: 42%"), Some(42));
    assert_eq!(parse_progress("no progress"), None);
}

#[test]
fn preferences_reject_conflicting_mod_formats() {
    let input = DanserRenderPreferences {
        mods: "HD".into(),
        mods2: "[]".into(),
        ..Default::default()
    };
    assert!(validate_preferences(&input).is_err());
}

#[test]
fn safe_output_stem_removes_path_characters() {
    assert_eq!(safe_stem("C:\\Replays\\a:b?.osr"), "a_b");
}

#[test]
fn failure_detail_prefers_the_panic_over_a_trailing_timestamp() {
    let lines = VecDeque::from([
        "2026/08/13 17:17:57 panic: mkdir videos\\C:: invalid path".into(),
        "2026/08/13 17:17:57 goroutine 35 [running]".into(),
        "2026/08/13 17:17:57".into(),
    ]);
    assert!(failure_detail(&lines, "exit 2").contains("panic: mkdir"));
}

#[test]
fn runtime_patch_points_danser_at_the_replay_osu_installation() {
    let preferences = DanserRenderPreferences {
        settings_patch: r#"{"Graphics":{"Width":1280}}"#.into(),
        ..Default::default()
    };
    let task = DanserTask {
        id: "test".into(),
        client: LocalClient::Stable,
        replay_path: "D:\\osu!\\Replays\\play.osr".into(),
        preferences,
        lazer_stage: None,
    };
    let patch: serde_json::Value = serde_json::from_str(
        &runtime_settings_patch(&task).expect("create runtime settings patch"),
    )
    .expect("parse runtime settings patch");
    assert_eq!(patch["General"]["OsuSongsDir"], "D:\\osu!\\Songs");
    assert_eq!(patch["General"]["OsuReplaysDir"], "D:\\osu!\\Replays");
    assert_eq!(patch["General"]["OsuSkinsDir"], "D:\\osu!\\Skins");
    assert_eq!(patch["Graphics"]["Width"], 1280);
    assert_eq!(patch["Recording"]["FrameWidth"], 1920);
    assert_eq!(patch["Recording"]["FPS"], 60);
    assert_eq!(patch["Recording"]["Encoder"], "libx264");
    assert_eq!(patch["Recording"]["libx264"]["CRF"], 14);
    assert_eq!(patch["Recording"]["MotionBlur"]["Enabled"], false);
}

#[test]
fn runtime_patch_points_lazer_tasks_at_the_staged_directories() {
    let preferences = DanserRenderPreferences::default();
    let task = DanserTask {
        id: "test".into(),
        client: LocalClient::Lazer,
        replay_path: "/home/user/.local/share/osu/replays/play.osr".into(),
        preferences,
        lazer_stage: Some(DanserLazerStage {
            songs_dir: std::path::PathBuf::from("/cache/materialized-sets/set-abc"),
            skins_root: std::path::PathBuf::from("/cache/danser-stage/Skins"),
        }),
    };
    let patch: serde_json::Value = serde_json::from_str(
        &runtime_settings_patch(&task).expect("create runtime settings patch"),
    )
    .expect("parse runtime settings patch");
    assert_eq!(
        patch["General"]["OsuSongsDir"],
        "/cache/materialized-sets/set-abc"
    );
    assert_eq!(patch["General"]["OsuSkinsDir"], "/cache/danser-stage/Skins");
}

#[test]
fn runtime_patch_uses_encoder_specific_quality_and_motion_blur() {
    let preferences = DanserRenderPreferences {
        encoder: "h264_nvenc".into(),
        quality: 20,
        motion_blur: true,
        motion_blur_oversample: 8,
        ..Default::default()
    };
    let task = DanserTask {
        id: "test".into(),
        client: LocalClient::Stable,
        replay_path: "D:\\osu!\\Replays\\play.osr".into(),
        preferences,
        lazer_stage: None,
    };
    let patch: serde_json::Value = serde_json::from_str(
        &runtime_settings_patch(&task).expect("create runtime settings patch"),
    )
    .expect("parse runtime settings patch");
    assert_eq!(patch["Recording"]["h264_nvenc"]["CQ"], 20);
    assert_eq!(patch["Recording"]["MotionBlur"]["Enabled"], true);
    assert_eq!(patch["Recording"]["MotionBlur"]["BlendFrames"], 12);
}

#[test]
fn detects_ffmpeg_in_the_danser_distribution_subdirectory() {
    let directory = tempfile::tempdir().expect("temp directory");
    let executable = directory.path().join("danser-cli.exe");
    std::fs::write(&executable, []).expect("create danser executable");
    std::fs::create_dir(directory.path().join("ffmpeg")).expect("create ffmpeg directory");
    std::fs::write(directory.path().join("ffmpeg").join("ffmpeg.exe"), [])
        .expect("create ffmpeg executable");
    assert!(ffmpeg_available(&executable));
}
