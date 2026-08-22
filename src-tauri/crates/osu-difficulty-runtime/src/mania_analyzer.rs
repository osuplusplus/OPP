use std::{cmp::Ordering, collections::VecDeque};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    MANIA_ANALYZER_VERSION, ManiaBaseFeatures, ManiaBeatmapMetadata, ManiaDifficultyVector,
    ManiaGameMod, ManiaModeFamily, ManiaPattern, ManiaRawFeatureRecord, ManiaStyleVector,
};

const ROW_TOLERANCE_MS: f64 = 2.0;
const ENTROPY_WINDOW_MS: f64 = 750.0;
const SECTION_MS: f64 = 400.0;
const SECTION_DECAY: f64 = 0.9;

#[derive(Debug, Error)]
pub enum ManiaAnalyzeError {
    #[error("invalid osu! file: {0}")]
    Invalid(String),
    #[error("only osu!mania mode is supported (found mode {0})")]
    UnsupportedMode(u8),
    #[error("only 4K, 6K, and 7K are supported (found {0}K)")]
    UnsupportedKeyCount(u8),
}

#[derive(Debug, Clone, Copy)]
struct HitObject {
    column: u8,
    start: f64,
    end: f64,
    long_note: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Tap,
    LongHead,
    LongTail,
}

#[derive(Debug, Clone, Copy)]
struct Event {
    time: f64,
    column: u8,
    kind: EventKind,
    end: f64,
}

#[derive(Debug, Clone)]
struct TimelineRow {
    time: f64,
    head_mask: u8,
    normal_mask: u8,
    ln_head_mask: u8,
    release_mask: u8,
    active_hold_mask: u8,
}

impl TimelineRow {
    fn head_count(&self) -> u32 {
        self.head_mask.count_ones()
    }
}

#[derive(Debug, Clone)]
struct ParsedManiaBeatmap {
    beatmap_id: u64,
    beatmapset_id: u64,
    artist: String,
    title: String,
    version: String,
    creator: String,
    key_count: u8,
    bpm: f64,
    sv_changes: usize,
    objects: Vec<HitObject>,
}

#[derive(Debug, Clone, Copy)]
struct StreamConfig {
    burst_tau: f64,
    sustain_tau: f64,
    burst_mix: f64,
}

const STREAM_CONFIGS: [StreamConfig; 8] = [
    StreamConfig {
        burst_tau: 220.0,
        sustain_tau: 1600.0,
        burst_mix: 0.78,
    },
    StreamConfig {
        burst_tau: 260.0,
        sustain_tau: 2200.0,
        burst_mix: 0.80,
    },
    StreamConfig {
        burst_tau: 300.0,
        sustain_tau: 1800.0,
        burst_mix: 0.88,
    },
    StreamConfig {
        burst_tau: 260.0,
        sustain_tau: 2400.0,
        burst_mix: 0.82,
    },
    StreamConfig {
        burst_tau: 450.0,
        sustain_tau: 3200.0,
        burst_mix: 0.70,
    },
    StreamConfig {
        burst_tau: 1200.0,
        sustain_tau: 10000.0,
        burst_mix: 0.58,
    },
    StreamConfig {
        burst_tau: 450.0,
        sustain_tau: 4000.0,
        burst_mix: 0.65,
    },
    StreamConfig {
        burst_tau: 30000.0,
        sustain_tau: 120000.0,
        burst_mix: 0.35,
    },
];

#[derive(Debug, Clone, Copy, Default)]
struct StreamState {
    burst: f64,
    sustain: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ManiaAnalyzer;

impl ManiaAnalyzer {
    pub const fn new() -> Self {
        Self
    }

    pub fn analyze_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<(ManiaBeatmapMetadata, ManiaRawFeatureRecord), ManiaAnalyzeError> {
        self.analyze_bytes_with_source_id_and_mod(bytes, None, ManiaGameMod::Nm)
    }

    /// Analyze a downloaded beatmap while treating its source filename ID as
    /// authoritative. Some old ranked files contain stale or zero BeatmapID
    /// metadata even though their download ID is valid and unique.
    pub fn analyze_bytes_with_beatmap_id(
        &self,
        bytes: &[u8],
        beatmap_id: u64,
    ) -> Result<(ManiaBeatmapMetadata, ManiaRawFeatureRecord), ManiaAnalyzeError> {
        if beatmap_id == 0 {
            return Err(ManiaAnalyzeError::Invalid(
                "source BeatmapID must be positive".into(),
            ));
        }
        self.analyze_bytes_with_source_id_and_mod(bytes, Some(beatmap_id), ManiaGameMod::Nm)
    }

    pub fn analyze_bytes_with_mod(
        &self,
        bytes: &[u8],
        game_mod: ManiaGameMod,
    ) -> Result<(ManiaBeatmapMetadata, ManiaRawFeatureRecord), ManiaAnalyzeError> {
        self.analyze_bytes_with_source_id_and_mod(bytes, None, game_mod)
    }

    pub fn analyze_bytes_with_beatmap_id_and_mod(
        &self,
        bytes: &[u8],
        beatmap_id: u64,
        game_mod: ManiaGameMod,
    ) -> Result<(ManiaBeatmapMetadata, ManiaRawFeatureRecord), ManiaAnalyzeError> {
        if beatmap_id == 0 {
            return Err(ManiaAnalyzeError::Invalid(
                "source BeatmapID must be positive".into(),
            ));
        }
        self.analyze_bytes_with_source_id_and_mod(bytes, Some(beatmap_id), game_mod)
    }

