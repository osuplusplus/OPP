//! Offline hitsound track synthesis with lazer gameplay-audio parity.
//!
//! The playable audio of an osu! map is BGM plus per-judgement samples:
//! each sample only fires when the judgement is a HIT (lazer
//! `DrawableHitObject.UpdateState`: `newState == ArmedState.Hit` →
//! `PlaySamples()`), at the judgement time, with volume/bank resolved from
//! the .osu sample data (`ConvertHitObjectParser` + `applySamples`):
//!
//! - every `[TimingPoints]` line carries a sample point (bank + volume);
//!   the point active at an object decides bank/volume the object itself
//!   does not specify (`LegacySampleControlPoint.ApplyTo`);
//! - non-repeating objects use the point at `EndTime + 5ms`, slider heads
//!   at `StartTime + 6ms`, slider node *i* at `StartTime + i*span + 5ms`
//!   (`CONTROL_POINT_LENIENCY` = 5);
//! - sliders play `NodeSamples[0]` on head hits, `NodeSamples[i]` on
//!   repeat hits, `TailSamples` (`NodeSamples[span]`) on the body
//!   judgement, `slidertick` on tick hits, and loop `sliderslide` (+
//!   `sliderwhistle` when the whistle flag is set) while tracked;
//! - playback volume = `max(volume, 5)%` (`MINIMUM_SAMPLE_VOLUME`),
//!   stereo balance follows the playfield X (`PositionalHitsoundsLevel`
//!   0.8 → `round2(1.6 * (x/512 - 0.5))`). Samples always play at their
//!   natural rate, under every mod — rate mods only compress the trigger
//!   times onto the wall timeline, the pitch never shifts (a deliberate
//!   deviation from lazer, whose gameplay mixer resamples samples with
//!   the rate; this holds under Nightcore too, whose BGM does keep the
//!   game's pitch-up);
//! - a combo drop to zero plays `Gameplay/combobreak` (`ComboEffects`:
//!   old combo > 20, or the first break while `AlwaysPlayFirstComboBreak`
//!   is on — the default), at full volume, centered;
//! - spinners: the judgement plays the line samples, bonus revolutions
//!   fire `spinnerbonus` at the ACTUAL full-rotation moments (bank/volume
//!   from the point at the spinner's start; spins past the tick budget
//!   `spinnerbonus-max`, whose LookupNames strip "-max"), and a looping
//!   `spinnerspin` runs while the disc audibly spins — synthesized with
//!   a phase accumulator at the lazer frequency curve
//!   (`20k/44.1k + progressUnclamped * 40k/44.1k`, capped `100k/44.1k`;
//!   `SpinnerFrequencyModulate` off pins it to the natural rate) under
//!   the tracking envelope (`VolumeTo(1, 300)` / `VolumeTo(0, 240)`).
//!
//! Samples come from the skin's `Gameplay/ArgonPro/` resource set
//! (`assets/sounds/ArgonPro`, embedded at compile time). Every osu!standard
//! gameplay lookup exists in that set, so the `ArgonProSkin.GetSample`
//! fallback chain never goes past it — including the slider sliding
//! loops (`sliderslide`/`sliderwhistle`), which the set ships as empty
//! PCM entries: ArgonPro plays NO sliding sounds. Node hit sounds
//! (head/repeat/tail) and `slidertick` have real samples and play.
//!
//! When a user skin is active (`--skin <dir>`) its own samples take
//! priority per element, with the ArgonPro set filling every slot the
//! skin leaves open (see [`SampleResolver`]). Beatmap skins are never
//! parsed — the renderer's deliberate deviation from lazer's chain,
//! which would consult them first.

use crate::game::{GameData, ObjKind};
use osu_parse::samples::{
    SampleBank as Bank, SampleBankInfo as BankInfo, SampleData, SampleObject as RawObj,
    SamplePoint,
};
use osu_replay_judge::process::NestedKind;
use osu_replay_judge::score::hit_result_ext;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Output sample rate (Hz). All shipped skin samples are 44.1k; foreign
/// rates are linearly resampled on decode.
pub const SAMPLE_RATE: u32 = 44_100;

/// `DrawableHitObject.MINIMUM_SAMPLE_VOLUME`.
const MINIMUM_SAMPLE_VOLUME: i32 = 5;
/// `LegacyBeatmapDecoder.CONTROL_POINT_LENIENCY`.
const CONTROL_POINT_LENIENCY: f64 = 5.0;
/// Lazer default `PositionalHitsoundsLevel` (0.8) doubled, see
/// `CalculateSamplePlaybackBalance`.
const POSITIONAL_HITSOUNDS_LEVEL: f64 = 0.8;


/// `SamplePointAt`: rightmost point with `time <= t`; before the first
/// point that point itself applies, else normal/100.
fn point_at(points: &[SamplePoint], t: f64) -> SamplePoint {
    if points.is_empty() {
        return SamplePoint { time: f64::NEG_INFINITY, bank: Bank::Normal, volume: 100 };
    }
    let mut lo = 0usize;
    let mut hi = points.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if points[mid].time <= t {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 { points[0] } else { points[lo - 1] }
}

/// A fully resolved playback sample.
#[derive(Clone, Copy, Debug)]
struct HitSample {
    name: &'static str,
    bank: Bank,
    volume: i32,
}

/// `convertSoundType` + `LegacySampleControlPoint.ApplyTo`: hitnormal
/// always, additions per flags; unspecified banks/volumes inherit from the
/// sample point active at the given time.
fn resolve_samples(sound_type: u8, info: &BankInfo, point: SamplePoint) -> Vec<HitSample> {
    let mut out = Vec::with_capacity(4);
    let mut push = |name: &'static str, bank: Option<Bank>| {
        let bank = bank.unwrap_or(point.bank);
        let volume = if info.volume > 0 { info.volume } else { point.volume };
        out.push(HitSample { name, bank, volume });
    };
    push("hitnormal", info.normal);
    if sound_type & 0b100 != 0 {
        push("hitfinish", info.additions);
    }
    if sound_type & 0b10 != 0 {
        push("hitwhistle", info.additions);
    }
    if sound_type & 0b1000 != 0 {
        push("hitclap", info.additions);
    }
    out
}

// ---------------------------------------------------------------------------
// Event assembly (map timeline, ms)
// ---------------------------------------------------------------------------

/// One scheduled sample placement: judgement time plus the playfield X for
/// stereo balance. Loop sounds are pre-expanded into placements at their
/// natural tile length, cut at the tracking end (`until`) — lazer stops
/// the looping sample the moment tracking breaks.
struct Placement {
    time: f64,
    sample: HitSample,
    /// Playfield X in osu coordinates (0..512).
    x: f32,
    /// Map-time cutoff (ms) for loop tiles; `None` lets the sample ring
    /// out naturally.
    until: Option<f64>,
}

fn balance(x: f32) -> f64 {
    let b = POSITIONAL_HITSOUNDS_LEVEL * 2.0 * (x as f64 / 512.0 - 0.5);
    (b * 100.0).round() / 100.0
}

