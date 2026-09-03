use super::{Timeline, ViewTrainerRequest};
use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::{LocalAnalysisService, LocalClient},
    state::AppState,
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

static TIMELINE_CACHE: LazyLock<Mutex<HashMap<String, (u64, u128, Timeline)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn path(state: &AppState, client: LocalClient, id: &str) -> CommandResult<std::path::PathBuf> {
    Ok(std::path::PathBuf::from(
        state.local_analysis.beatmap_file_path(client, id)?,
    ))
}

pub fn timeline(state: &AppState, client: LocalClient, id: &str) -> CommandResult<Timeline> {
    timeline_for_analysis(&state.local_analysis, client, id)
}

pub fn timeline_for_analysis(
    analysis: &LocalAnalysisService,
    client: LocalClient,
    id: &str,
) -> CommandResult<Timeline> {
    let p = analysis.beatmap_file_path(client, id).map(PathBuf::from)?;
    let metadata = fs::metadata(&p)
        .map_err(|e| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", e.to_string()))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |time| time.as_nanos());
    let cache_key = format!("{}:{client:?}", p.display());
    if let Ok(cache) = TIMELINE_CACHE.lock()
        && let Some((size, stamp, value)) = cache.get(&cache_key)
        && *size == metadata.len()
        && *stamp == modified
    {
        return Ok(value.clone());
    }
    let bytes = std::fs::read(&p)
        .map_err(|e| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", e.to_string()))?;
    let text = crate::features::local_analysis::parser::decode_text(&bytes);
    let mut section = "";
    let mut times = Vec::new();
    let mut bpms = Vec::new();
    let mut duration: f64 = 0.0;
    let mut ar = 5.0;
    let mut od = 5.0;
    let mut cs = 4.0;
    let mut hp = 5.0;
    let mut mode = 0;
    for raw in text.lines() {
        let t = raw.trim();
        if t.starts_with('[') {
            section = t;
            continue;
        }
        if section == "[Difficulty]" {
            if let Some((k, v)) = t.split_once(':') {
                if let Ok(n) = v.trim().parse() {
                    match k {
                        "ApproachRate" => ar = n,
                        "OverallDifficulty" => od = n,
                        "CircleSize" => cs = n,
                        "HPDrainRate" => hp = n,
                        _ => {}
                    }
                }
            }
        }
        if section == "[General]"
            && let Some((k, v)) = t.split_once(':')
            && k.trim().eq_ignore_ascii_case("Mode")
        {
            mode = v.trim().parse().unwrap_or(0);
        }
        if section == "[HitObjects]" && !t.is_empty() && !t.starts_with("//") {
            if let Some(v) = t.split(',').nth(2).and_then(|v| v.parse::<f64>().ok()) {
                times.push(v);
                duration = duration.max(v);
            }
        }
        if section == "[TimingPoints]" && !t.is_empty() && !t.starts_with("//") {
            let p = t.split(',').collect::<Vec<_>>();
            if p.len() >= 2 {
                let uninherited = p.get(6).is_some_and(|value| value.trim() == "1");
                if let (Ok(a), Ok(b)) = (p[0].parse(), p[1].parse::<f64>()) {
                    if b > 0.0 && uninherited {
                        bpms.push((a, 60000.0 / b));
                    }
                }
            }
        }
    }
    let primary_bpm = bpms.first().map(|(_, bpm)| *bpm);
    let (strain_series, strain_section_start_time_ms, strain_section_length_ms) =
        crate::features::local_analysis::parser::calculate_strains(&bytes)
            .map(|analysis| {
                (
                    analysis.series,
                    analysis.section_start_time_ms,
                    analysis.section_length_ms,
                )
            })
            .unwrap_or_default();
    let result = Timeline {
        duration_ms: duration,
        object_count: times.len(),
        bpm_segments: bpms,
        ar,
        od,
        cs,
        hp,
        mode,
        primary_bpm,
        strain_series,
        strain_section_start_time_ms,
        strain_section_length_ms,
    };
    if let Ok(mut cache) = TIMELINE_CACHE.lock() {
        if cache.len() > 128 {
            cache.clear();
        }
        cache.insert(cache_key, (metadata.len(), modified, result.clone()));
    }
    Ok(result)
}

fn ar_to_ms(ar: f64) -> f64 {
    if ar <= 5.0 {
        1800.0 - ar * 120.0
    } else {
        1200.0 - (ar - 5.0) * 150.0
    }
}

fn ms_to_ar(ms: f64) -> f32 {
    let ar = if ms >= 1200.0 {
        (1800.0 - ms) / 120.0
    } else {
        5.0 + (1200.0 - ms) / 150.0
    };
    ar.clamp(0.0, 11.0) as f32
}

fn scale_ar(ar: f32, rate: f64) -> f32 {
    ms_to_ar(ar_to_ms(ar as f64) / rate)
}

fn scale_od(od: f32, rate: f64) -> f32 {
    let value = ((79.5 - (-6.0 * od as f64 + 79.5) / rate) / 6.0).clamp(0.0, 11.0);
    (value * 10.0).round() as f32 / 10.0
}

pub fn resolve_request(
    state: &AppState,
    mut request: ViewTrainerRequest,
) -> CommandResult<ViewTrainerRequest> {
    resolve_request_with_analysis(&state.local_analysis, request)
}

pub fn resolve_request_with_analysis(
    analysis: &LocalAnalysisService,
    mut request: ViewTrainerRequest,
) -> CommandResult<ViewTrainerRequest> {
    let source = timeline_for_analysis(analysis, request.client, &request.resource_id)?;
    if request.bpm_locked {
        let target = request
            .target_bpm
            .ok_or_else(|| CommandError::new("INVALID_TARGET_BPM", "请填写目标 BPM"))?;
        let primary = source
            .primary_bpm
            .ok_or_else(|| CommandError::new("NO_SOURCE_BPM", "谱面没有可用的主 BPM"))?;
        request.rate = target / primary;
    }
    if !request.lock_ar && request.scale_ar && source.mode != 1 && source.mode != 3 {
        request.ar = scale_ar(source.ar, request.rate);
    }
    if !request.lock_od && request.scale_od {
        request.od = scale_od(source.od, request.rate);
    }
    if !request.lock_cs {
        request.cs = source.cs;
    }
    if !request.lock_hp {
        request.hp = source.hp;
    }
    validate(&request)?;
    Ok(request)
}

pub fn validate(r: &ViewTrainerRequest) -> CommandResult<()> {
    if !(0.75..=2.0).contains(&r.rate) {
        return Err(CommandError::new(
            "INVALID_RATE",
            "Rate 必须在 0.75× 到 2.00× 之间",
        ));
    }
    if !(0.0..=11.0).contains(&r.ar)
        || !(0.0..=11.0).contains(&r.od)
        || !(0.0..=10.0).contains(&r.cs)
        || !(0.0..=10.0).contains(&r.hp)
    {
        return Err(CommandError::new(
            "INVALID_DIFFICULTY",
            "AR/OD 必须在 0 到 11 之间，CS/HP 必须在 0 到 10 之间",
        ));
    }
    if r.bpm_locked
        && r.target_bpm
            .is_none_or(|bpm| !(1.0..=1000.0).contains(&bpm))
    {
        return Err(CommandError::new(
            "INVALID_TARGET_BPM",
            "目标 BPM 必须在 1 到 1000 之间",
        ));
    }
    let s = r.start_time_ms.unwrap_or(0.0);
    if r.end_time_ms.is_some_and(|e| e <= s) {
        return Err(CommandError::new(
            "INVALID_TIME_RANGE",
            "结束时间必须大于开始时间",
        ));
    }
    if let (Some(a), Some(b)) = (r.min_bpm, r.max_bpm) {
        if a > b {
            return Err(CommandError::new(
                "INVALID_BPM_RANGE",
                "最低 BPM 不能高于最高 BPM",
            ));
        }
    }
    Ok(())
}

pub fn import_staged(
    state: &AppState,
    client: LocalClient,
    resource_id: &str,
    staged_path: &str,
) -> CommandResult<String> {
    let source = PathBuf::from(
        state
            .local_analysis
            .beatmap_file_path(client, resource_id)?,
    );
    import_staged_at_path(source, staged_path, None)
}

pub fn import_staged_at_path(
    source: PathBuf,
    staged_path: &str,
    target_root: Option<PathBuf>,
) -> CommandResult<String> {
    let root = std::env::temp_dir().join("opp").join("view-trainer");
    let staged = PathBuf::from(staged_path);
    let staged = staged
        .canonicalize()
        .map_err(|e| CommandError::new("VIEW_TRAINER_STAGE_NOT_FOUND", e.to_string()))?;
    let root = root
        .canonicalize()
        .map_err(|e| CommandError::new("VIEW_TRAINER_STAGE_NOT_FOUND", e.to_string()))?;
    if !staged.starts_with(&root) || !staged.is_dir() {
        return Err(CommandError::new(
            "VIEW_TRAINER_INVALID_STAGE",
            "只能导入 View Trainer 生成的暂存目录",
        ));
    }
    let songs = target_root.unwrap_or_else(|| {
        source
            .parent()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_default()
    });
    if songs.as_os_str().is_empty() || !songs.is_dir() {
        return Err(CommandError::new(
            "LOCAL_RESOURCE_NOT_FOUND",
            "osu! Songs 目录不可用",
        ));
    }
    let import_id = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let target = songs.join(format!("OPP Trainer - Imported - {import_id}"));
    let temporary_target = songs.join(format!(".opp-trainer-import-{import_id}.tmp"));
    fs::create_dir_all(&temporary_target)
        .map_err(|e| CommandError::new("VIEW_TRAINER_IMPORT_FAILED", e.to_string()))?;
    // Assets are intentionally omitted from the fast preview stage. On import,
    // copy only files referenced by the generated chart instead of every large
    // video/skin file in the source directory; this keeps osu! editor indexing
    // responsive for large beatmap sets.
    let source_dir = source
        .parent()
        .ok_or_else(|| CommandError::new("LOCAL_RESOURCE_NOT_FOUND", "谱面目录不可用"))?;
    let generated_map = staged.join("OPP Trainer.osu");
    let generated_text = fs::read_to_string(&generated_map).unwrap_or_default();
    let mut referenced = std::collections::HashSet::<String>::new();
    for raw in generated_text.lines() {
        let line = raw.trim();
        if let Some(value) = line.strip_prefix("AudioFilename:") {
            referenced.insert(value.trim().replace(['\\', '/'], ""));
        } else if line.starts_with("0,0,") || line.starts_with("Video,") {
            if let Some(value) = line.split('"').nth(1) {
                referenced.insert(value.replace(['\\', '/'], ""));
            }
        } else if line.contains(",\"") {
            if let Some(value) = line.split('"').nth(1) {
                referenced.insert(value.replace(['\\', '/'], ""));
            }
        }
    }
    // Storyboard files can reference additional sprites/audio by filename.
    // Resolve those references transitively while staying inside source_dir.
    let mut pending = referenced.iter().cloned().collect::<Vec<_>>();
    while let Some(name) = pending.pop() {
        if !name.to_ascii_lowercase().ends_with(".osb") {
            continue;
        }
        let path = source_dir.join(&name);
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for value in text.split('"').skip(1).step_by(2) {
            let asset = value.replace(['\\', '/'], "");
            if !asset.is_empty() && referenced.insert(asset.clone()) {
                pending.push(asset);
            }
        }
    }
    for name in referenced {
        if name.is_empty() || name.eq_ignore_ascii_case("opp-trainer-audio.mp3") {
            continue;
        }
        let path = source_dir.join(&name);
        if path.is_file() {
            fs::copy(&path, temporary_target.join(&name))
                .map_err(|e| CommandError::new("VIEW_TRAINER_IMPORT_FAILED", e.to_string()))?;
        }
    }
    for entry in fs::read_dir(&staged)
        .map_err(|e| CommandError::new("VIEW_TRAINER_IMPORT_FAILED", e.to_string()))?
    {
        let entry =
            entry.map_err(|e| CommandError::new("VIEW_TRAINER_IMPORT_FAILED", e.to_string()))?;
        if entry.path().is_file() {
            fs::copy(entry.path(), temporary_target.join(entry.file_name()))
                .map_err(|e| CommandError::new("VIEW_TRAINER_IMPORT_FAILED", e.to_string()))?;
        }
    }
    fs::rename(&temporary_target, &target)
        .map_err(|e| CommandError::new("VIEW_TRAINER_IMPORT_FAILED", e.to_string()))?;
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{scale_ar, scale_od};

    #[test]
    fn rate_scaling_matches_osu_windows() {
        assert!((scale_ar(7.0, 1.5) - 9.0).abs() < 0.01);
        assert!((scale_od(7.0, 1.5) - 9.1).abs() < 0.01);
    }

    #[test]
    fn rate_scaling_is_clamped_to_eleven() {
        assert_eq!(scale_ar(10.0, 2.0), 11.0);
        assert!(scale_od(10.0, 2.0) <= 11.0);
    }
}