    fn analyze_bytes_with_source_id_and_mod(
        &self,
        bytes: &[u8],
        source_beatmap_id: Option<u64>,
        game_mod: ManiaGameMod,
    ) -> Result<(ManiaBeatmapMetadata, ManiaRawFeatureRecord), ManiaAnalyzeError> {
        let mut parsed = parse_beatmap(bytes)?;
        if let Some(beatmap_id) = source_beatmap_id {
            parsed.beatmap_id = beatmap_id;
        }
        parsed.apply_clock_rate(game_mod.clock_rate());
        let (difficulty, style, base, family, dominant) = analyze_parsed(&parsed)?;
        let checksum = hex::encode(Sha256::digest(bytes));
        let metadata = ManiaBeatmapMetadata {
            beatmap_id: parsed.beatmap_id,
            beatmapset_id: parsed.beatmapset_id,
            checksum,
            artist: parsed.artist,
            title: parsed.title,
            version: parsed.version,
            creator: parsed.creator,
            online_url: format!("https://osu.ppy.sh/b/{}", parsed.beatmap_id),
            key_count: parsed.key_count,
            mode_family: family,
            dominant_pattern: dominant,
        };
        let record = ManiaRawFeatureRecord {
            beatmap_id: metadata.beatmap_id,
            beatmapset_id: metadata.beatmapset_id,
            difficulty,
            style,
            base,
            key_count: metadata.key_count,
            mode_family: family,
            dominant_pattern: dominant,
            analyzer_version: MANIA_ANALYZER_VERSION,
        };
        Ok((metadata, record))
    }
}

impl ParsedManiaBeatmap {
    fn apply_clock_rate(&mut self, clock_rate: f64) {
        if (clock_rate - 1.0).abs() <= f64::EPSILON {
            return;
        }
        self.bpm *= clock_rate;
        for object in &mut self.objects {
            object.start /= clock_rate;
            object.end /= clock_rate;
        }
    }
}

fn parse_beatmap(bytes: &[u8]) -> Result<ParsedManiaBeatmap, ManiaAnalyzeError> {
    let text = String::from_utf8_lossy(bytes);
    let mut section = "";
    let mut mode = None;
    let mut key_count = None;
    let mut beatmap_id = 0_u64;
    let mut beatmapset_id = 0_u64;
    let mut artist = String::new();
    let mut title = String::new();
    let mut version = String::new();
    let mut creator = String::new();
    let mut bpms = Vec::new();
    let mut sv_changes = 0_usize;
    let mut objects = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
            continue;
        }
        match section {
            "[General]" => {
                if let Some(value) = value_after(line, "Mode:") {
                    mode = value.parse::<u8>().ok();
                }
            }
            "[Metadata]" => {
                if let Some(value) = value_after(line, "BeatmapID:") {
                    beatmap_id = value.parse().unwrap_or(0);
                } else if let Some(value) = value_after(line, "BeatmapSetID:") {
                    beatmapset_id = value.parse().unwrap_or(0);
                } else if let Some(value) = value_after(line, "Artist:") {
                    artist = value.to_owned();
                } else if let Some(value) = value_after(line, "Title:") {
                    title = value.to_owned();
                } else if let Some(value) = value_after(line, "Version:") {
                    version = value.to_owned();
                } else if let Some(value) = value_after(line, "Creator:") {
                    creator = value.to_owned();
                }
            }
            "[Difficulty]" => {
                if let Some(value) = value_after(line, "CircleSize:") {
                    key_count = value.parse::<f64>().ok().map(|value| value.round() as u8);
                }
            }
            "[TimingPoints]" => {
                let parts = line.split(',').collect::<Vec<_>>();
                if parts.len() >= 7 {
                    let beat_length = parts[1].parse::<f64>().unwrap_or(0.0);
                    let uninherited = parts[6].trim() == "1";
                    if uninherited && beat_length > 0.0 {
                        bpms.push(60_000.0 / beat_length);
                    } else if !uninherited && beat_length < 0.0 {
                        sv_changes += 1;
                    }
                }
            }
            "[HitObjects]" => {
                let parts = line.split(',').collect::<Vec<_>>();
                if parts.len() < 5 {
                    continue;
                }
                let x = parts[0].parse::<f64>().map_err(|_| {
                    ManiaAnalyzeError::Invalid("invalid hit object x coordinate".into())
                })?;
                let start = parts[2].parse::<f64>().map_err(|_| {
                    ManiaAnalyzeError::Invalid("invalid hit object timestamp".into())
                })?;
                let kind = parts[3].parse::<u32>().unwrap_or(0);
                let long_note = kind & 128 != 0;
                let end = if long_note {
                    parts
                        .get(5)
                        .and_then(|value| value.split(':').next())
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(start)
                        .max(start)
                } else {
                    start
                };
                // The key count is validated below; retain x temporarily as a
                // 0..127 pseudo-column so malformed metadata can still fail cleanly.
                objects.push(HitObject {
                    column: ((x.clamp(0.0, 511.999) / 512.0) * 128.0).floor() as u8,
                    start,
                    end,
                    long_note,
                });
            }
            _ => {}
        }
    }

    let mode = mode.ok_or_else(|| ManiaAnalyzeError::Invalid("missing Mode".into()))?;
    if mode != 3 {
        return Err(ManiaAnalyzeError::UnsupportedMode(mode));
    }
    let key_count = key_count
        .ok_or_else(|| ManiaAnalyzeError::Invalid("missing CircleSize/key count".into()))?;
    if !matches!(key_count, 4 | 6 | 7) {
        return Err(ManiaAnalyzeError::UnsupportedKeyCount(key_count));
    }
    if objects.is_empty() {
        return Err(ManiaAnalyzeError::Invalid(
            "beatmap has no hit objects".into(),
        ));
    }
    for object in &mut objects {
        object.column =
            ((object.column as usize * key_count as usize) / 128).min(key_count as usize - 1) as u8;
    }
    objects.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then_with(|| left.column.cmp(&right.column))
    });
    if beatmap_id == 0 {
        let digest = Sha256::digest(bytes);
        beatmap_id = u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix"));
        if beatmap_id == 0 {
            beatmap_id = 1;
        }
    }
    bpms.sort_by(f64::total_cmp);
    let bpm = if bpms.is_empty() {
        0.0
    } else {
        bpms[bpms.len() / 2]
    };

    Ok(ParsedManiaBeatmap {
        beatmap_id,
        beatmapset_id,
        artist,
        title,
        version,
        creator,
        key_count,
        bpm,
        sv_changes,
        objects,
    })
}