fn build_placements(game: &GameData, data: &SampleData, t0: f64, t_map_end: f64, resolver: &mut SampleResolver) -> Vec<Placement> {
    let mut out: Vec<Placement> = Vec::new();
    if game.objects.len() != data.objects.len() {
        return out; // parser mismatch; silence rather than desynced sounds
    }

    for (obj, raw) in game.objects.iter().zip(&data.objects) {
        match obj.kind {
            ObjKind::Circle => {
                if let Some((t, r)) = obj.body_judged {
                    if hit_result_ext::is_hit(r) && t >= t0 && t <= t_map_end {
                        let point = point_at(&data.points, obj.end_time + CONTROL_POINT_LENIENCY);
                        for s in resolve_samples(raw.sound_type, &raw.bank, point) {
                            out.push(Placement { time: t, sample: s, x: obj.position[0], until: None });
                        }
                    }
                }
            }
            ObjKind::Spinner => {
                if let Some((t, r)) = obj.body_judged {
                    if hit_result_ext::is_hit(r) && t >= t0 && t <= t_map_end {
                        let point = point_at(&data.points, raw.end_time + CONTROL_POINT_LENIENCY);
                        for s in resolve_samples(raw.sound_type, &raw.bank, point) {
                            out.push(Placement { time: t, sample: s, x: obj.position[0], until: None });
                        }
                    }
                }
                // Bonus revolutions (`SpinnerBonusTick`: Samples =
                // CreateHitSampleInfo("spinnerbonus"), bank/volume from the
                // sample point at the spinner's start) fire at the ACTUAL
                // full-rotation moments. Plain `SpinnerTick`s carry no
                // samples in lazer (SmallBonus is silent); spins past the
                // tick budget play `spinnerbonus-max` (sound, no score —
                // `maxBonusSample`; its LookupNames tail falls back to
                // "spinnerbonus" when a skin has no -max file).
                let point = point_at(&data.points, raw.start_time);
                for (oi, time, large) in &game.spinner_ticks {
                    if *oi == obj.index && *large && *time >= t0 && *time <= t_map_end {
                        out.push(Placement {
                            time: *time,
                            sample: HitSample { name: "spinnerbonus", bank: point.bank, volume: point.volume },
                            x: 256.0,
                            until: None,
                        });
                    }
                }
                for (oi, time) in &game.spinner_max_ticks {
                    if *oi == obj.index && *time >= t0 && *time <= t_map_end {
                        out.push(Placement {
                            time: *time,
                            sample: HitSample { name: "spinnerbonus-max", bank: point.bank, volume: point.volume },
                            x: 256.0,
                            until: None,
                        });
                    }
                }
            }
            ObjKind::Slider => {
                // Object-level samples resolve at StartTime + leniency + 1
                // and seed the tick + sliding sounds.
                let (obj_samples, obj_normal) = slider_object_samples(raw, obj, data);
                let span_duration = if obj.span_count > 0 { obj.duration / obj.span_count as f64 } else { 0.0 };

                let node_samples = |i: usize| -> Vec<HitSample> {
                    let ty = raw.node_types.get(i).copied().unwrap_or(raw.sound_type);
                    let info = raw.node_banks.get(i).unwrap_or(&raw.bank);
                    let point = point_at(
                        &data.points,
                        obj.start_time + i as f64 * span_duration + CONTROL_POINT_LENIENCY,
                    );
                    resolve_samples(ty, info, point)
                };

                // Head (NodeSamples[0]).
                if let Some((t, r)) = obj.head_judged {
                    if hit_result_ext::is_hit(r) && t >= t0 && t <= t_map_end {
                        for s in node_samples(0) {
                            out.push(Placement { time: t, sample: s, x: obj.position[0], until: None });
                        }
                    }
                }

                for n in &obj.nested {
                    let Some((t, r)) = n.judged else { continue };
                    if !hit_result_ext::is_hit(r) || t < t0 || t > t_map_end {
                        continue;
                    }
                    match n.kind {
                        NestedKind::Tick => {
                            out.push(Placement {
                                time: t,
                                sample: HitSample { name: "slidertick", ..obj_normal },
                                x: n.position[0],
                                until: None,
                            });
                        }
                        NestedKind::Repeat => {
                            // RepeatIndex + 1 == span index of the repeat.
                            for s in node_samples(n.span_index) {
                                out.push(Placement { time: t, sample: s, x: n.position[0], until: None });
                            }
                        }
                        NestedKind::Head | NestedKind::Tail => {}
                    }
                }

                // Slider body judgement plays the tail samples.
                if let Some((t, r)) = obj.body_judged {
                    if hit_result_ext::is_hit(r) && t >= t0 && t <= t_map_end {
                        for s in node_samples(obj.span_count) {
                            out.push(Placement { time: t, sample: s, x: obj.end_position[0], until: None });
                        }
                    }
                }

                // Sliding loops while tracked (snapshots carry the engine's
                // tracking state at 60fps game frames, map timeline).
                // Shared sample construction + run scan with the live
                // loop-event collector (see collect_loop_events).
                let dbg = std::env::var("HITSOUND_DEBUG").is_ok();
                let runs = tracked_runs(game, obj);
                for sample in slide_loop_samples(&obj_samples, obj_normal, raw.sound_type) {
                    let len_ms = resolver.clip(sample).map(|w| w.duration_ms()).unwrap_or(0.0);
                    if len_ms <= 0.0 {
                        continue;
                    }
                    for &(a, b) in &runs {
                        tile_loop(&mut out, obj, a, b, sample, len_ms, game.rate, t0, t_map_end);
                    }
                }
                if dbg {
                    eprintln!(
                        "hitsound debug: slider #{} [{:.0}..{:.0}] head={:?} body={:?} nested={} runs={:?} whistle={}",
                        obj.index,
                        obj.start_time,
                        obj.end_time,
                        obj.head_judged.map(|(t, r)| (t as i64, format!("{:?}", r))),
                        obj.body_judged.map(|(t, r)| (t as i64, format!("{:?}", r))),
                        obj.nested.iter().filter(|n| n.judged.is_some()).count(),
                        runs,
                        raw.sound_type & 0b10 != 0,
                    );
                }
            }
        }
    }

    // Combo breaks (`ComboEffects`): when the score processor's combo
    // drops to zero the combobreak sample plays if the old combo was > 20,
    // or on the very first break (`AlwaysPlayFirstComboBreak`, default
    // on). Full volume, centered (a plain `SampleInfo`, no balance).
    {
        let mut first_break = false;
        let mut prev_combo = 0;
        for e in &game.score_events {
            if e.combo == 0 && prev_combo != 0 && (prev_combo > 20 || !first_break) {
                first_break = true;
                if e.time >= t0 && e.time <= t_map_end {
                    out.push(Placement {
                        time: e.time,
                        sample: HitSample { name: "combobreak", bank: Bank::Normal, volume: 100 },
                        x: 256.0,
                        until: None,
                    });
                }
            }
            prev_combo = e.combo;
        }
    }
    out
}

/// Tiles a loop sample across the tracked interval [a, b] (map ms) so it
/// loops seamlessly at its natural rate on the wall timeline: each tile
/// rings for `len_ms` wall ms, i.e. `len_ms * rate` map ms apart.
/// Balance follows the ball position per tile.
fn tile_loop(
    out: &mut Vec<Placement>,
    obj: &crate::game::ObjView,
    a: f64,
    b: f64,
    sample: HitSample,
    len_ms: f64,
    rate: f64,
    t0: f64,
    t_map_end: f64,
) {
    if len_ms <= 0.0 {
        return;
    }
    let mut t = a;
    while t < b {
        if t >= t0 && t <= t_map_end {
            let progress = if obj.duration > 0.0 { (t - obj.start_time) / obj.duration } else { 0.0 };
            let x = obj.slider_ball_at(progress.clamp(0.0, 1.0))[0];
            out.push(Placement { time: t, sample, x, until: Some(b.min(t_map_end)) });
        }
        t += len_ms * rate;
    }
}

/// Object-level samples of a slider plus the hitnormal fallback that
/// seeds the sliding loops.
fn slider_object_samples(raw: &RawObj, obj: &crate::game::ObjView, data: &SampleData) -> (Vec<HitSample>, HitSample) {
    let head_point = point_at(&data.points, obj.start_time + CONTROL_POINT_LENIENCY + 1.0);
    let obj_samples = resolve_samples(raw.sound_type, &raw.bank, head_point);
    let obj_normal = obj_samples.iter().find(|s| s.name == "hitnormal").copied().unwrap_or(HitSample {
        name: "hitnormal",
        bank: head_point.bank,
        volume: head_point.volume,
    });
    (obj_samples, obj_normal)
}

/// Looping samples of a tracked slider: `sliderslide` always,
/// `sliderwhistle` when the object's whistle flag is set.
fn slide_loop_samples(obj_samples: &[HitSample], obj_normal: HitSample, sound_type: u8) -> Vec<HitSample> {
    let mut out = vec![HitSample { name: "sliderslide", ..obj_normal }];
    if sound_type & 0b10 != 0 {
        out.push(HitSample {
            name: "sliderwhistle",
            bank: obj_samples.iter().find(|s| s.name == "hitwhistle").map(|s| s.bank).unwrap_or(obj_normal.bank),
            volume: obj_normal.volume,
        });
    }
    out
}

/// Tracking runs of a slider (map ms): consecutive tracked snapshots
/// extend a run to the next frame boundary, an untracked frame flushes.
fn tracked_runs(game: &GameData, obj: &crate::game::ObjView) -> Vec<(f64, f64)> {
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut run: Option<(f64, f64)> = None;
    for (i, snap) in game.snapshots.iter().enumerate() {
        let tracked = snap
            .sliders
            .iter()
            .any(|(idx, tr)| *idx == obj.index && *tr)
            && snap.time >= obj.start_time
            && snap.time <= obj.end_time;
        match (&mut run, tracked) {
            (Some((_, end)), true) => {
                let next = game.snapshots.get(i + 1).map(|s| s.time).unwrap_or(snap.time);
                *end = next.max(snap.time);
            }
            (None, true) => {
                let next = game.snapshots.get(i + 1).map(|s| s.time).unwrap_or(snap.time);
                run = Some((snap.time, next.max(snap.time)));
            }
            _ => {
                if let Some((a, b)) = run.take() {
                    runs.push((a, b));
                }
            }
        }
    }
    if let Some((a, b)) = run.take() {
        runs.push((a, b));
    }
    runs
}

// ---------------------------------------------------------------------------
// Live-playback loop events
// ---------------------------------------------------------------------------

/// A loop sound for live playback hosts (kira): `spinnerspin` /
/// `sliderslide` / `sliderwhistle`. The offline export synthesizes these
/// into the mix buffer; a live preview drives a looping handle per run
/// with the same lazer semantics.
#[derive(Clone, Debug)]
pub struct LoopSoundEvent {
    pub name: &'static str,
    pub bank: &'static str,
    /// Beatmap sample volume 0-100 (receiver applies `max(5)` and the
    /// Effect channel volume).
    pub volume: i32,
    /// Audible intervals (map ms, ascending); slider runs are exactly the
    /// tracking intervals, spinner runs carry the 300ms/240ms fade.
    pub runs: Vec<(f64, f64)>,
    pub control: LoopControl,
}

#[derive(Clone, Debug)]
pub enum LoopControl {
    /// Pan follows the slider ball (index into `GameData::objects`).
    Slider { object_index: usize },
    /// Playback rate follows spin progress (snapshot time → cumulative
    /// rotation degrees, plus the filled-spinner rotation count).
    Spin { rotation: Vec<(f64, f32)>, spins_required: f64 },
}

impl LoopSoundEvent {
    /// Run audible at map time `t`, if any (spinner runs include their
    /// fade tail).
    pub fn run_at(&self, t: f64) -> Option<usize> {
        let i = self.runs.partition_point(|r| r.0 <= t).checked_sub(1)?;
        let (a, b) = self.runs[i];
        let end = match self.control {
            LoopControl::Spin { .. } => b + SPIN_FADE_OUT,
            LoopControl::Slider { .. } => b,
        };
        (t >= a && t <= end).then_some(i)
    }

    /// (playback rate, 0..1 amplitude (line volume × envelope), playfield
    /// X) at map time `t` inside run `ri`. Mirrors `synth_spin_loop`.
    pub fn params_at(&self, ri: usize, t: f64, freq_modulate: bool, game: &GameData) -> (f64, f64, f32) {
        let vol = self.volume.max(MINIMUM_SAMPLE_VOLUME) as f64 / 100.0;
        match &self.control {
            LoopControl::Slider { object_index } => {
                let obj = &game.objects[*object_index];
                let progress = if obj.duration > 0.0 { (t - obj.start_time) / obj.duration } else { 0.0 };
                (1.0, vol, obj.slider_ball_at(progress.clamp(0.0, 1.0))[0])
            }
            LoopControl::Spin { rotation, spins_required } => {
                let (a, b) = self.runs[ri];
                let env = if t <= b {
                    ((t - a) / SPIN_FADE_IN).clamp(0.0, 1.0)
                } else {
                    1.0 - (t - b) / SPIN_FADE_OUT
                };
                let progress = if *spins_required > 0.0 {
                    rotation_at(rotation, t) / 360.0 / *spins_required
                } else {
                    1.0
                };
                let rate = if freq_modulate {
                    (20_000.0 / 44_100.0 + progress * 40_000.0 / 44_100.0).min(100_000.0 / 44_100.0)
                } else {
                    1.0
                };
                (rate, vol * env.clamp(0.0, 1.0), 256.0)
            }
        }
    }
}

/// All loop sounds of a replay (sorted by first audibility); the loop
/// counterpart of [`collect_events`]. `data` is the sample-side data
/// parsed with the beatmap (`game.sample_data`).
pub fn collect_loop_events(game: &GameData, data: &SampleData) -> Vec<LoopSoundEvent> {
    let mut out = Vec::new();
    if game.objects.len() != data.objects.len() {
        return out; // parser mismatch; silence rather than desynced sounds
    }
    for (obj, raw) in game.objects.iter().zip(&data.objects) {
        match obj.kind {
            ObjKind::Slider => {
                let runs = tracked_runs(game, obj);
                if runs.is_empty() {
                    continue;
                }
                let (obj_samples, obj_normal) = slider_object_samples(raw, obj, &data);
                for sample in slide_loop_samples(&obj_samples, obj_normal, raw.sound_type) {
                    out.push(LoopSoundEvent {
                        name: sample.name,
                        bank: sample.bank.as_str(),
                        volume: sample.volume,
                        runs: runs.clone(),
                        control: LoopControl::Slider { object_index: obj.index },
                    });
                }
            }
            ObjKind::Spinner => {
                if let Some(lp) = spin_loop_for(game, &data, obj, raw) {
                    out.push(LoopSoundEvent {
                        name: "spinnerspin",
                        bank: lp.sample.bank.as_str(),
                        volume: lp.sample.volume,
                        runs: lp.runs,
                        control: LoopControl::Spin { rotation: lp.rotation, spins_required: lp.spins_required },
                    });
                }
            }
            ObjKind::Circle => {}
        }
    }
    out.sort_by(|a, b| {
        let k = |e: &LoopSoundEvent| e.runs.first().map(|r| r.0).unwrap_or(f64::INFINITY);
        k(a).partial_cmp(&k(b)).unwrap()
    });
    out
}

// ---------------------------------------------------------------------------
// Skin samples (embedded ArgonPro hitsounds)
// ---------------------------------------------------------------------------

macro_rules! wav {
    ($file:literal) => {
        include_bytes!(concat!("../assets/sounds/ArgonPro/", $file, ".wav"))
    };
}

/// `Gameplay/combobreak` (a `SampleInfo`, no bank): not present in the
/// ArgonPro set at all, so the `ArgonProSkin.GetSample` chain falls to
/// `Gameplay/Argon/combobreak`. The spinner trio resolves from the Argon
/// level the same way (`spinnerspin` is the looping spin sound, the
/// bonus entries are the per-revolution rewards).
const COMBOBREAK: &[u8] = include_bytes!("../assets/sounds/Argon/combobreak.wav");
const SPINNERSPIN: &[u8] = include_bytes!("../assets/sounds/Argon/spinnerspin.wav");
const SPINNERBONUS: &[u8] = include_bytes!("../assets/sounds/Argon/spinnerbonus.wav");
const SPINNERBONUS_MAX: &[u8] = include_bytes!("../assets/sounds/Argon/spinnerbonus-max.wav");

/// `ArgonProSkin.GetSample` (osu.Game/Skinning/ArgonProSkin.cs) resolves
/// each `HitSampleInfo.LookupNames` entry through the skin's own samples
/// (none embedded), then `Gameplay/ArgonPro/`, then `Gameplay/Argon/`,
/// then the plain `Gameplay/` set. Every banked osu!standard gameplay
/// lookup EXISTS in the ArgonPro set — a present resource ends the chain,
/// even when it decodes to no PCM: the set's `sliderslide`/
/// `sliderwhistle` entries are empty (muted), so ArgonPro plays no
/// sliding sounds and the loop tiles degrade to silence instead of
/// falling through to the Argon copies. Bank-less lookups the ArgonPro
/// set doesn't carry (combobreak) resolve from the Argon level.
///
/// LookupNames: "Gameplay/{Bank}-{Name}{Suffix}" → "Gameplay/{Bank}-{Name}".
/// Suffix lookups need custom sample banks (index ≥ 2), which the
/// embedded set doesn't provide, so only the plain bank form applies.
fn sample_clip(sample: HitSample) -> Option<Clip> {
    let bytes: &[u8] = match (sample.bank.as_str(), sample.name) {
        ("normal", "hitnormal") => wav!("normal-hitnormal"),
        ("normal", "hitwhistle") => wav!("normal-hitwhistle"),
        ("normal", "hitfinish") => wav!("normal-hitfinish"),
        ("normal", "hitclap") => wav!("normal-hitclap"),
        ("normal", "slidertick") => wav!("normal-slidertick"),
        ("normal", "sliderslide") => wav!("normal-sliderslide"),
        ("normal", "sliderwhistle") => wav!("normal-sliderwhistle"),
        ("soft", "hitnormal") => wav!("soft-hitnormal"),
        ("soft", "hitwhistle") => wav!("soft-hitwhistle"),
        ("soft", "hitfinish") => wav!("soft-hitfinish"),
        ("soft", "hitclap") => wav!("soft-hitclap"),
        ("soft", "slidertick") => wav!("soft-slidertick"),
        ("soft", "sliderslide") => wav!("soft-sliderslide"),
        ("soft", "sliderwhistle") => wav!("soft-sliderwhistle"),
        ("drum", "hitnormal") => wav!("drum-hitnormal"),
        ("drum", "hitwhistle") => wav!("drum-hitwhistle"),
        ("drum", "hitfinish") => wav!("drum-hitfinish"),
        ("drum", "hitclap") => wav!("drum-hitclap"),
        ("drum", "slidertick") => wav!("drum-slidertick"),
        ("drum", "sliderslide") => wav!("drum-sliderslide"),
        ("drum", "sliderwhistle") => wav!("drum-sliderwhistle"),
        (_, "combobreak") => COMBOBREAK,
        (_, "spinnerspin") => SPINNERSPIN,
        (_, "spinnerbonus") => SPINNERBONUS,
        (_, "spinnerbonus-max") => SPINNERBONUS_MAX,
        _ => return None,
    };
    decode_wav(bytes)
}