fn value_after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix).map(str::trim)
}

fn timeline_rows(objects: &[HitObject], key_count: u8) -> Vec<TimelineRow> {
    let mut events = Vec::with_capacity(objects.len() * 2);
    for object in objects {
        events.push(Event {
            time: object.start,
            column: object.column,
            kind: if object.long_note {
                EventKind::LongHead
            } else {
                EventKind::Tap
            },
            end: object.end,
        });
        if object.long_note {
            events.push(Event {
                time: object.end,
                column: object.column,
                kind: EventKind::LongTail,
                end: object.end,
            });
        }
    }
    events.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then_with(|| left.column.cmp(&right.column))
            .then_with(|| event_order(left.kind).cmp(&event_order(right.kind)))
    });

    let mut rows = Vec::new();
    let mut active_until = vec![f64::NEG_INFINITY; key_count as usize];
    let mut index = 0;
    while index < events.len() {
        let time = events[index].time;
        let mut next = index;
        let mut head_mask = 0_u8;
        let mut normal_mask = 0_u8;
        let mut ln_head_mask = 0_u8;
        let mut release_mask = 0_u8;
        while next < events.len() && (events[next].time - time).abs() <= ROW_TOLERANCE_MS {
            let event = events[next];
            let bit = 1_u8 << event.column;
            match event.kind {
                EventKind::Tap => {
                    head_mask |= bit;
                    normal_mask |= bit;
                }
                EventKind::LongHead => {
                    head_mask |= bit;
                    ln_head_mask |= bit;
                    active_until[event.column as usize] =
                        active_until[event.column as usize].max(event.end);
                }
                EventKind::LongTail => release_mask |= bit,
            }
            next += 1;
        }
        let mut active_hold_mask = 0_u8;
        for (column, end) in active_until.iter().enumerate() {
            if *end + ROW_TOLERANCE_MS >= time {
                active_hold_mask |= 1_u8 << column;
            }
        }
        rows.push(TimelineRow {
            time,
            head_mask,
            normal_mask,
            ln_head_mask,
            release_mask,
            active_hold_mask,
        });
        index = next;
    }
    rows
}

fn event_order(kind: EventKind) -> u8 {
    match kind {
        EventKind::LongTail => 0,
        EventKind::Tap => 1,
        EventKind::LongHead => 2,
    }
}

fn analyze_parsed(
    parsed: &ParsedManiaBeatmap,
) -> Result<
    (
        ManiaDifficultyVector,
        ManiaStyleVector,
        ManiaBaseFeatures,
        ManiaModeFamily,
        ManiaPattern,
    ),
    ManiaAnalyzeError,