// ---------------------------------------------------------------------------
// Sample resolution: user skin first, embedded ArgonPro fills the gaps
// ---------------------------------------------------------------------------

/// Per-render hitsound resolver — lazer's gameplay sample chain with the
/// beatmap-skin layer deliberately removed (this renderer never reads
/// beatmap skins). The USER skin is asked first through lazer's
/// `LegacySkin.GetSample` name chain (`getLegacyLookupNames`):
/// `UseCustomSampleBanks` is only ever true for beatmap skins, so for a
/// user skin the custom-sample-index suffix is always filtered out and
/// the candidates collapse to the unsuffixed names
/// `Gameplay/{bank}-{name}` and `Gameplay/{name}` (each expanding to its
/// last path piece inside [`crate::skin::Skin::get_sample`]), plus the
/// universal bank-less `{name}`. Every slot the skin leaves open is
/// filled by the embedded ArgonPro set ([`sample_clip`]) — elements MIX
/// between the two, exactly one provider per lookup. Both missing (or a
/// skin file that fails to decode, which lazer treats as a null sample
/// and walks past) → the slot is silent.
pub struct SampleResolver<'a> {
    skin: Option<&'a dyn crate::skin::Skin>,
    /// `OsuSkinConfiguration.SpinnerFrequencyModulate` (default true): the
    /// spinning loop's playback rate rises with spin progress.
    frequency_modulate: bool,
    cache: HashMap<(Bank, &'static str), Option<Arc<Clip>>>,
}

impl<'a> SampleResolver<'a> {
    pub fn new(skin: &'a dyn crate::skin::Skin) -> Self {
        SampleResolver {
            skin: skin.is_legacy().then_some(skin),
            frequency_modulate: skin
                .get_config(crate::skin::SkinLookup::Generic("SpinnerFrequencyModulate".into()))
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            cache: HashMap::new(),
        }
    }

    /// The embedded ArgonPro set alone (no user skin in scope).
    fn builtin() -> SampleResolver<'static> {
        SampleResolver { skin: None, frequency_modulate: true, cache: HashMap::new() }
    }

    /// Resolve one sample: user skin file → embedded ArgonPro entry →
    /// silent. Results (including negatives) are cached per (bank, name).
    fn clip(&mut self, sample: HitSample) -> Option<Arc<Clip>> {
        if let Some(hit) = self.cache.get(&(sample.bank, sample.name)) {
            return hit.clone();
        }
        let mut resolved = self
            .skin
            .and_then(|skin| lookup_candidates(sample.bank.as_str(), sample.name).iter().find_map(|n| skin.get_sample(n)));
        // `SpinnerBonusMaxSampleInfo.LookupNames`: the "-max" suffix
        // strips back to the plain entry — a skin with only
        // "spinnerbonus" serves the max spins too.
        if resolved.is_none()
            && let Some(stripped) = sample.name.strip_suffix("-max")
        {
            resolved = self
                .skin
                .and_then(|skin| lookup_candidates(sample.bank.as_str(), stripped).iter().find_map(|n| skin.get_sample(n)));
        }
        let clip = match resolved {
            Some(path) => match decode_sample_file(&path) {
                Ok(clip) => Some(Arc::new(clip)),
                Err(e) => {
                    eprintln!("hitsound warning: skin sample {} failed to decode ({}), falling back", path.display(), e);
                    None
                }
            },
            None => None,
        }
        .or_else(|| sample_clip(sample).map(Arc::new));
        if clip.is_none() {
            eprintln!(
                "hitsound warning: no sample for {}-{} in the skin or the default set, its hitsounds are silent",
                sample.bank.as_str(),
                sample.name
            );
        }
        self.cache.insert((sample.bank, sample.name), clip.clone());
        clip
    }

    /// Distinct (bank, name) slots resolved so far (debug output).
    fn distinct(&self) -> usize {
        self.cache.len()
    }
}

/// `HitSampleInfo.LookupNames` order for a user legacy skin (suffix
/// always filtered: `UseCustomSampleBanks` is beatmap-skin-only).
/// `Gameplay/combobreak` is a plain `SampleInfo` with no bank.
fn lookup_candidates(bank: &str, name: &str) -> Vec<String> {
    if name == "combobreak" {
        vec!["Gameplay/combobreak".to_string()]
    } else {
        vec![format!("Gameplay/{bank}-{name}"), format!("Gameplay/{name}")]
    }
}

/// Bytes-level resolution for live-playback hosts that own their audio
/// engine (decode the returned WAV themselves): the user skin's file
/// first under the same mix policy (skin WAVs pass through untouched,
/// mp3/ogg re-encode through ffmpeg), else the embedded ArgonPro entry.
pub fn resolve_sample_wav(bank: &str, name: &str, skin: &dyn crate::skin::Skin) -> Option<Vec<u8>> {
    if skin.is_legacy() {
        let mut names = lookup_candidates(bank, name);
        // `SpinnerBonusMaxSampleInfo` strips "-max" back to the plain
        // entry (`SampleResolver::clip` mirrors this).
        if let Some(stripped) = name.strip_suffix("-max") {
            names.extend(lookup_candidates(bank, stripped));
        }
        let path = names.iter().find_map(|n| skin.get_sample(n));
        if let Some(path) = path {
            let is_wav = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
            if is_wav {
                if let Ok(bytes) = std::fs::read(&path) {
                    return Some(bytes);
                }
            } else if let Some(clip) = ffmpeg_pcm(&path) {
                return Some(encode_wav(&clip.data, false));
            }
        }
    }
    sample_bytes(bank, name).map(|b| b.to_vec())
}

/// Decode a skin sample file: WAV in-process ([`decode_wav`]); anything
/// else (mp3/ogg/...) through the local ffmpeg binary the export path
/// already encodes with.
fn decode_sample_file(path: &Path) -> Result<Clip, String> {
    let is_wav = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
    if is_wav {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        return decode_wav(&bytes).ok_or_else(|| "not a decodable RIFF/PCM wav".to_string());
    }
    ffmpeg_pcm(path).ok_or_else(|| "ffmpeg decode failed".to_string())
}

/// `ffmpeg -i <file> -map a:0 -f s16le -ar 44100 -ac 2 pipe:1` — PCM16
/// on stdout, converted to the interleaved f32 clip format.
fn ffmpeg_pcm(path: &Path) -> Option<Clip> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "a:0", "-f", "s16le", "-acodec", "pcm_s16le", "-ar", "44100", "-ac", "2", "pipe:1"])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let data = out
        .stdout
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect();
    Some(Clip { data })
}

// ---------------------------------------------------------------------------
// WAV decode / mix / encode
// ---------------------------------------------------------------------------

/// Decoded clip: interleaved stereo f32 at `SAMPLE_RATE`.
struct Clip {
    data: Vec<f32>,
}

impl Clip {
    fn duration_ms(&self) -> f64 {
        self.data.len() as f64 / 2.0 / SAMPLE_RATE as f64 * 1000.0
    }
}

/// Minimal RIFF PCM decoder (16/24-bit, plain or WAVE_FORMAT_EXTENSIBLE)
/// with linear resampling to the output rate; mono sources are duplicated
/// to both channels.
fn decode_wav(bytes: &[u8]) -> Option<Clip> {
    let rd = |i: usize| -> u16 {
        u16::from_le_bytes([*bytes.get(i).unwrap_or(&0), *bytes.get(i + 1).unwrap_or(&0)])
    };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (audio format, channels, rate, bits)
    let mut data: Option<(usize, usize)> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
        let body = pos + 8;
        if body + size > bytes.len() {
            break;
        }
        match id {
            b"fmt " => {
                let mut audio_format = rd(body);
                let channels = rd(body + 2);
                let rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
                let bits = rd(body + 14);
                // WAVE_FORMAT_EXTENSIBLE: the real tag is the SubFormat
                // GUID's Data1 (1 = PCM) at offset 24. The shipped
                // soft-hitnormal is such a file.
                if audio_format == 0xFFFE && size >= 40 {
                    let sub = u32::from_le_bytes([
                        bytes[body + 24], bytes[body + 25], bytes[body + 26], bytes[body + 27],
                    ]);
                    audio_format = sub as u16;
                }
                fmt = Some((audio_format, channels, rate, bits));
            }
            b"data" => data = Some((body, size)),
            _ => {}
        }
        pos = body + size + (size & 1);
    }
    let (format, channels, rate, bits) = fmt?;
    let (body, size) = data?;
    let channels = channels as usize;
    if format != 1 || (bits != 16 && bits != 24) || channels == 0 || channels > 2 || rate == 0 {
        return None;
    }
    let width = bits as usize / 8;
    let frames = size / width / channels;
    let src = |f: usize, c: usize| -> f32 {
        let off = body + (f * channels + c) * width;
        if bits == 16 {
            rd(off) as i16 as f32 / 32768.0
        } else {
            // 3 bytes LE, sign-extended to i32.
            let v = ((rd(off) as u32) | ((rd(off + 1) as u32) << 8) | ((*bytes.get(off + 2).unwrap_or(&0) as u32) << 16)) << 8;
            (v as i32 >> 8) as f32 / 8388608.0
        }
    };
    let mut data = Vec::with_capacity(frames * 2);
    if rate == SAMPLE_RATE {
        for f in 0..frames {
            let l = src(f, 0);
            let r = if channels == 2 { src(f, 1) } else { l };
            data.push(l);
            data.push(r);
        }
    } else {
        let out_frames = ((frames as f64) * (SAMPLE_RATE as f64 / rate as f64)).round() as usize;
        for f in 0..out_frames {
            let t = f as f64 * rate as f64 / SAMPLE_RATE as f64;
            let i = t.floor() as usize;
            let frac = (t - i as f64) as f32;
            let j = (i + 1).min(frames.saturating_sub(1));
            let l = src(i, 0) * (1.0 - frac) + src(j, 0) * frac;
            let r = if channels == 2 {
                src(i, 1) * (1.0 - frac) + src(j, 1) * frac
            } else {
                l
            };
            data.push(l);
            data.push(r);
        }
    }
    Some(Clip { data })
}

/// Mixes one placement into `buf` (interleaved stereo f32) at the clip's
/// natural rate — samples never pitch-shift with rate mods, DT/HT only
/// move the trigger times. `until_sec` truncates loop tiles where lazer
/// would stop the looping sample.
fn place(buf: &mut [f32], clip: &Clip, start_sec: f64, gl: f32, gr: f32, until_sec: Option<f64>) {
    let frames = clip.data.len() / 2;
    if frames == 0 {
        return;
    }
    let dst_start = (start_sec * SAMPLE_RATE as f64).round() as isize;
    let mut dst_len = frames;
    if let Some(until) = until_sec {
        let cut = ((until * SAMPLE_RATE as f64).round() as isize - dst_start).max(0) as usize;
        dst_len = dst_len.min(cut);
    }
    for n in 0..dst_len {
        let dst = dst_start + n as isize;
        if dst < 0 {
            continue;
        }
        let dst = dst as usize;
        if dst * 2 + 1 >= buf.len() {
            break;
        }
        buf[dst * 2] += clip.data[n * 2] * gl;
        buf[dst * 2 + 1] += clip.data[n * 2 + 1] * gr;
    }
}

/// Renders the hitsound track for the exported range and encodes it as a
/// PCM16 stereo WAV. `t0` is the first output frame's map time,
/// `wall_secs` the output video's duration in seconds; the track spans
/// exactly that wall window so it muxes 1:1 with the video.
/// `master_gain` scales the whole bus (`--hitsounds-volume`).
///
/// Loudness follows the game's defaults: samples play at their authored
/// level (beatmap volume × the sample's mastering, Effect channel 1.0),
/// no bus normalization. Stacked hits sum in float and the encoder's
/// soft limiter replaces the DAC clipping the game would do.
/// A one-shot hitsound event on the map timeline, for live preview
/// playback (fire when the playhead crosses `time`). Loop sounds
/// (sliderslide/sliderwhistle/spinnerspin) are separate: see
/// [`collect_loop_events`].
#[derive(Clone, Copy, Debug)]
pub struct HitsoundEvent {
    /// Map time in ms (judgement time).
    pub time: f64,
    /// Sample name: "hitnormal"/"hitwhistle"/"hitfinish"/"hitclap"/
    /// "slidertick"/"spinnerbonus"/"spinnerbonus-max"/"combobreak".
    pub name: &'static str,
    /// Sample bank: "normal"/"soft"/"drum".
    pub bank: &'static str,
    /// Beatmap sample volume 0-100 (apply `max(5)` and the Effect
    /// channel volume on the receiver's side).
    pub volume: i32,
    /// Playfield X (0..512) for stereo balance.
    pub pan_x: f32,
}

// ---------------------------------------------------------------------------
// Spinner spinning loop (pitch-modulated, phase-accumulator synth)
// ---------------------------------------------------------------------------

/// `DrawableSpinner`'s loop fires on `IsSpinning` — the rotation outpaces
/// the damped display by >10°, which in steady state means a sustained
/// rate above ~100°/s (10° / -ln(0.99)) inside the spinnable window.
const SPIN_ACTIVE_RATE: f64 = 0.1005; // deg per ms
/// Damp stickiness: a pause shorter than this keeps the loop alive.
const SPIN_RUN_MERGE_GAP: f64 = 100.0;
/// `updateSpinningSample`: VolumeTo(1, 300) on start, VolumeTo(0, 240) on
/// stop.
const SPIN_FADE_IN: f64 = 300.0;
const SPIN_FADE_OUT: f64 = 240.0;

/// One spinner's looping `spinnerspin` descriptor. Not a [`Placement`]:
/// the loop plays at a continuously VARYING rate (frequency modulation),
/// which fixed-rate one-shots cannot express — `render_spin_loops`
/// synthesizes it directly with a phase accumulator.
struct SpinLoop {
    /// `referenceSample.With("spinnerspin")`: the .osu line's first
    /// sample's bank/volume with the name swapped.
    sample: HitSample,
    /// Audibly-spinning intervals (map ms).
    runs: Vec<(f64, f64)>,
    /// (snapshot time, score-side total rotation) across the spinner, for
    /// the modulation curve.
    rotation: Vec<(f64, f32)>,
    spins_required: f64,
}

fn collect_spin_loops(game: &GameData, data: &SampleData) -> Vec<SpinLoop> {
    let mut loops = Vec::new();
    if game.objects.len() != data.objects.len() {
        return loops;
    }
    for (obj, raw) in game.objects.iter().zip(&data.objects) {
        if obj.kind != ObjKind::Spinner {
            continue;
        }
        if let Some(lp) = spin_loop_for(game, data, obj, raw) {
            loops.push(lp);
        }
    }
    loops
}

/// One spinner's loop descriptor (rotation timeline + audibly-spinning
/// runs with the 100ms stickiness merge).
fn spin_loop_for(game: &GameData, data: &SampleData, obj: &crate::game::ObjView, raw: &RawObj) -> Option<SpinLoop> {
    let mut rotation = Vec::new();
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<f64> = None;
    let mut last_run_end = f64::NEG_INFINITY;
    let mut prev: Option<(f64, f32)> = None;
    for (si, snap) in game.snapshots.iter().enumerate() {
        let Some(sp) = snap.spinners.iter().find(|sp| sp.object_index == obj.index) else {
            continue;
        };
        rotation.push((snap.time, sp.total_rotation));
        let active = match prev {
            Some((pt, prot)) => {
                let dt = snap.time - pt;
                // Spinnable window only; movement above the threshold.
                snap.time >= obj.start_time
                    && snap.time < obj.end_time
                    && dt > 0.0
                    && (sp.total_rotation - prot).abs() as f64 / dt > SPIN_ACTIVE_RATE
            }
            None => false,
        };
        // The run extends at least to the NEXT snapshot (the active
        // judgement was made over the interval ending here).
        let cur_end = game.snapshots.get(si + 1).map(|s| s.time).unwrap_or(snap.time);
        match (&mut open, active) {
            (None, true) => {
                if snap.time - last_run_end > SPIN_RUN_MERGE_GAP {
                    open = Some(snap.time);
                } else if let Some(last) = runs.last_mut() {
                    // Re-activate within the stickiness gap: extend the
                    // previous run instead of starting a new fade-in.
                    last.1 = cur_end.max(last.1);
                    open = Some(last.0);
                } else {
                    open = Some(snap.time);
                }
            }
            (Some(_), true) => {
                if let Some(last) = runs.last_mut() {
                    last.1 = cur_end.max(last.1);
                }
            }
            (Some(start), false) => {
                runs.push((*start, snap.time));
                last_run_end = snap.time;
                open = None;
            }
            (None, false) => {}
        }
        prev = Some((snap.time, sp.total_rotation));
    }
    if let Some(start) = open {
        runs.push((start, obj.end_time));
    }
    if runs.is_empty() {
        return None;
    }
    let point = point_at(&data.points, raw.start_time);
    Some(SpinLoop {
        sample: HitSample {
            name: "spinnerspin",
            bank: raw.bank.normal.unwrap_or(point.bank),
            volume: if raw.bank.volume > 0 { raw.bank.volume } else { point.volume },
        },
        runs,
        rotation,
        spins_required: obj.spins_required,
    })
}