> {
    let rows = timeline_rows(&parsed.objects, parsed.key_count);
    if rows.is_empty() {
        return Err(ManiaAnalyzeError::Invalid(
            "beatmap has no analyzable rows".into(),
        ));
    }

    let key_count = parsed.key_count as usize;
    let mask_count = 1_usize << key_count;
    let transition_count = mask_count * mask_count;
    let mut mask_counts = vec![0_u32; mask_count];
    let mut transition_counts = vec![0_u32; transition_count];
    let mut entropy_queue = VecDeque::<(f64, usize, usize)>::new();
    let mut entropy_total = 0_u32;
    let mut transition_total = 0_u32;
    let mut previous_mask = 0_u8;
    let mut previous_head_time = None;
    let mut previous_head_mask = 0_u8;
    let mut previous_dt = None;
    let mut previous_column = vec![None::<f64>; key_count];
    let zones = zones_for(parsed.key_count);
    let mut previous_zone_time = vec![None::<f64>; zones.len()];
    let mut previous_zone_mask = vec![0_u8; zones.len()];
    let mut hand_stamina = vec![0.0_f64; zones.len()];
    let mut stream_states = [StreamState::default(); 8];
    let mut stream_values: [Vec<f64>; 8] = std::array::from_fn(|_| Vec::with_capacity(rows.len()));
    let mut local_values = Vec::with_capacity(rows.len());
    let mut pattern_amounts = [0.0_f64; 6];
    let mut nps_1s = VecDeque::<(f64, u32)>::new();
    let mut nps_4s = VecDeque::<(f64, u32)>::new();
    let mut count_1s = 0_u32;
    let mut count_4s = 0_u32;
    let mut peak_nps = 0.0_f64;
    let mut head_rows = 0_u32;
    let mut note_count = 0_u32;
    let mut ln_count = 0_u32;
    let mut chord_rows = 0_u32;
    let mut large_chord_rows = 0_u32;
    let mut anchor_rows = 0_u32;
    let mut hybrid_rows = 0_u32;
    let mut rotations = 0_u32;
    let mut zone_transitions = 0_u32;
    let mut entropy_sum = 0.0_f64;
    let mut transition_entropy_sum = 0.0_f64;
    let mut entropy_samples = 0_u32;
    let mut inactive_ms = 0.0_f64;
    let mut break_count = 0_u32;
    let mut previous_timeline_time = rows.first().map(|row| row.time).unwrap_or(0.0);
    let mut previous_active_mask = 0_u8;
    let mut hold_lane_ms = 0.0_f64;

    for (row_index, row) in rows.iter().enumerate() {
        let pattern_previous_mask = previous_head_mask;
        let timeline_dt = (row.time - previous_timeline_time).max(0.0);
        hold_lane_ms += timeline_dt * previous_active_mask.count_ones() as f64;
        previous_timeline_time = row.time;
        previous_active_mask = row.active_hold_mask;

        let head_count = row.head_count();
        let mut inputs = [0.0_f64; 8];
        let mut anchor = false;
        let mut rotation_mean = 0.0;
        let mut row_chord = 0.0;

        if head_count > 0 {
            head_rows += 1;
            note_count += head_count;
            ln_count += row.ln_head_mask.count_ones();
            if head_count >= 2 {
                chord_rows += 1;
            }
            if head_count as usize >= key_count.div_ceil(2) {
                large_chord_rows += 1;
            }
            if row.ln_head_mask != 0 && row.normal_mask != 0 {
                hybrid_rows += 1;
            }

            while let Some((time, mask, transition)) = entropy_queue.front().copied() {
                if row.time - time <= ENTROPY_WINDOW_MS {
                    break;
                }
                entropy_queue.pop_front();
                mask_counts[mask] = mask_counts[mask].saturating_sub(1);
                entropy_total = entropy_total.saturating_sub(1);
                if transition != usize::MAX {
                    transition_counts[transition] = transition_counts[transition].saturating_sub(1);
                    transition_total = transition_total.saturating_sub(1);
                }
            }
            let mask = row.head_mask as usize;
            let transition = if previous_mask == 0 {
                usize::MAX
            } else {
                previous_mask as usize * mask_count + mask
            };
            mask_counts[mask] += 1;
            entropy_total += 1;
            if transition != usize::MAX {
                transition_counts[transition] += 1;
                transition_total += 1;
            }
            entropy_queue.push_back((row.time, mask, transition));
            let rhythm_entropy = normalized_entropy(&mask_counts, entropy_total, key_count as f64);
            let transition_entropy =
                normalized_entropy(&transition_counts, transition_total, (key_count * 2) as f64);
            entropy_sum += rhythm_entropy;
            transition_entropy_sum += transition_entropy;
            entropy_samples += 1;

            let dt_row = previous_head_time
                .map(|time| row.time - time)
                .unwrap_or(1000.0);
            if dt_row > 1000.0 {
                inactive_ms += dt_row - 1000.0;
                break_count += 1;
            }
            let row_rate = if previous_head_time.is_some() {
                strain_rate(dt_row, 155.0, 30.0, 1.06)
            } else {
                0.0
            };
            let mut zone_rates = Vec::with_capacity(zones.len());
            let mut zone_rotation_rates = Vec::with_capacity(zones.len());
            let mut same_hand_chords = 0_u32;
            let mut same_hand_overlap_sum = 0.0_f64;
            let mut active_zone_count = 0_u32;
            for (zone_index, zone_mask) in zones.iter().copied().enumerate() {
                let active = row.head_mask & zone_mask;
                if active == 0 {
                    continue;
                }
                active_zone_count += 1;
                if previous_zone_mask[zone_index] & active != 0 {
                    same_hand_overlap_sum += 1.0;
                }
                let dt = previous_zone_time[zone_index]
                    .map(|time| row.time - time)
                    .unwrap_or(1000.0);
                let rate = if previous_zone_time[zone_index].is_some() {
                    strain_rate(dt, 180.0, 40.0, 1.08)
                } else {
                    0.0
                };
                let rotated = previous_zone_mask[zone_index] != 0
                    && previous_zone_mask[zone_index] & active == 0;
                if previous_zone_mask[zone_index] != 0 {
                    zone_transitions += 1;
                    if rotated {
                        rotations += 1;
                    }
                }
                zone_rates.push(rate);
                zone_rotation_rates.push(if rotated {
                    strain_rate(dt, 205.0, 45.0, 1.05)
                } else {
                    0.0
                });
                same_hand_chords += active.count_ones().saturating_sub(1);
                hand_stamina[zone_index] = decay_state(hand_stamina[zone_index], rate, dt, 8000.0);
                previous_zone_time[zone_index] = Some(row.time);
                previous_zone_mask[zone_index] = active;
            }
            let max_zone = zone_rates.iter().copied().fold(0.0, f64::max);
            let mean_zone = mean(&zone_rates);
            inputs[0] = 0.55 * row_rate + 0.30 * max_zone + 0.15 * mean_zone;
            inputs[1] = zone_rates
                .iter()
                .copied()
                .zip(zone_rotation_rates.iter().copied())
                .map(|(rate, rotation)| 0.70 * rate + 0.30 * rotation)
                .fold(0.0, f64::max);

            let mut max_jack = 0.0_f64;
            for (column, previous_column_time) in
                previous_column.iter_mut().enumerate().take(key_count)
            {
                if row.head_mask & (1_u8 << column) == 0 {
                    continue;
                }
                if let Some(previous) = *previous_column_time {
                    let dt = row.time - previous;
                    max_jack = max_jack.max(strain_rate(dt, 185.0, 35.0, 1.18));
                    if dt <= 220.0 {
                        anchor = true;
                    }
                }
                *previous_column_time = Some(row.time);
            }
            if anchor {
                anchor_rows += 1;
            }
            row_chord = head_count.saturating_sub(1) as f64 / (key_count - 1).max(1) as f64;
            let same_hand_chord = same_hand_chords as f64 / (key_count - zones.len()).max(1) as f64;
            inputs[2] = max_jack * (1.0 + 0.20 * row_chord + 0.15 * f64::from(anchor));
            let same_hand_overlap = same_hand_overlap_sum / active_zone_count.max(1) as f64;
            let chord_input = row_chord * (1.0 + 0.18 * inputs[0]) + 0.22 * same_hand_chord;
            inputs[3] =
                chord_input * (0.55 * inputs[2] + 0.30 * same_hand_overlap + 0.15 * inputs[1]);

            let rhythm_chaos = previous_dt
                .map(|previous: f64| {
                    (((dt_row + 24.0) / (previous + 24.0)).log2().abs() / 2.0).min(1.0)
                })
                .unwrap_or(0.0);
            inputs[4] = 0.32 * rhythm_chaos
                + 0.24 * rhythm_entropy
                + 0.24 * transition_entropy
                + 0.20 * f64::from(previous_head_mask != 0 && previous_head_mask != row.head_mask);

            nps_1s.push_back((row.time, head_count));
            nps_4s.push_back((row.time, head_count));
            count_1s += head_count;
            count_4s += head_count;
            while nps_1s
                .front()
                .is_some_and(|(time, _)| row.time - *time > 1000.0)
            {
                count_1s -= nps_1s.pop_front().expect("front checked").1;
            }
            while nps_4s
                .front()
                .is_some_and(|(time, _)| row.time - *time > 4000.0)
            {
                count_4s -= nps_4s.pop_front().expect("front checked").1;
            }
            peak_nps = peak_nps.max(count_1s as f64);
            let stamina_hand = hand_stamina.iter().copied().fold(0.0, f64::max);
            inputs[5] = 0.40 * (count_1s as f64).ln_1p() / 24.0_f64.ln()
                + 0.35 * ((count_4s as f64 / 4.0).ln_1p() / 24.0_f64.ln())
                + 0.25 * stamina_hand;
            previous_dt = Some(dt_row);
            previous_head_time = Some(row.time);
            previous_head_mask = row.head_mask;
            previous_mask = row.head_mask;
            rotation_mean = if zone_rotation_rates.is_empty() {
                0.0
            } else {
                zone_rotation_rates
                    .iter()
                    .filter(|value| **value > 0.0)
                    .count() as f64
                    / zone_rotation_rates.len() as f64
            };
        }

        let occupancy = row.active_hold_mask.count_ones() as f64 / key_count.max(1) as f64;
        let release_pressure = row.release_mask.count_ones() as f64 / key_count.max(1) as f64;
        let hybrid = f64::from(row.ln_head_mask != 0 && row.normal_mask != 0);
        let ln_head_density = row.ln_head_mask.count_ones() as f64 / key_count.max(1) as f64;
        inputs[6] =
            0.35 * occupancy + 0.25 * release_pressure + 0.20 * hybrid + 0.20 * ln_head_density;
        inputs[7] = 0.45 * inputs[0]
            + 0.20 * inputs[1]
            + 0.15 * inputs[2]
            + 0.10 * inputs[3]
            + 0.10 * inputs[4];

        let state_dt = if row_index == 0 {
            0.0
        } else {
            row.time - rows[row_index - 1].time
        };
        for stream in 0..8 {
            let config = STREAM_CONFIGS[stream];
            stream_states[stream].burst = decay_state(
                stream_states[stream].burst,
                inputs[stream],
                state_dt,
                config.burst_tau,
            );
            stream_states[stream].sustain = decay_state(
                stream_states[stream].sustain,
                inputs[stream],
                state_dt,
                config.sustain_tau,
            );
            let value = config.burst_mix * stream_states[stream].burst
                + (1.0 - config.burst_mix) * stream_states[stream].sustain;
            stream_values[stream].push(value);
        }
        let local = 0.20 * stream_values[0].last().copied().unwrap_or(0.0)
            + 0.16 * stream_values[1].last().copied().unwrap_or(0.0)
            + 0.14 * stream_values[2].last().copied().unwrap_or(0.0)
            + 0.14 * stream_values[3].last().copied().unwrap_or(0.0)
            + 0.11 * stream_values[4].last().copied().unwrap_or(0.0)
            + 0.10 * stream_values[5].last().copied().unwrap_or(0.0)
            + 0.10 * stream_values[6].last().copied().unwrap_or(0.0)
            + 0.05 * stream_values[7].last().copied().unwrap_or(0.0);
        local_values.push(local);

        let pattern =
            classify_pattern(row, pattern_previous_mask, anchor, row_chord, rotation_mean);
        let next_time = rows
            .get(row_index + 1)
            .map(|next| next.time)
            .unwrap_or(row.time + 1.0);
        let amount = (next_time - row.time).clamp(1.0, 1000.0);
        pattern_amounts[pattern.index()] += amount;
    }

    let aggregates = std::array::from_fn(|index| stream_aggregate(&stream_values[index], &rows));
    let difficulty = ManiaDifficultyVector::from_array(aggregates.map(|value| value as f32));
    let duration_ms = (rows.last().expect("non-empty").time - rows[0].time).max(1.0);
    let active_ms = (duration_ms - inactive_ms).max(1.0);
    let pattern_total = pattern_amounts.iter().sum::<f64>().max(1.0);
    let pattern_shares = pattern_amounts.map(|value| (value / pattern_total) as f32);
    let local_q97 = quantile(&local_values, 0.97);
    let local_q50 = quantile(&local_values, 0.50);
    let peak_to_sustain_gap = if local_q97 > 1e-9 {
        ((local_q97 - local_q50) / local_q97).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let hold_occupancy = (hold_lane_ms / (duration_ms * key_count as f64)).clamp(0.0, 1.0);
    let style = ManiaStyleVector {
        stream: pattern_shares[0],
        chordstream: pattern_shares[1],
        jacks: pattern_shares[2],
        coordination: pattern_shares[3],
        density: pattern_shares[4],
        wildcard: pattern_shares[5],
        chord_rate: ratio_u32(chord_rows, head_rows),
        large_chord_rate: ratio_u32(large_chord_rows, head_rows),
        rotation_rate: ratio_u32(rotations, zone_transitions),
        anchor_rate: ratio_u32(anchor_rows, head_rows),
        rhythm_entropy: ratio_f64(entropy_sum, entropy_samples),
        transition_entropy: ratio_f64(transition_entropy_sum, entropy_samples),
        ln_note_ratio: ratio_u32(ln_count, note_count),
        hold_occupancy: hold_occupancy as f32,
        hybrid_row_ratio: ratio_u32(hybrid_rows, head_rows),
        peak_to_sustain_gap: peak_to_sustain_gap as f32,
    };
    let mode_family = if style.ln_note_ratio <= 0.15 {
        ManiaModeFamily::Rc
    } else if style.ln_note_ratio >= 0.90 {
        ManiaModeFamily::Ln
    } else if style.hybrid_row_ratio >= 0.10 {
        ManiaModeFamily::Hb
    } else {
        ManiaModeFamily::Mix
    };
    let dominant_pattern = ManiaPattern::ALL
        .into_iter()
        .max_by(|left, right| {
            pattern_shares[left.index()]
                .partial_cmp(&pattern_shares[right.index()])
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or_default();
    let base = ManiaBaseFeatures {
        bpm: parsed.bpm as f32,
        length_seconds: (duration_ms / 1000.0) as f32,
        active_length_seconds: (active_ms / 1000.0) as f32,
        note_count: note_count as f32,
        row_count: head_rows as f32,
        avg_nps: (note_count as f64 / (active_ms / 1000.0)) as f32,
        peak_nps: peak_nps as f32,
        break_density: (break_count as f64 / (active_ms / 60_000.0).max(1.0)) as f32,
        sv_change_rate: (parsed.sv_changes as f64 / (duration_ms / 60_000.0).max(1.0)) as f32,
    };
    if difficulty
        .as_array()
        .into_iter()
        .chain(style.as_array())
        .chain([
            base.bpm,
            base.length_seconds,
            base.active_length_seconds,
            base.avg_nps,
            base.peak_nps,
            base.break_density,
            base.sv_change_rate,
        ])
        .any(|value| !value.is_finite())
    {
        return Err(ManiaAnalyzeError::Invalid(
            "analysis produced a non-finite feature".into(),
        ));
    }
    Ok((difficulty, style, base, mode_family, dominant_pattern))
}

fn zones_for(key_count: u8) -> Vec<u8> {
    match key_count {
        4 => vec![0b0011, 0b1100],
        6 => vec![0b000111, 0b111000],
        7 => vec![0b0000111, 0b0001000, 0b1110000],
        _ => unreachable!("validated key count"),
    }
}

fn classify_pattern(
    row: &TimelineRow,
    previous_mask: u8,
    anchor: bool,
    row_chord: f64,
    rotation: f64,
) -> ManiaPattern {
    if row.ln_head_mask != 0 && row.normal_mask != 0 || row.release_mask & row.head_mask != 0 {
        ManiaPattern::Coordination
    } else if row.ln_head_mask != 0
        && (row.head_count() >= 2 || row.active_hold_mask.count_ones() >= 2)
    {
        ManiaPattern::Density
    } else if row.ln_head_mask != 0 || row.release_mask != 0 || row.active_hold_mask != 0 {
        ManiaPattern::Wildcard
    } else if anchor || previous_mask & row.head_mask != 0 {
        ManiaPattern::Jacks
    } else if row_chord > 0.0 {
        ManiaPattern::Chordstream
    } else if rotation > 0.0 || row.head_mask != 0 {
        ManiaPattern::Stream
    } else {
        ManiaPattern::Wildcard
    }
}

fn strain_rate(dt: f64, base: f64, offset: f64, power: f64) -> f64 {
    (base / (dt + offset).max(16.0)).powf(power).min(8.0)
}

fn decay_state(state: f64, input: f64, dt: f64, tau: f64) -> f64 {
    state * (-dt.max(0.0) / tau).exp() + input
}

fn normalized_entropy(counts: &[u32], total: u32, normalizer_bits: f64) -> f64 {
    if total == 0 || normalizer_bits <= 0.0 {
        return 0.0;
    }
    let mut entropy = 0.0;
    for count in counts.iter().copied().filter(|count| *count > 0) {
        let probability = count as f64 / total as f64;
        entropy -= probability * probability.log2();
    }
    (entropy / normalizer_bits).clamp(0.0, 1.0)
}

fn stream_aggregate(values: &[f64], rows: &[TimelineRow]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let q97 = quantile_sorted(&sorted, 0.97);
    let q90 = quantile_sorted(&sorted, 0.90);
    let q75 = quantile_sorted(&sorted, 0.75);
    let q50 = quantile_sorted(&sorted, 0.50);
    let tail_count = ((sorted.len() as f64 * 0.04).ceil() as usize).max(1);
    let tail_mean = mean(&sorted[sorted.len() - tail_count..]);
    let power_mean = (sorted
        .iter()
        .map(|value| value.max(0.0).powf(2.4))
        .sum::<f64>()
        / sorted.len() as f64)
        .powf(1.0 / 2.4);
    let weighted =
        0.30 * q97 + 0.22 * q90 + 0.18 * tail_mean + 0.15 * q75 + 0.10 * power_mean + 0.05 * q50;

    let first_time = rows.first().map(|row| row.time).unwrap_or(0.0);
    let mut peaks = Vec::new();
    for (value, row) in values.iter().copied().zip(rows) {
        let section = ((row.time - first_time).max(0.0) / SECTION_MS).floor() as usize;
        if peaks.len() <= section {
            peaks.resize(section + 1, 0.0);
        }
        peaks[section] = f64::max(peaks[section], value);
    }
    peaks.sort_by(|left, right| right.total_cmp(left));
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (index, peak) in peaks.into_iter().enumerate() {
        let weight = SECTION_DECAY.powi(index as i32);
        numerator += peak * weight;
        denominator += weight;
    }
    let section = if denominator > 0.0 {
        numerator / denominator
    } else {
        0.0
    };
    0.80 * weighted + 0.20 * section
}

fn quantile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    quantile_sorted(&sorted, q)
}

fn quantile_sorted(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let position = q.clamp(0.0, 1.0) * (values.len() - 1) as f64;
    let left = position.floor() as usize;
    let right = position.ceil() as usize;
    values[left] * (1.0 - position.fract()) + values[right] * position.fract()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn ratio_u32(numerator: u32, denominator: u32) -> f32 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f32 / denominator as f32
    }
}

fn ratio_f64(numerator: f64, denominator: u32) -> f32 {
    if denominator == 0 {
        0.0
    } else {
        (numerator / denominator as f64) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(key_count: u8, objects: &str) -> Vec<u8> {
        format!(
            "osu file format v14\n\n[General]\nMode:3\n\n[Metadata]\nTitle:Test\nArtist:Test\nCreator:Test\nVersion:Test\nBeatmapID:42\nBeatmapSetID:4\n\n[Difficulty]\nCircleSize:{key_count}\nOverallDifficulty:8\n\n[TimingPoints]\n0,500,4,2,0,100,1,0\n\n[HitObjects]\n{objects}\n"
        )
        .into_bytes()
    }

    #[test]
    fn rejects_non_mania_and_unsupported_keys() {
        let non_mania = map(4, "64,192,0,1,0,0:0:0:0:");
        let text = String::from_utf8(non_mania)
            .unwrap()
            .replace("Mode:3", "Mode:0");
        assert!(matches!(
            ManiaAnalyzer::new().analyze_bytes(text.as_bytes()),
            Err(ManiaAnalyzeError::UnsupportedMode(0))
        ));
        assert!(matches!(
            ManiaAnalyzer::new().analyze_bytes(&map(5, "64,192,0,1,0,0:0:0:0:")),
            Err(ManiaAnalyzeError::UnsupportedKeyCount(5))
        ));
        assert!(matches!(
            ManiaAnalyzer::new().analyze_bytes(&map(4, "")),
            Err(ManiaAnalyzeError::Invalid(_))
        ));
    }

    #[test]
    fn supports_each_v1_key_count() {
        for key_count in [4_u8, 6, 7] {
            let objects = (0..key_count)
                .map(|lane| {
                    let x = ((lane as f64 + 0.5) * 512.0 / key_count as f64).floor() as usize;
                    format!("{x},192,{},1,0,0:0:0:0:", lane as usize * 120)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let (metadata, record) = ManiaAnalyzer::new()
                .analyze_bytes(&map(key_count, &objects))
                .unwrap();
            assert_eq!(metadata.key_count, key_count);
            assert_eq!(record.key_count, key_count);
        }
    }

    #[test]
    fn source_filename_id_overrides_stale_embedded_id() {
        let bytes = map(4, "64,192,0,1,0,0:0:0:0:");
        let (metadata, record) = ManiaAnalyzer::new()
            .analyze_bytes_with_beatmap_id(&bytes, 1234)
            .unwrap();
        assert_eq!(metadata.beatmap_id, 1234);
        assert_eq!(record.beatmap_id, 1234);
        assert_eq!(metadata.online_url, "https://osu.ppy.sh/b/1234");
    }

    #[test]
    fn long_notes_populate_ln_features() {
        let objects =
            "64,192,0,128,0,500:0:0:0:0:\n192,192,250,1,0,0:0:0:0:\n320,192,500,128,0,900:0:0:0:0:";
        let (_, record) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, objects))
            .unwrap();
        assert!(record.difficulty.long_note > 0.0);
        assert!(record.style.ln_note_ratio > 0.0);
        assert!(record.style.hold_occupancy > 0.0);
    }

    #[test]
    fn inherited_timing_points_are_counted_as_sv_changes() {
        let bytes = map(4, "64,192,0,1,0,0:0:0:0:\n192,192,1000,1,0,0:0:0:0:");
        let text = String::from_utf8(bytes).unwrap().replace(
            "0,500,4,2,0,100,1,0",
            "0,500,4,2,0,100,1,0\n500,-50,4,2,0,100,0,0",
        );
        let (_, record) = ManiaAnalyzer::new().analyze_bytes(text.as_bytes()).unwrap();
        assert_eq!(record.base.bpm, 120.0);
        assert!(record.base.sv_change_rate > 0.0);
    }

    #[test]
    fn jack_axis_exceeds_stream_for_fast_repeats() {
        let jack = (0..20)
            .map(|index| format!("64,192,{},1,0,0:0:0:0:", index * 100))
            .collect::<Vec<_>>()
            .join("\n");
        let (_, record) = ManiaAnalyzer::new().analyze_bytes(&map(4, &jack)).unwrap();
        assert!(record.difficulty.jack > record.difficulty.hand_stream);
        assert!(record.style.jacks > record.style.stream);
    }

    #[test]
    fn all_features_are_finite_for_7k_center_notes() {
        let objects = (0..24)
            .map(|index| {
                let x = if index % 2 == 0 {
                    256
                } else {
                    64 + (index % 7) * 64
                };
                format!("{x},192,{},1,0,0:0:0:0:", index * 120)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (_, record) = ManiaAnalyzer::new()
            .analyze_bytes(&map(7, &objects))
            .unwrap();
        assert!(record.difficulty.as_array().into_iter().all(f32::is_finite));
        assert!(record.style.as_array().into_iter().all(f32::is_finite));
    }

    #[test]
    fn seven_key_center_lane_preserves_mirror_symmetry() {
        let lanes = [0_usize, 3, 1, 5, 6, 3, 2, 4, 0, 6, 3, 1];
        let render = |mirror: bool| {
            lanes
                .iter()
                .enumerate()
                .map(|(index, lane)| {
                    let lane = if mirror { 6 - lane } else { *lane };
                    let x = ((lane as f64 + 0.5) * 512.0 / 7.0).floor() as usize;
                    format!("{x},192,{},1,0,0:0:0:0:", 1000 + index * 125)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (_, original) = ManiaAnalyzer::new()
            .analyze_bytes(&map(7, &render(false)))
            .unwrap();
        let (_, mirrored) = ManiaAnalyzer::new()
            .analyze_bytes(&map(7, &render(true)))
            .unwrap();
        for (left, right) in original
            .difficulty
            .as_array()
            .into_iter()
            .chain(original.style.as_array())
            .zip(
                mirrored
                    .difficulty
                    .as_array()
                    .into_iter()
                    .chain(mirrored.style.as_array()),
            )
        {
            assert!((left - right).abs() < 1e-5, "{left} != {right}");
        }
    }

    #[test]
    fn stream_chordjack_and_ln_samples_raise_their_axes() {
        let stream = (0..24)
            .map(|index| {
                let x = [64, 320, 192, 448][index % 4];
                format!("{x},192,{},1,0,0:0:0:0:", index * 120)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let chordjack = (0..24)
            .flat_map(|index| [64, 192].map(|x| format!("{x},192,{},1,0,0:0:0:0:", index * 120)))
            .collect::<Vec<_>>()
            .join("\n");
        let long_notes = (0..12)
            .map(|index| {
                let start = index * 300;
                let x = [64, 192, 320, 448][index % 4];
                format!("{x},192,{start},128,0,{}:0:0:0:0:", start + 240)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (_, stream_record) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &stream))
            .unwrap();
        let (_, chordjack_record) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &chordjack))
            .unwrap();
        let (_, ln_record) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &long_notes))
            .unwrap();
        assert!(stream_record.style.stream > stream_record.style.jacks);
        assert!(
            chordjack_record.difficulty.chordjack > stream_record.difficulty.chordjack,
            "{} <= {}",
            chordjack_record.difficulty.chordjack,
            stream_record.difficulty.chordjack
        );
        assert!(ln_record.difficulty.long_note > stream_record.difficulty.long_note);
    }

    #[test]
    fn rows_within_two_milliseconds_are_merged() {
        let objects = "64,192,100,1,0,0:0:0:0:\n320,192,102,1,0,0:0:0:0:";
        let (_, record) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, objects))
            .unwrap();
        assert_eq!(record.base.row_count, 1.0);
        assert_eq!(record.base.note_count, 2.0);
        assert_eq!(record.style.chord_rate, 1.0);
    }

    #[test]
    fn time_compression_increases_speed() {
        let chart = |step: usize| {
            (0..32)
                .map(|index| {
                    let x = [64, 192, 320, 448][index % 4];
                    format!("{x},192,{},1,0,0:0:0:0:", index * step)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (_, slow) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &chart(200)))
            .unwrap();
        let (_, fast) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &chart(100)))
            .unwrap();
        assert!(fast.difficulty.speed > slow.difficulty.speed);
        assert!(fast.difficulty.stamina > slow.difficulty.stamina);
    }

    #[test]
    fn mirror_and_global_time_offset_preserve_features() {
        let source = [
            (64, 100),
            (192, 220),
            (448, 340),
            (320, 460),
            (64, 580),
            (448, 700),
            (192, 820),
            (320, 940),
        ];
        let render = |mirror: bool, offset: i32| {
            source
                .iter()
                .map(|(x, time)| {
                    let x = if mirror { 512 - x } else { *x };
                    format!("{x},192,{},1,0,0:0:0:0:", time + offset)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (_, original) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &render(false, 0)))
            .unwrap();
        let (_, transformed) = ManiaAnalyzer::new()
            .analyze_bytes(&map(4, &render(true, 5000)))
            .unwrap();
        for (left, right) in original
            .difficulty
            .as_array()
            .into_iter()
            .chain(original.style.as_array())
            .zip(
                transformed
                    .difficulty
                    .as_array()
                    .into_iter()
                    .chain(transformed.style.as_array()),
            )
        {
            assert!((left - right).abs() < 1e-5, "{left} != {right}");
        }
    }

    #[test]
    fn clock_rate_mods_recompute_timing_bpm_and_strain() {
        let objects = (0..64)
            .map(|index| {
                let x = [64, 192, 320, 448][index % 4];
                format!("{x},192,{},1,0,0:0:0:0:", index * 150)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = map(4, &objects);
        let analyzer = ManiaAnalyzer::new();
        let (_, nm) = analyzer
            .analyze_bytes_with_mod(&bytes, ManiaGameMod::Nm)
            .unwrap();
        let (_, dt) = analyzer
            .analyze_bytes_with_mod(&bytes, ManiaGameMod::Dt)
            .unwrap();
        let (_, ht) = analyzer
            .analyze_bytes_with_mod(&bytes, ManiaGameMod::Ht)
            .unwrap();

        assert!((dt.base.bpm - nm.base.bpm * 1.5).abs() < 1e-4);
        assert!((ht.base.bpm - nm.base.bpm * 0.75).abs() < 1e-4);
        assert!((dt.base.length_seconds - nm.base.length_seconds / 1.5).abs() < 1e-4);
        assert!((ht.base.length_seconds - nm.base.length_seconds / 0.75).abs() < 1e-4);
        assert!(dt.base.avg_nps > nm.base.avg_nps && nm.base.avg_nps > ht.base.avg_nps);
        assert!(dt.difficulty.speed > nm.difficulty.speed);
        assert!(nm.difficulty.speed > ht.difficulty.speed);
    }
}