/// Linear-interpolated score-side rotation at a map time.
fn rotation_at(rot: &[(f64, f32)], t: f64) -> f64 {
    match rot.binary_search_by(|(time, _)| time.partial_cmp(&t).unwrap()) {
        Ok(i) => rot[i].1 as f64,
        Err(0) => rot.first().map(|r| r.1 as f64).unwrap_or(0.0),
        Err(i) => {
            let (t0, r0) = rot[i - 1];
            match rot.get(i) {
                Some(&(t1, r1)) => {
                    if t1 > t0 {
                        r0 as f64 + (r1 - r0) as f64 * (t - t0) / (t1 - t0)
                    } else {
                        r0 as f64
                    }
                }
                None => r0 as f64,
            }
        }
    }
}

/// Synthesize the spinning loops straight into the mix buffer (clip
/// resolution via the resolver, then the pure [`synth_spin_loop`]).
fn render_spin_loops(buf: &mut [f32], loops: &[SpinLoop], resolver: &mut SampleResolver, t0: f64, rate: f64) {
    for lp in loops {
        let Some(clip) = resolver.clip(lp.sample) else { continue };
        synth_spin_loop(buf, &clip.data, lp, resolver.frequency_modulate, t0, rate);
    }
}

/// The read phase advances by the modulation ratio each output frame and
/// wraps at the clip length (seamless: the sample is authored to loop),
/// gain = line volume × the tracking envelope. `progressUnclamped` keeps
/// rising past 1.0, capped at the `100_000/44_100` frequency ceiling.
fn synth_spin_loop(buf: &mut [f32], data: &[f32], lp: &SpinLoop, freq_modulate: bool, t0: f64, rate: f64) {
    let frames = data.len() / 2;
    if frames == 0 {
        return;
    }
    let vol = lp.sample.volume.max(MINIMUM_SAMPLE_VOLUME) as f32 / 100.0;
    for &(a, b) in &lp.runs {
        let frame_at = |map_ms: f64| ((map_ms - t0) / rate / 1000.0 * SAMPLE_RATE as f64).round() as i64;
        let fa = frame_at(a);
        let fe = frame_at(b + SPIN_FADE_OUT);
        let mut phase = 0.0f64;
        let buf_frames = (buf.len() / 2) as i64;
        let mut i = fa.max(0);
        while i < fe.min(buf_frames) {
            let map_t = t0 + i as f64 / SAMPLE_RATE as f64 * rate * 1000.0;
            let env = if map_t <= b {
                ((map_t - a) / SPIN_FADE_IN).clamp(0.0, 1.0)
            } else {
                1.0 - (map_t - b) / SPIN_FADE_OUT
            };
            if env > 0.0 {
                let progress = if lp.spins_required > 0.0 {
                    rotation_at(&lp.rotation, map_t) / 360.0 / lp.spins_required
                } else {
                    1.0
                };
                let ratio = if freq_modulate {
                    (20_000.0 / 44_100.0 + progress * 40_000.0 / 44_100.0).min(100_000.0 / 44_100.0)
                } else {
                    1.0
                };
                phase += ratio;
                let p0 = (phase as usize) % frames;
                let p1 = (p0 + 1) % frames;
                let frac = (phase - phase.floor()) as f32;
                let l = data[p0 * 2] + (data[p1 * 2] - data[p0 * 2]) * frac;
                let r = data[p0 * 2 + 1] + (data[p1 * 2 + 1] - data[p0 * 2 + 1]) * frac;
                let g = vol * env as f32;
                buf[(i as usize) * 2] += l * g;
                buf[(i as usize) * 2 + 1] += r * g;
            }
            i += 1;
        }
    }
}


/// All one-shot hitsound events of a replay (sorted by time, loop
/// sounds excluded — those are [`collect_loop_events`]). `data` is the
/// sample-side data parsed with the beatmap (`game.sample_data`).
/// Volume/bank resolution follows the same lazer semantics as the
/// offline track.
pub fn collect_events(game: &GameData, data: &SampleData) -> Vec<HitsoundEvent> {
    let mut events: Vec<HitsoundEvent> = build_placements(game, data, f64::NEG_INFINITY, f64::INFINITY, &mut SampleResolver::builtin())
        .into_iter()
        .filter(|p| p.until.is_none())
        .map(|p| HitsoundEvent {
            time: p.time,
            name: p.sample.name,
            bank: p.sample.bank.as_str(),
            volume: p.sample.volume,
            pan_x: p.x,
        })
        .collect();
    events.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    events
}

/// Embedded skin sample bytes for a (bank, name) pair (ArgonPro set,
/// combobreak from the Argon set per the lookup chain). PCM WAV, for the
/// caller's own decoder.
pub fn sample_bytes(bank: &str, name: &str) -> Option<&'static [u8]> {
    let bytes: &[u8] = match (bank, name) {
        ("normal", "hitnormal") => wav!("normal-hitnormal"),
        ("normal", "hitwhistle") => wav!("normal-hitwhistle"),
        ("normal", "hitfinish") => wav!("normal-hitfinish"),
        ("normal", "hitclap") => wav!("normal-hitclap"),
        ("normal", "slidertick") => wav!("normal-slidertick"),
        ("normal", "sliderslide") => wav!("normal-sliderslide"),
        ("normal", "sliderwhistle") => wav!("normal-sliderwhistle"),
        ("soft", "hitnormal") => wav!("soft-hitnormal"),
        ("soft", "hitwhistle") => wav!("soft-hitwhistle"),
        ("soft", "hitfinish") => wav!("soft-hitfinish"),
        ("soft", "hitclap") => wav!("soft-hitclap"),
        ("soft", "slidertick") => wav!("soft-slidertick"),
        ("soft", "sliderslide") => wav!("soft-sliderslide"),
        ("soft", "sliderwhistle") => wav!("soft-sliderwhistle"),
        ("drum", "hitnormal") => wav!("drum-hitnormal"),
        ("drum", "hitwhistle") => wav!("drum-hitwhistle"),
        ("drum", "hitfinish") => wav!("drum-hitfinish"),
        ("drum", "hitclap") => wav!("drum-hitclap"),
        ("drum", "slidertick") => wav!("drum-slidertick"),
        ("drum", "sliderslide") => wav!("drum-sliderslide"),
        ("drum", "sliderwhistle") => wav!("drum-sliderwhistle"),
        (_, "combobreak") => COMBOBREAK,
        (_, "spinnerspin") => SPINNERSPIN,
        (_, "spinnerbonus") => SPINNERBONUS,
        (_, "spinnerbonus-max") => SPINNERBONUS_MAX,
        _ => return None,
    };
    Some(bytes)
}

pub fn render_track_wav(game: &GameData, data: &SampleData, t0: f64, wall_secs: f64, rate: f64, master_gain: f32, skin: &dyn crate::skin::Skin) -> Vec<u8> {
    render_track(game, data, t0, wall_secs, rate, master_gain, true, skin)
}

/// `render_track_wav` without the bus soft limiter: the sum stays
/// linear and only true `>= 1.0` overs saturate at the PCM16
/// conversion. `scale` is PCM headroom the caller divides back out
/// before applying its own channel/master chain — for mixes that clip
/// exactly once at the end (live-render parity). A tanh knee on this
/// bus ducked dense stacks 2-4 dB and bent every stack peak; the
/// float-sum mix only needs the headroom so PCM16 can carry stacks
/// above unity.
pub fn render_track_wav_linear(game: &GameData, data: &SampleData, t0: f64, wall_secs: f64, rate: f64, scale: f32, skin: &dyn crate::skin::Skin) -> Vec<u8> {
    render_track(game, data, t0, wall_secs, rate, scale, false, skin)
}

fn render_track(game: &GameData, data: &SampleData, t0: f64, wall_secs: f64, rate: f64, gain: f32, limit: bool, skin: &dyn crate::skin::Skin) -> Vec<u8> {
    let t_map_end = t0 + wall_secs * rate * 1000.0;
    let mut resolver = SampleResolver::new(skin);
    let placements = build_placements(game, &data, t0, t_map_end, &mut resolver);
    if std::env::var("HITSOUND_DEBUG").is_ok() {
        eprintln!("hitsound debug: parsed {} objects (game {}), {} points, {} placements in [{},{}]",
            data.objects.len(), game.objects.len(), data.points.len(), placements.len(), t0, t_map_end);
    }

    let total = (wall_secs.max(0.0) * SAMPLE_RATE as f64).round() as usize;
    let mut buf = vec![0.0f32; total * 2];

    // Decode each distinct (bank, name) once; a sample that fails to
    // decode warns once and drops only its own placements (a silent
    // `continue` here hid a 24-bit asset mismatch before).
    for p in &placements {
        let Some(clip) = resolver.clip(p.sample) else {
            continue;
        };
        let volume = p.sample.volume.max(MINIMUM_SAMPLE_VOLUME) as f32 / 100.0;
        let bal = balance(p.x) as f32;
        let gl = volume * (1.0 - bal.max(0.0));
        let gr = volume * (1.0 - (-bal).max(0.0));
        let wall = (p.time - t0) / rate / 1000.0;
        let until = p.until.map(|u| (u - t0) / rate / 1000.0);
        place(&mut buf, &clip, wall, gl, gr, until);
    }

    // The spinner spinning loops (pitch-modulated, phase-accumulator
    // synthesis — see `render_spin_loops`).
    let loops = collect_spin_loops(game, &data);
    if std::env::var("HITSOUND_DEBUG").is_ok() {
        for lp in &loops {
            eprintln!(
                "hitsound debug: spin loop bank={} vol={} spins_req={} runs={:?} rot[0..3]={:?} rot[-1]={:?}",
                lp.sample.bank.as_str(),
                lp.sample.volume,
                lp.spins_required,
                lp.runs,
                &lp.rotation[..lp.rotation.len().min(3)],
                lp.rotation.last()
            );
        }
    }
    render_spin_loops(&mut buf, &loops, &mut resolver, t0, rate);

    // Master gain / headroom scale (`--hitsounds-volume`, or the linear
    // variant's PCM headroom).
    if (gain - 1.0).abs() > 1e-6 {
        for v in &mut buf {
            *v *= gain;
        }
    }

    if std::env::var("HITSOUND_DEBUG").is_ok() {
        let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        eprintln!("hitsound debug: {} clips decoded, buffer peak {:.4}", resolver.distinct(), peak);
        let mut names: Vec<(usize, &str)> = Vec::new();
        for p in &placements {
            match names.iter_mut().find(|(_, n)| *n == p.sample.name) {
                Some((c, _)) => *c += 1,
                None => names.push((1, p.sample.name)),
            }
        }
        eprintln!("hitsound debug: {:?}", names);
        for p in placements.iter().take(6) {
            eprintln!("hitsound debug: t={:.0} {} {} vol={} x={:.0}", p.time, p.sample.bank.as_str(), p.sample.name, p.sample.volume, p.x);
        }
    }

    encode_wav(&buf, limit)
}

/// Soft-knee limiter (tanh above the threshold). The game's BASS mixer
/// sums channels in float and only clips at the DAC; stacking samples
/// must not hard-clip inside the track, or dense sections turn into
/// rail-slamming squares that bury the BGM once summed again in ffmpeg.
fn soft_limit(x: f32) -> f32 {
    const T: f32 = 0.7;
    let a = x.abs();
    if a <= T {
        x
    } else {
        let t = (a - T) / (1.0 - T);
        let limited = T + (1.0 - T) * t.tanh();
        limited.copysign(x)
    }
}

fn encode_wav(interleaved: &[f32], limit: bool) -> Vec<u8> {
    let frames = interleaved.len() / 2;
    let data_len = frames * 4;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    out.reserve(data_len);
    for v in interleaved {
        // f32 -> i16 `as` casts saturate: the linear variant's only clip.
        let s = if limit { soft_limit(*v) } else { *v };
        out.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped `soft-hitnormal` is the set's only 24-bit
    /// WAVE_FORMAT_EXTENSIBLE file; the decoder must carry it (it was
    /// silently dropped before, muting every soft-bank hit).
    #[test]
    fn decodes_24bit_extensible_wav() {
        let clip = decode_wav(wav!("soft-hitnormal")).expect("24-bit EXTENSIBLE wav decodes");
        assert!((930.0..950.0).contains(&clip.duration_ms()));
        let peak = clip.data.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.5, "decoded content should be loud, peak={peak}");
    }

    #[test]
    fn decodes_16bit_wav() {
        let clip = decode_wav(wav!("normal-hitnormal")).expect("16-bit wav decodes");
        assert!(clip.duration_ms() > 500.0);
        assert!(!clip.data.is_empty());
    }

    /// The API-level regression: soft hitnormal resolves to a playable clip.
    #[test]
    fn soft_hitnormal_resolves_to_clip() {
        let sample = HitSample { name: "hitnormal", bank: Bank::Soft, volume: 100 };
        assert!(sample_clip(sample).is_some());
    }

    /// Samples land sample-for-sample at their natural rate — rate mods
    /// move trigger times only, never the playback speed/pitch — and
    /// `until` truncates loop tiles at the wall-time cutoff.
    #[test]
    fn place_copies_at_natural_rate() {
        // 100-frame ramp, one channel duplicated to stereo.
        let clip = Clip { data: (0..100).flat_map(|i| [i as f32 / 100.0; 2]).collect() };
        let mut buf = vec![0.0f32; 30000 * 2];
        place(&mut buf, &clip, 0.5, 1.0, 1.0, None);
        for f in 0..100 {
            assert_eq!(buf[(22050 + f) * 2], clip.data[f * 2]);
        }
        assert_eq!(buf[(22050 + 100) * 2], 0.0, "must not ring past the natural length");

        let mut cut = vec![0.0f32; 30000 * 2];
        place(&mut cut, &clip, 0.5, 1.0, 1.0, Some(0.5 + 50.0 / SAMPLE_RATE as f64));
        assert_eq!(cut[(22050 + 49) * 2], clip.data[49 * 2]);
        assert_eq!(cut[(22050 + 50) * 2], 0.0, "cut at until");
    }

    /// A user skin sample takes priority over the embedded set, missing
    /// slots mix with it, and lookups nothing provides are silent.
    #[test]
    fn resolver_mixes_skin_with_builtin() {
        let dir = std::env::temp_dir().join(format!("osr_hitsound_skin_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 50ms of near-silence - the builtin hitwhistle is much longer,
        // so which provider answered is observable via the duration.
        let short = encode_wav(&vec![0.01f32; 4410], false);
        std::fs::write(dir.join("normal-hitwhistle.wav"), &short).unwrap();
        // Bank-less universal name: "hitclap" with no bank prefix.
        std::fs::write(dir.join("hitclap.wav"), &short).unwrap();
        std::fs::write(dir.join("skin.ini"), "[General]\nVersion: 2.5\n").unwrap();

        let skin = crate::skin::load_skin(Some(&dir)).unwrap();
        assert!(skin.is_legacy());
        let mut resolver = SampleResolver::new(&skin);

        let whistled = resolver.clip(HitSample { name: "hitwhistle", bank: Bank::Normal, volume: 100 }).unwrap();
        assert!((45.0..55.0).contains(&whistled.duration_ms()), "skin file wins, got {}ms", whistled.duration_ms());
        let builtin_whistle = sample_clip(HitSample { name: "hitwhistle", bank: Bank::Normal, volume: 100 }).unwrap();
        assert!(builtin_whistle.duration_ms() > 100.0);

        // The skin has no drum-hitfinish: the embedded ArgonPro copy fills it.
        let finish = resolver.clip(HitSample { name: "hitfinish", bank: Bank::Drum, volume: 100 }).unwrap();
        assert_eq!(finish.duration_ms(), sample_clip(HitSample { name: "hitfinish", bank: Bank::Drum, volume: 100 }).unwrap().duration_ms());

        // Universal bank-less file serves every bank's hitclap (lazer's
        // `Gameplay/{Name}` + raw-name tail of `getLegacyLookupNames`).
        let clap = resolver.clip(HitSample { name: "hitclap", bank: Bank::Soft, volume: 100 }).unwrap();
        assert!((45.0..55.0).contains(&clap.duration_ms()));

        // Nothing provides this: silent, and cached as such.
        assert!(resolver.clip(HitSample { name: "nosuchsound", bank: Bank::Soft, volume: 100 }).is_none());
        assert!(resolver.clip(HitSample { name: "nosuchsound", bank: Bank::Soft, volume: 100 }).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `SampleResolver::new` on the builtin skin (no `--skin`) keeps the
    /// embedded set as the only provider.
    #[test]
    fn resolver_builtin_skin_uses_embedded_set() {
        let skin = crate::skin::load_skin(None).unwrap();
        let mut resolver = SampleResolver::new(&skin);
        let clip = resolver.clip(HitSample { name: "hitnormal", bank: Bank::Normal, volume: 100 }).unwrap();
        let builtin = sample_clip(HitSample { name: "hitnormal", bank: Bank::Normal, volume: 100 }).unwrap();
        assert_eq!(clip.duration_ms(), builtin.duration_ms());
    }

    /// The spinner trio resolves from the embedded Argon level (the
    /// ArgonPro set carries no spinner entries — lazer's chain falls
    /// through the same way).
    #[test]
    fn spinner_samples_resolve_from_argon_layer() {
        for name in ["spinnerspin", "spinnerbonus", "spinnerbonus-max"] {
            let sample = HitSample { name, bank: Bank::Normal, volume: 100 };
            assert!(sample_clip(sample).is_some(), "{name} missing from the embedded set");
        }
    }

    /// `collect_loop_events` on an autoplay one-slider-one-spinner map.
    #[test]
    fn collect_loop_events_autoplay() {
        let map = "osu file format v14\n\
                   \n[General]\nMode: 0\n\
                   \n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:8\nApproachRate:8\nSliderMultiplier:1.8\nSliderTickRate:1\n\
                   \n[TimingPoints]\n500,400,4,2,0,40,1,0\n\
                   \n[HitObjects]\n\
                   100,100,1000,2,0,B|300:100,1,200,0,0:0:0:0:0,0:0:0:0:0\n\
                   256,192,2000,12,0,3500,0:0:0:0:0\n";
        let dir = std::env::temp_dir().join(format!("osr_loop_events_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loops.osu");
        std::fs::write(&path, map).unwrap();
        let game = crate::game::load_autoplay(path.to_str().unwrap()).unwrap();

        let events = collect_loop_events(&game, &osu_parse::samples::parse(map));
        let slide = events.iter().find(|e| e.name == "sliderslide").expect("sliderslide event");
        let spin = events.iter().find(|e| e.name == "spinnerspin").expect("spinnerspin event");
        assert_eq!(events.len(), 2, "no whistle flag on the slider: slide + spin only");

        // Both resolve bank/volume from the timing point active at the
        // object (soft, 40%).
        for e in [&slide, &spin] {
            assert_eq!(e.bank, "soft");
            assert_eq!(e.volume, 40);
        }

        // Slide: exactly the tracking interval, hard start/stop.
        assert_eq!(slide.runs.len(), 1);
        let (a, b) = slide.runs[0];
        assert!((995.0..1020.0).contains(&a), "slide run starts at the slider head, got {a}");
        assert!(b > a + 100.0, "slide run spans the slider body, got [{a},{b}]");
        assert!(slide.run_at(a).is_some() && slide.run_at(b).is_some());
        assert!(slide.run_at(a - 1.0).is_none());
        assert!(slide.run_at(b + 1.0).is_none(), "no fade tail on slide runs");
        let (rate, amp, x) = slide.params_at(0, a + (b - a) / 2.0, true, &game);
        assert_eq!(rate, 1.0);
        assert!((amp - 0.4).abs() < 1e-9, "slide amplitude = line volume, got {amp}");
        assert!((100.0..=300.0).contains(&x), "slide pan follows the ball, got {x}");

        // Spin: run inside the spinnable window [2000, 3500].
        assert!(!spin.runs.is_empty());
        let (a, b) = spin.runs[0];
        assert!(a >= 2000.0 && a < 2150.0, "spin run starts after the spinner opens, got {a}");
        assert!(b > 3000.0 && b <= 3500.0 + 1000.0 / 60.0 + 1.0, "spin run ends at the spinner close (+≤1 frame), got {b}");
        assert!(spin.run_at(b + SPIN_FADE_OUT).is_some(), "fade tail keeps the run audible");
        assert!(spin.run_at(b + SPIN_FADE_OUT + 1.0).is_none());

        // Envelope: silent at the run start, line volume after the 300ms
        // fade-in; playback rate starts below 1 (frequency modulation at
        // zero progress) and rises with spin progress.
        let (_, amp0, _) = spin.params_at(0, a, true, &game);
        assert!(amp0 < 0.01, "fade-in starts silent, got {amp0}");
        let (_, amp1, _) = spin.params_at(0, a + SPIN_FADE_IN, true, &game);
        assert!((amp1 - 0.4).abs() < 1e-9, "envelope reaches line volume, got {amp1}");
        let (r0, _, _) = spin.params_at(0, a + SPIN_FADE_IN, true, &game);
        let (r1, _, _) = spin.params_at(0, b, true, &game);
        assert!(r0 < 1.0, "modulated rate starts below natural, got {r0}");
        assert!(r1 > r0, "rate rises with progress: {r0} -> {r1}");
        let (rn, _, _) = spin.params_at(0, a + SPIN_FADE_IN, false, &game);
        assert_eq!(rn, 1.0, "SpinnerFrequencyModulate off pins the natural rate");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A 200Hz sine through the spin-loop synth: with modulation the
    /// ratio at zero progress is `20_000/44_100` (pitch drops to ~91Hz),
    /// with `SpinnerFrequencyModulate` off it stays at the natural rate,
    /// and the envelope gates the run (300ms in / 240ms out).
    #[test]
    fn spin_loop_modulates_pitch_and_envelopes() {
        let sr = SAMPLE_RATE as usize;
        let clip: Vec<f32> = (0..sr / 2)
            .flat_map(|i| {
                let v = (2.0 * std::f32::consts::PI * 200.0 * i as f32 / sr as f32).sin();
                [v, v]
            })
            .collect();
        // Constant rotation rate: 144 deg/s -> progress 0.4/s with
        // spins_required = 1 (stays under the 2.27x frequency cap).
        let lp = SpinLoop {
            sample: HitSample { name: "spinnerspin", bank: Bank::Normal, volume: 100 },
            runs: vec![(100.0, 900.0)],
            rotation: (0..=20).map(|i| (i as f64 * 100.0, i as f32 * 14.4)).collect(),
            spins_required: 1.0,
        };
        let zcr = |buf: &[f32], a: usize, b: usize| {
            let mut c = 0;
            for i in a..b {
                if (buf[i * 2] < 0.0) != (buf[(i + 1) * 2] < 0.0) {
                    c += 1;
                }
            }
            c as f64 / ((b - a) as f64 / sr as f64)
        };

        // 2s of stereo buffer; `vi(ms)` is the VALUE index of a frame.
        let vi = |ms: usize| ms * sr / 1000 * 2;
        let mut buf = vec![0.0f32; vi(2000)];
        synth_spin_loop(&mut buf, &clip, &lp, false, 0.0, 1.0);
        // Natural rate inside the run; silence before it and after the
        // fade tail (900ms + 240ms).
        assert!((395.0..405.0).contains(&zcr(&buf, sr / 2, sr * 8 / 10)), "natural rate (2 crossings/period)");
        assert!(buf[..vi(50)].iter().all(|v| *v == 0.0), "silent before the run");
        assert!(buf[vi(1200)..].iter().all(|v| *v == 0.0), "silent after the fade tail");
        // Full gain well inside the run (fade-in done by 400ms).
        let peak = buf[vi(600)..vi(800)].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.95, "envelope reaches unity, peak={peak}");

        // Modulated: over [300ms, 500ms] the progress spans 0.12..0.2 ->
        // ratio 0.562..0.635 -> ~225-254 crossings/s.
        let mut buf = vec![0.0f32; vi(2000)];
        synth_spin_loop(&mut buf, &clip, &lp, true, 0.0, 1.0);
        let z = zcr(&buf, sr * 3 / 10, sr / 2);
        assert!((220.0..260.0).contains(&z), "modulated pitch ~0.6x, got {z}");
        // Later window runs faster: [700ms, 900ms] -> ratio ~0.71..0.78.
        let z2 = zcr(&buf, sr * 7 / 10, sr * 9 / 10);
        assert!(z2 > z + 30.0, "pitch rises with progress: {z} -> {z2}");
    }
}
