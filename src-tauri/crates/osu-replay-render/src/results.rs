//! Results screen (lazer `osu.Game/Screens/Ranking`), drawn as the STATIC
//! end state of every entrance animation: the selected score's
//! `ScorePanel` in `PanelState.Expanded` with the accuracy gauge full,
//! all rank badges shown, the rank letter up and every counter at its
//! final value (lazer `FinishTransforms` state).
//!
//! Layout ports (all in the root container's 1024x768 virtual units,
//! scaled by `Mapper::virt`):
//!
//! - `ResultsScreen`: a grid of [content | bottom bar]; the bar is
//!   `TwoLayerButton.SIZE_EXTENDED.Y` (50) tall over `#333`.
//! - `ScorePanelList` centres the (single) panel in the content row
//!   (height 768 - 50 = 718).
//! - `ScorePanel`: 360x586 with a +20 vertical fudge; the top layer
//!   (`ExpandedPanelTopContent`: avatar + username) slides up
//!   `EXPANDED_TOP_LAYER_HEIGHT / 2` and the middle layer down the same,
//!   leaving a 53px strip visible above the panel body.
//! - `ExpandedPanelMiddleContent`: title/artist, the 230px
//!   `AccuracyCircle`, the rolling total score, star rating +
//!   difficulty icon + mods, difficulty name / creator, then the
//!   statistics grid (`AccuracyStatistic`, `ComboStatistic`,
//!   `PerformanceStatistic` + `HitResultStatistic` rows).
//!
//! Rank math: `ScoreProcessor.RankFromScore` cutoffs, osu!'s S/X miss
//! downgrade and `ModHidden.AdjustRank` (silver SH/XH).

use crate::draw::{ttf_measure, draw_ttf_text, Atlas, Blend, Colour, DrawList, TtfFont};
use crate::game::GameData;
use crate::scene::{Assets, Mapper};
use osu_replay_judge::mods::Mods;
use osu_replay_judge::score::HitResult;

// -- ScorePanel ---------------------------------------------------------------

const EXPANDED_WIDTH: f32 = 360.0;
const EXPANDED_HEIGHT: f32 = 586.0;
pub const EXPANDED_TOP_LAYER_HEIGHT: f32 = 53.0;
const CORNER_RADIUS: f32 = 20.0;
/// `ScorePanel`'s `vertical_fudge`: the audio content sits 20 below the
/// tracking container's centre.
const VERTICAL_FUDGE: f32 = 20.0;
/// `TwoLayerButton.SIZE_EXTENDED.Y`: the results screen's bottom bar.
const BOTTOM_BAR_HEIGHT: f32 = 50.0;
const MIDDLE_PADDING: f32 = 10.0;
/// `StatisticsPanel.SIDE_PADDING`.
const SIDE_PADDING: f32 = 20.0;

// -- AccuracyCircle -----------------------------------------------------------

/// `ScoreProcessor` accuracy cutoffs (osu! standard).
const ACC_X: f64 = 1.0;
const ACC_S: f64 = 0.95;
const ACC_A: f64 = 0.9;
const ACC_B: f64 = 0.8;
const ACC_C: f64 = 0.7;
/// The D badge sits at 60% (`AccuracyCircle` hardcodes its own cutoff;
/// `ScoreProcessor.accuracy_cutoff_d` is 0 for the rank check itself).
const ACC_D_BADGE: f64 = 0.6;
/// SS is displayed as a 1% region, otherwise it would be invisible.
const VIRTUAL_SS_PERCENTAGE: f64 = 0.01;
/// Width of the spacing between grade circles, in accuracy fraction.
const GRADE_SPACING_PERCENTAGE: f64 = 2.0 / 360.0;
/// `AccuracyCircle`'s box inside the middle content.
const CIRCLE_SIZE: f32 = 230.0;
/// The gauge visually fills slightly past the target; the final alignment
/// nudge (`visual_alignment_offset`).
const VISUAL_ALIGNMENT_OFFSET: f64 = 0.001;

/// `ScoreRank` (display order: D < C < B < A < S < SH < X < XH).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Rank {
    D,
    C,
    B,
    A,
    S,
    SH,
    X,
    XH,
}

impl Rank {
    /// `OsuColour.ForRank`.
    pub fn colour(self) -> Colour {
        match self {
            Rank::X | Rank::XH => Colour::from_hex(0xDE31AE),
            Rank::S | Rank::SH => Colour::from_hex(0x02B5C3),
            Rank::A => Colour::from_hex(0x88DA20),
            Rank::B => Colour::from_hex(0xE3B130),
            Rank::C => Colour::from_hex(0xFF8E5D),
            Rank::D => Colour::from_hex(0xFF5A5A),
        }
    }

    /// `DrawableRank.GetRankLetter`.
    pub fn letter(self) -> &'static str {
        match self {
            Rank::X | Rank::XH => "SS",
            Rank::S | Rank::SH => "S",
            Rank::A => "A",
            Rank::B => "B",
            Rank::C => "C",
            Rank::D => "D",
        }
    }

    /// `DrawableRank.GetRankLetterColour` (the letter ON the badge).
    fn letter_colour(self) -> (Colour, Colour) {
        match self {
            Rank::XH | Rank::SH => (Colour::from_hex(0xFFFFFF), Colour::from_hex(0xAFDFF0)),
            Rank::X | Rank::S => (Colour::from_hex(0xFFE7A8), Colour::from_hex(0xFFB800)),
            Rank::A => (Colour::from_hex(0x275227), Colour::from_hex(0x275227)),
            Rank::B => (Colour::from_hex(0x553A2B), Colour::from_hex(0x553A2B)),
            Rank::C => (Colour::from_hex(0x473625), Colour::from_hex(0x473625)),
            Rank::D => (Colour::from_hex(0x512525), Colour::from_hex(0x512525)),
        }
    }

    fn badge_letter_colour(self) -> Colour {
        let (top, bottom) = self.letter_colour();
        Colour::lerp(top, bottom, 0.5)
    }

    /// `ScoreProcessor.RankFromScore` + `OsuScoreProcessor` miss downgrade
    /// + `ModHidden.AdjustRank`.
    pub fn from_score(accuracy: f64, misses: i64, hidden: bool) -> Rank {
        let mut rank = if accuracy == ACC_X {
            Rank::X
        } else if accuracy >= ACC_S {
            Rank::S
        } else if accuracy >= ACC_A {
            Rank::A
        } else if accuracy >= ACC_B {
            Rank::B
        } else if accuracy >= ACC_C {
            Rank::C
        } else {
            Rank::D
        };
        // osu!: an S/SS with any miss is an A.
        if matches!(rank, Rank::S | Rank::X) && misses > 0 {
            rank = Rank::A;
        }
        if hidden {
            rank = match rank {
                Rank::X => Rank::XH,
                Rank::S => Rank::SH,
                other => other,
            };
        }
        rank
    }
}

/// `OsuColour.ForHitResult` (the statistic header colours).
fn colour_for_hit_result(result: HitResult) -> Colour {
    match result {
        HitResult::Miss | HitResult::LargeTickMiss => Colour::from_hex(0xED1121),
        HitResult::Meh => Colour::from_hex(0xFFCC22),
        HitResult::Ok => Colour::from_hex(0x88B300),
        HitResult::Good => Colour::from_hex(0xB3D944),
        HitResult::SmallTickHit | HitResult::LargeTickHit | HitResult::SliderTailHit | HitResult::Great => {
            Colour::from_hex(0x66CCFF)
        }
        _ => Colour::from_hex(0x99EEFF),
    }
}

/// Star-rating spectrum (`OsuColour.STAR_DIFFICULTY_SPECTRUM`,
/// `ColourUtils.SampleFromLinearGradient`).
fn star_colour(stars: f64) -> Colour {
    const STOPS: [(f64, u32); 13] = [
        (0.1, 0xAAAAAA),
        (0.1, 0x4290FB),
        (1.25, 0x4FC0FF),
        (2.0, 0x4FFFD5),
        (2.5, 0x7CFF4F),
        (3.3, 0xF6F05C),
        (4.2, 0xFF8068),
        (4.9, 0xFF4E6F),
        (5.8, 0xC645B8),
        (6.7, 0x6563DE),
        (7.7, 0x18158E),
        (9.0, 0x000000),
        (10.0, 0x000000),
    ];
    sample_gradient(&STOPS, stars)
}

/// Star-rating pill text colour (`OsuColour.ForStarDifficultyText`): black 75%
/// below 6.5, `Orange1` until the 9.0 gradient cutoff, then the text spectrum.
fn star_text_colour(stars: f64) -> Colour {
    const TEXT_STOPS: [(f64, u32); 5] = [
        (9.0, 0xF6F05C),
        (9.9, 0xFF8068),
        (10.6, 0xFF4E6F),
        (11.5, 0xC645B8),
        (12.4, 0x6563DE),
    ];
    if stars < 6.5 {
        return Colour::from_hex(0x000000).opacity(0.75);
    }
    if stars < 9.0 {
        return Colour::from_hex(0xFFD966);
    }
    sample_gradient(&TEXT_STOPS, stars)
}

/// `ColourUtils.SampleFromLinearGradient`: piecewise-linear sample over `stops`.
fn sample_gradient(stops: &[(f64, u32)], stars: f64) -> Colour {
    let stars = (stars * 100.0).round() / 100.0;
    if stars <= stops[0].0 {
        return Colour::from_hex(stops[0].1);
    }
    for w in stops.windows(2) {
        let (p0, c0) = w[0];
        let (p1, c1) = w[1];
        if stars <= p1 {
            let t = if p1 - p0 <= 0.0 { 0.0 } else { ((stars - p0) / (p1 - p0)).clamp(0.0, 1.0) };
            return Colour::lerp(Colour::from_hex(c0), Colour::from_hex(c1), t as f32);
        }
    }
    Colour::from_hex(stops[stops.len() - 1].1)
}

// -- ModDisplay ----------------------------------------------------------------

/// One mod chip: acronym + `OsuColour.ForModType` colour + the built-in
/// icon's index into `MOD_ICON_NAMES`.
fn mod_list(mods: &Mods) -> Vec<(&'static str, u32, u16)> {
    let mut v: Vec<(&'static str, u32, u16)> = Vec::new();
    // DifficultyReduction (Lime1).
    let lime = 0xB2FF66;
    // DifficultyIncrease (Red1).
    let red = 0xFF6666;
    // Automation (Blue1).
    let blue = 0x66CCFF;
    // Conversion (Purple1).
    let purple = 0x8C66FF;
    // System (Yellow).
    let yellow = 0xFFCC22;
    if mods.easy {
        v.push(("EZ", lime, 0));
    }
    if mods.no_fail {
        v.push(("NF", lime, 1));
    }
    if mods.half_time {
        v.push(("HT", lime, 8));
    }
    if mods.hidden {
        v.push(("HD", red, 2));
    }
    if mods.hard_rock {
        v.push(("HR", red, 3));
    }
    if mods.sudden_death {
        v.push(("SD", red, 4));
    }
    if mods.perfect {
        v.push(("PF", red, 5));
    }
    if mods.nightcore {
        v.push(("NC", red, 7));
    } else if mods.double_time {
        v.push(("DT", red, 6));
    }
    if mods.flashlight {
        v.push(("FL", red, 9));
    }
    if mods.spun_out {
        v.push(("SO", blue, 10));
    }
    if mods.classic {
        v.push(("CL", purple, 11));
    }
    if mods.score_v2 {
        v.push(("SV2", yellow, 12));
    }
    if mods.touch_device {
        v.push(("TD", yellow, 13));
    }
    v
}

// -- Formatting -----------------------------------------------------------------

/// `"N0"`: thousands-separated integer.
fn fmt_thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let neg = n < 0;
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        format!("-{}", out)
    } else {
        out
    }
}

/// `FormatUtils.FormatAccuracy`: floored to 4 decimals, then "0.00%" — a
/// 99.9999% play must never display as 100.00%.
fn fmt_accuracy(a: f64) -> String {
    let floored = (a * 10_000.0).floor() / 10_000.0;
    format!("{:.2}%", floored * 100.0)
}

/// `FormatUtils.FormatStarRating`: floored to 2 decimals.
fn fmt_stars(s: f64) -> String {
    let floored = (s * 100.0).floor() / 100.0;
    format!("{:.2}", floored)
}

// -- Text helpers ----------------------------------------------------------------

/// Ink height of a string (px at the given size).
fn text_h(font: &TtfFont, text: &str, size: f32, spacing: f32) -> f32 {
    let (_, top, bottom) = ttf_measure(font, text, size, spacing);
    (bottom - top).max(0.0)
}

/// Sprite line-box height: ink plus a little leading (framework SpriteText
/// reserves the full glyph line box).
fn line_h(font: &TtfFont, text: &str, size: f32) -> f32 {
    text_h(font, text, size, 0.0) + size * 0.2
}

/// Hard-truncates to `max_w` px (TruncatingSpriteText; no ellipsis).
fn trunc(font: &TtfFont, text: &str, size: f32, spacing: f32, max_w: f32) -> String {
    let (w, _, _) = ttf_measure(font, text, size, spacing);
    if w <= max_w {
        return text.to_string();
    }
    let mut s = text.to_string();
    while !s.is_empty() {
        s.pop();
        let (w, _, _) = ttf_measure(font, &s, size, spacing);
        if w <= max_w {
            break;
        }
    }
    s
}

/// Draws a string whose ink colour blends vertically from `top` to
/// `bottom` (per-glyph, by splitting each glyph quad at its midline —
/// lazer's `ColourInfo.GradientVertical` on a sprite text).
#[allow(clippy::too_many_arguments)]
fn draw_gradient_text(
    list: &mut DrawList,
    atlas: &Atlas,
    font: &TtfFont,
    text: &str,
    center: [f32; 2],
    size_px: f32,
    top: Colour,
    bottom: Colour,
    spacing: f32,
) {
    let em = TtfFont::class_for(size_px);
    let scale = size_px / em as f32;
    let mut width = 0.0f32;
    for c in text.chars() {
        if let Some(g) = font.glyphs.get(&(c, em)) {
            width += g.advance * scale + spacing;
        }
    }
    if !text.is_empty() {
        width -= spacing;
    }
    let mut top_ink = f32::MAX;
    let mut bot_ink = f32::MIN;
    for c in text.chars() {
        if let Some(g) = font.glyphs.get(&(c, em)) {
            top_ink = top_ink.min(g.yoff);
            bot_ink = bot_ink.max(g.yoff + g.h);
        }
    }
    if top_ink == f32::MAX {
        return;
    }
    let baseline = center[1] - (top_ink + bot_ink) * 0.5 * scale;
    let mid = (top_ink + bot_ink) * 0.5;
    let mut pen_x = center[0] - width * 0.5;
    let ah = atlas.height as f32;
    for c in text.chars() {
        let g = match font.glyphs.get(&(c, em)) {
            Some(g) => g,
            None => continue,
        };
        let x = pen_x + g.xoff * scale;
        let y0 = baseline + g.yoff * scale;
        let h_full = g.h * scale;
        // Split v at the glyph's midpoint relative to the string's ink band
        // so all glyphs share one horizontal gradient line.
        let split = ((mid - g.yoff) / g.h).clamp(0.12, 0.88);
        let w = g.w * scale;
        list.set_blend(Blend::Alpha);
        list.image_sub(
            atlas,
            crate::draw::Region::Glyph { weight: 0, c, em },
            [x + w * 0.5, y0 + h_full * split * 0.5],
            [w, h_full * split],
            0.0,
            top,
            Blend::Alpha,
            0.0,
            0.0,
            1.0,
            split,
        );
        list.image_sub(
            atlas,
            crate::draw::Region::Glyph { weight: 0, c, em },
            [x + w * 0.5, y0 + h_full * (split + 1.0) * 0.5],
            [w, h_full * (1.0 - split)],
            0.0,
            bottom,
            Blend::Alpha,
            0.0,
            split,
            1.0,
            1.0,
        );
        pen_x += g.advance * scale + spacing;
    }
}

// -- Statistics model -------------------------------------------------------------

fn stat_count(stats: &[(HitResult, i32)], r: HitResult) -> i32 {
    stats.iter().find(|(s, _)| *s == r).map(|&(_, c)| c).unwrap_or(0)
}

/// One `HitResultStatistic` for the bottom grids.
struct HitStat {
    result: HitResult,
    name: &'static str,
    count: i32,
    /// `GetStatisticsForDisplay`: null for the classic accuracy judgements,
    /// the maximum count for the tick/bonus types.
    max_count: Option<i32>,
}

impl HitStat {
    /// `ScoreInfo.GetStatisticsForDisplay` for osu!standard, in
    /// `EnumExtensions.GetValuesInOrder` order: the accuracy judgements
    /// first, then the completion counts (only when the map has any).
    fn rows(stats: &[(HitResult, i32)], max_stats: &[(HitResult, i32)]) -> (Vec<HitStat>, Vec<HitStat>) {
        let mut row1 = Vec::new();
        for (r, name) in [
            (HitResult::Great, "GREAT"),
            (HitResult::Ok, "OK"),
            (HitResult::Meh, "MEH"),
            (HitResult::Miss, "MISS"),
        ] {
            row1.push(HitStat { result: r, name, count: stat_count(stats, r), max_count: None });
        }
        let mut row2 = Vec::new();
        for (r, name) in [
            (HitResult::LargeTickHit, "L TICK"),
            (HitResult::SmallTickHit, "S TICK"),
            (HitResult::SliderTailHit, "SLIDER TAIL"),
            (HitResult::LargeBonus, "L BONUS"),
            (HitResult::SmallBonus, "S BONUS"),
        ] {
            let max = stat_count(max_stats, r);
            if max > 0 {
                row2.push(HitStat { result: r, name, count: stat_count(stats, r), max_count: Some(max) });
            }
        }
        (row1, row2)
    }
}

// -- The screen -------------------------------------------------------------------

/// Everything the painter needs beyond `GameData`: the score display mode
/// and whether the atlas carries a (blurred) beatmap background / a
/// custom avatar image.
pub struct ResultsView<'a> {
    pub game: &'a GameData,
    /// Show the classic (stable) score total instead of standardised.
    pub classic_score: bool,
    /// The atlas carries the (pre-blurred) beatmap background.
    pub has_bg: bool,
    /// The atlas carries the custom avatar (`--avatar` / config).
    pub has_avatar: bool,
}

/// `PlayedOnText` format (`d MMMM yyyy HH:mm`, 24-hour) from the .osr's
/// .NET ticks (100 ns since 0001-01-01, stored as a UTC instant),
/// rendered in the system's local time like lazer's `ToLocalTime()`.
fn format_played_on(ticks: u64) -> String {
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    // .NET epoch (0001-01-01) is 62135596800 s before 1970-01-01.
    let secs = (ticks / 10_000_000) as i64 - 62_135_596_800;
    let wall = secs + local_utc_offset(secs) as i64;
    let days = wall.div_euclid(86_400);
    let sod = wall.rem_euclid(86_400);
    let (hh, mm) = (sod / 3600, (sod % 3600) / 60);
    // Civil-from-days (Howard Hinnant): days since 1970-01-01 -> y/m/d.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("Played on {} {} {} {:02}:{:02}", d, MONTHS[(m - 1) as usize], y, hh, mm)
}

/// The system zone's UTC offset (seconds) in effect at `unix_secs`,
/// read from the TZif database at `/etc/localtime` (v1 32-bit block for
/// old data, v2+ 64-bit block when present). UTC (0) when unavailable.
fn local_utc_offset(unix_secs: i64) -> i32 {
    let Ok(data) = std::fs::read("/etc/localtime") else { return 0 };
    if data.len() < 44 || &data[0..4] != b"TZif" {
        return 0;
    }
    let version = data[4];
    // `tzif_block` expects `counts_pos` at the count array, i.e. header
    // start + 20 (magic 4 + version 1 + reserved 15).
    let try_block = |counts_pos: usize, time_size: usize| -> Option<(Vec<i64>, Vec<i32>, i32)> {
        let (t, o, f, _) = tzif_block(&data, counts_pos, time_size)?;
        Some((t, o, f))
    };
    // Preferred: the v2+ 64-bit block. "Slim" files truncate the v1 data,
    // so its counts can't locate the second header — scan for the next
    // TZif magic instead of trusting them.
    let mut best = try_block(20, 4);
    if version >= b'2' {
        let mut from = 4;
        while let Some(p) = find_sub(&data, from, b"TZif") {
            if let Some(b) = try_block(p + 20, 8) {
                best = Some(b);
                break;
            }
            from = p + 1;
        }
    }
    let Some((times, offsets, fallback)) = best else { return 0 };
    let idx = times.partition_point(|&t| t <= unix_secs);
    if idx == 0 { fallback } else { offsets[idx - 1] }
}

/// Finds the next `needle` occurrence in `haystack` at or after `from`.
fn find_sub(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// Parses one TZif data block at `pos`: returns (transition times,
/// per-transition offsets, default offset = first non-DST type).
fn tzif_block(data: &[u8], pos: usize, time_size: usize) -> Option<(Vec<i64>, Vec<i32>, i32, usize)> {
    let u32be = |o: usize| -> Option<u32> { Some(u32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?)) };
    let mut counts = [0usize; 6];
    for (i, c) in counts.iter_mut().enumerate() {
        *c = u32be(pos + i * 4)? as usize;
    }
    let [isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt] = counts;
    let mut pos = pos + 24;
    let mut times = Vec::with_capacity(timecnt.min(1 << 20));
    for i in 0..timecnt {
        let o = pos + i * time_size;
        let b = data.get(o..o + time_size)?;
        times.push(if time_size == 8 {
            i64::from_be_bytes(b.try_into().ok()?)
        } else {
            i32::from_be_bytes(b.try_into().ok()?) as i64
        });
    }
    pos += timecnt * time_size;
    let mut indices = Vec::with_capacity(timecnt.min(1 << 20));
    for i in 0..timecnt {
        indices.push(*data.get(pos + i)?);
    }
    pos += timecnt;
    let mut types = Vec::with_capacity(typecnt.min(1 << 16));
    let mut fallback = 0i32;
    for i in 0..typecnt {
        let o = pos + i * 6;
        let utoff = i32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?);
        let isdst = *data.get(o + 4)?;
        if i == 0 || (isdst == 0 && fallback == 0) {
            fallback = utoff;
        }
        types.push(utoff);
    }
    pos += typecnt * 6 + charcnt + leapcnt * (4 + time_size) + isstdcnt + isutcnt;
    let offsets = indices.into_iter().map(|ti| types[ti as usize]).collect();
    Some((times, offsets, fallback, pos))
}

/// `AccuracyCircle`'s final gauge target with all the alignment nudges
/// (`LoadComplete`: notch avoidance, virtual-SS clamp, visual offset).
fn accuracy_gauge_target(accuracy: f64, rank: Rank, failed_s: bool) -> f64 {
    if failed_s {
        return ACC_S - GRADE_SPACING_PERCENTAGE / 2.0 - VISUAL_ALIGNMENT_OFFSET;
    }
    let mut target = accuracy;
    for p in [ACC_S, ACC_A, ACC_B, ACC_C] {
        if (p - target).abs() < GRADE_SPACING_PERCENTAGE / 2.0 {
            let dir = if target >= p { 1.0 } else { -1.0 };
            target = p + dir * GRADE_SPACING_PERCENTAGE / 2.0;
            break;
        }
    }
    if matches!(rank, Rank::X | Rank::XH) {
        target = 1.0;
    } else {
        target = target.min(ACC_X - VIRTUAL_SS_PERCENTAGE - GRADE_SPACING_PERCENTAGE / 2.0);
    }
    if target < 1.0 && target >= VISUAL_ALIGNMENT_OFFSET {
        target -= VISUAL_ALIGNMENT_OFFSET;
    }
    target
}

/// Angle (degrees, draw.rs convention: 0 = +x, y down, positive clockwise)
/// of an accuracy on the gauge: top at 100%, sweeping counter-clockwise
/// (`RankBadge.circlePosition`: `-PI/2 - (1 - p) * 2PI`).
fn angle_for_accuracy(a: f64) -> f32 {
    -90.0 - (1.0 - a) as f32 * 360.0
}

/// Draws one vertical-gradient arc segment set (the accuracy gauge's
/// `ColourInfo.GradientVertical(#7CF6FF, #BAFFA9)`).
fn draw_gradient_arc(
    list: &mut DrawList,
    centre: [f32; 2],
    band_radius: f32,
    thickness: f32,
    box_r: f32,
    sweep: f64,
) {
    let (c0, c1) = (Colour::from_hex(0x7CF6FF), Colour::from_hex(0xBAFFA9));
    let segs = 96;
    let start = -450.0f32;
    let r_in = band_radius - thickness * 0.5;
    let r_out = band_radius + thickness * 0.5;
    let pt = |rad: f32, a_deg: f32| -> [f32; 2] {
        let (sn, cs) = a_deg.to_radians().sin_cos();
        [centre[0] + rad * cs, centre[1] + rad * sn]
    };
    // framework `GradientVertical`: the colour varies with the vertex's Y
    // across the drawable box (top #7CF6FF -> bottom #BAFFA9). Per-vertex
    // colours keep the band continuous - no segment seams.
    let col_at = |y: f32| -> Colour {
        let t = ((y - (centre[1] - box_r)) / (2.0 * box_r)).clamp(0.0, 1.0);
        Colour::lerp(c0, c1, t)
    };
    for i in 0..segs {
        let f0 = i as f32 / segs as f32;
        let f1 = (i + 1) as f32 / segs as f32;
        let a0 = start + f0 * sweep as f32 * 360.0;
        let a1 = start + f1 * sweep as f32 * 360.0;
        let (pi0, po0) = (pt(r_in, a0), pt(r_out, a0));
        let (pi1, po1) = (pt(r_in, a1), pt(r_out, a1));
        list.quad_gradient(
            &[pi0, po0, po1, pi1],
            [col_at(pi0[1]), col_at(po0[1]), col_at(po1[1]), col_at(pi1[1])],
            Blend::Alpha,
        );
    }
}

/// The full results screen. `mapper` supplies the virtual-unit scale.
pub fn draw(view: &ResultsView, assets: &Assets, m: &Mapper, list: &mut DrawList) {
    let game = view.game;
    let s = m.virt; // virtual unit -> screen px
    let cw = m.screen_w / s; // canvas width in virtual units (1365.33 at 16:9)
    let px = |v: [f32; 2]| -> [f32; 2] { [v[0] * s, v[1] * s] };
    // Score values.
    let misses = stat_count(&game.final_statistics, HitResult::Miss) as i64;
    let rank = Rank::from_score(game.final_accuracy, misses, game.mods.hidden);
    let failed_s = game.final_accuracy >= ACC_S && rank == Rank::A;
    let score = if view.classic_score { game.final_classic_score } else { game.final_score };

    // ------------------------------------------------------------------
    // Background: the beatmap image, pre-blurred in the atlas
    // (`ResultsScreen` entering: `BlurAmount = BACKGROUND_BLUR = 10`,
    // the framework blur sigma) and faded to 50% grey
    // (`ApplyToBackground(b => b.FadeColour(OsuColour.Gray(0.5)))`),
    // cover-fitted to the screen. Without a beatmap background: the
    // clear colour.
    // ------------------------------------------------------------------
    if view.has_bg {
        let rect = assets.atlas.region_rect(crate::draw::Region::BackgroundBlurred);
        let (rw, rh) = (rect.x1 - rect.x0, rect.y1 - rect.y0);
        let k = (m.screen_w / rw).max(m.screen_h / rh);
        list.image(
            assets.atlas,
            crate::draw::Region::BackgroundBlurred,
            [m.screen_w * 0.5, m.screen_h * 0.5],
            [rw * k, rh * k],
            0.0,
            Colour::rgb(0.5, 0.5, 0.5),
            Blend::Alpha,
        );
    }

    // ------------------------------------------------------------------
    // Bottom bar (`ResultsScreen`'s bottom grid row: #333 box).
    // ------------------------------------------------------------------
    let bar_top = 768.0 - BOTTOM_BAR_HEIGHT;
    list.quad_gradient(
        &[px([0.0, bar_top]), px([cw, bar_top]), px([cw, 768.0]), px([0.0, 768.0])],
        [
            Colour::from_hex(0x333333),
            Colour::from_hex(0x333333),
            Colour::from_hex(0x333333),
            Colour::from_hex(0x333333),
        ],
        Blend::Alpha,
    );

    // ------------------------------------------------------------------
    // ScorePanel, detached to the LEFT edge (`StatisticsPanel` visible:
    // `MoveToX(StatisticsPanel.SIDE_PADDING)`), vertically centred in the
    // content row + the vertical fudge.
    // ------------------------------------------------------------------
    let c = [SIDE_PADDING + EXPANDED_WIDTH * 0.5, (768.0 - BOTTOM_BAR_HEIGHT) * 0.5 + VERTICAL_FUDGE];

    // Top layer strip (#444 -> #333): 360 x 53 sitting above the body.
    // Lazer `ScorePanel` expanded: the top layer (120 tall) moves UP by
    // 53/2 and the middle layer DOWN by 53/2, so the visible 53-unit
    // strip's bottom edge sits FLUSH on the body's top edge — no gap.
    let strip_c = [c[0], c[1] - EXPANDED_HEIGHT * 0.5];
    list.rounded_rect(
        px(strip_c),
        [EXPANDED_WIDTH * s, EXPANDED_TOP_LAYER_HEIGHT * s],
        CORNER_RADIUS * s,
        Colour::from_hex(0x444444),
        Colour::from_hex(0x333333),
        Blend::Alpha,
    );

    // Middle layer (#555 -> #333) + the user-cover white wash.
    let body_c = [c[0], c[1] + EXPANDED_TOP_LAYER_HEIGHT * 0.5];
    let body_size = [EXPANDED_WIDTH * s, EXPANDED_HEIGHT * s];
    list.rounded_rect(
        px(body_c),
        body_size,
        CORNER_RADIUS * s,
        Colour::from_hex(0x555555),
        Colour::from_hex(0x333333),
        Blend::Alpha,
    );
    list.rounded_rect(
        px(body_c),
        body_size,
        CORNER_RADIUS * s,
        Colour::from_hex(0xFFFFFF).opacity(0.28),
        Colour::from_hex(0x444444).opacity(0.0),
        Blend::Alpha,
    );

    // ------------------------------------------------------------------
    // Top layer content: avatar + username. `ExpandedPanelTopContent`
    // anchors its CENTRE to the strip's top edge, so the avatar straddles
    // the panel's top.
    // ------------------------------------------------------------------
    let user = if game.player.is_empty() { "osu!" } else { game.player.as_str() };
    let user_h = line_h(assets.semibold, user, 16.0);
    // Avatar contained from the strip's TOP EDGE downward (lazer lets it
    // straddle above; here it must not reach the very top).
    let strip_top = strip_c[1] - EXPANDED_TOP_LAYER_HEIGHT * 0.5;
    // Lazer `ExpandedPanelTopContent`: the 80-unit avatar + username flow
    // is centred on the top layer's TopCentre (26.5 above the panel top),
    // so the avatar straddles the strip and the username ends just above
    // the body. Header positions track the strip's new, flush placement.
    let header_up = 50.5;
    let avatar_c = [c[0], strip_top + 40.0 - header_up];
    list.rounded_rect(
        px(avatar_c),
        [80.0 * s, 80.0 * s],
        CORNER_RADIUS * s,
        Colour::from_hex(0x6B6B7A),
        Colour::from_hex(0x3A3A44),
        Blend::Alpha,
    );
    // Custom avatar (`--avatar` / config `avatar`): the atlas carries it
    // cover-cropped square with pre-masked rounded corners; drawn over
    // the placeholder box (its corners show through the mask).
    // Placeholder otherwise: the player's initial (no network fetch).
    if view.has_avatar {
        list.image(
            assets.atlas,
            crate::draw::Region::Avatar,
            px(avatar_c),
            [80.0 * s, 80.0 * s],
            0.0,
            Colour::WHITE,
            Blend::Alpha,
        );
    } else {
        let initial = user.chars().next().map(|ch| ch.to_uppercase().next().unwrap_or(ch)).unwrap_or('?');
        draw_ttf_text(
            list,
            assets.atlas,
            assets.bold,
            true,
            &initial.to_string(),
            px(avatar_c),
            34.0 * s,
            Colour::WHITE.opacity(0.85),
            0.0,
            Blend::Alpha,
        );
    }
    let username_c = [c[0], strip_top + 80.0 - header_up + user_h * 0.5 + 4.0];
    draw_ttf_text(
        list,
        assets.atlas,
        assets.semibold,
        false,
        user,
        px(username_c),
        16.0 * s,
        Colour::WHITE,
        0.0,
        Blend::Alpha,
    );

    // "Played on ..." (lazer `PlayedOnText`: size 10 SemiBold anchored to
    // the bottom centre of the expanded panel; format `d MMMM yyyy HH:mm`).
    if let Some(ticks) = game.played_at_ticks {
        let text = format_played_on(ticks);
        let by = body_c[1] + EXPANDED_HEIGHT * 0.5 - 14.0;
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            &text,
            px([c[0], by]),
            10.0 * s,
            Colour::WHITE,
            0.0,
            Blend::Alpha,
        );
    }

    // ------------------------------------------------------------------
    // Middle content.
    // ------------------------------------------------------------------
    let inner_w = EXPANDED_WIDTH - MIDDLE_PADDING * 2.0;
    let mut y = body_c[1] - EXPANDED_HEIGHT * 0.5 + MIDDLE_PADDING;

    // Measure the fixed-height blocks for the fit pass.
    let title = trunc(assets.semibold, &game.map_meta.title, 20.0, 0.0, inner_w);
    let artist = trunc(assets.semibold, &game.map_meta.artist, 14.0, 0.0, inner_w);
    let title_h = line_h(assets.semibold, &title, 20.0);
    let artist_h = line_h(assets.semibold, &artist, 14.0);
    let score_text = fmt_thousands(score);
    let score_h = line_h(assets.semibold, &score_text, 60.0) + 5.0;
    let diff = trunc(assets.semibold, &game.map_meta.version, 16.0, 0.0, inner_w);
    let diff_h = line_h(assets.semibold, &diff, 16.0);
    let creator_line = if game.map_meta.creator.is_empty() {
        String::new()
    } else {
        format!("mapped by {}", game.map_meta.creator)
    };
    let creator_name = if creator_line.is_empty() { String::new() } else { game.map_meta.creator.clone() };
    let creator_h = if creator_line.is_empty() { 0.0 } else { line_h(assets.regular, &creator_line, 12.0) };
    let mods = mod_list(&game.mods);
    let mods_row_h = if mods.is_empty() { 22.0 } else { 40.0 };

    let (row1, row2) = HitStat::rows(&game.final_statistics, &game.final_maximum_statistics);
    let counter_h = |t: &str| line_h(assets.regular, t, 20.0);
    let r1_h = 12.0 + counter_h("0") + 2.0;
    let r2_h = if row2.is_empty() { 0.0 } else { 12.0 + counter_h("0") + 2.0 };
    let top_stats_h = 12.0 + counter_h("0") + 2.0;

    let fixed = title_h
        + artist_h
        + CIRCLE_SIZE
        + score_h
        + mods_row_h
        + diff_h
        + creator_h
        + top_stats_h
        + r1_h
        + r2_h
        + 2.0 * 5.0;
    // Compress the flexible gaps when the content overflows the panel
    // (lazer masks the overflow instead; fitting looks better in a video).
    let avail = EXPANDED_HEIGHT - MIDDLE_PADDING * 2.0;
    let slack = (avail - fixed).max(0.0);
    let circle_margin = slack.min(40.0).max(6.0);
    let flow_gap = (slack - circle_margin).min(20.0).max(4.0);

    // Metadata.
    draw_ttf_text(
        list,
        assets.atlas,
        assets.semibold,
        false,
        &title,
        px([c[0], y + title_h * 0.5]),
        20.0 * s,
        Colour::WHITE,
        0.0,
        Blend::Alpha,
    );
    y += title_h;
    if !artist.is_empty() {
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            &artist,
            px([c[0], y + artist_h * 0.5]),
            14.0 * s,
            Colour::WHITE,
            0.0,
            Blend::Alpha,
        );
        y += artist_h;
    }
    y += circle_margin;

    // ------------------------------------------------------------------
    // AccuracyCircle (230x230), final animation state.
    // ------------------------------------------------------------------
    let cc = [c[0], y + CIRCLE_SIZE * 0.5];
    let cs = CIRCLE_SIZE * s;
    let r_out = cs * 0.5;

    // Background circle: gray(47), alpha 0.5, InnerRadius 0.21.
    let bg_thickness = 0.21 * r_out;
    list.ring(
        px(cc),
        r_out,
        bg_thickness,
        Colour::from_hex(0x2F2F2F).opacity(0.5),
        Colour::from_hex(0x2F2F2F).opacity(0.5),
        Blend::Alpha,
    );

    // Graded circles: a 0.8-sized box inset by 2.5 each side.
    let g_quad = CIRCLE_SIZE * 0.8 - 5.0;
    let g_r = g_quad * 0.5 * s;
    let g_thickness = 0.05 * g_r;
    let g_band = g_r - g_thickness * 0.5;
    let inset = GRADE_SPACING_PERCENTAGE * 0.5;
    for (start, end, col) in [
        (0.0, ACC_C, Rank::D.colour()),
        (ACC_C, ACC_B, Rank::C.colour()),
        (ACC_B, ACC_A, Rank::B.colour()),
        (ACC_A, ACC_S, Rank::A.colour()),
        (ACC_S, ACC_X - VIRTUAL_SS_PERCENTAGE, Rank::S.colour()),
        (ACC_X - VIRTUAL_SS_PERCENTAGE, 1.0, Rank::X.colour()),
    ] {
        // Clockwise from the segment's start to its end accuracy
        // (matching the gauge sweep and the badge positions).
        let a0 = angle_for_accuracy(start + inset);
        let a1 = angle_for_accuracy(end - inset);
        if a1 - a0 <= 0.05 {
            continue;
        }
        list.arc(px(cc), g_band, g_thickness, a0, a1, col, Blend::Alpha);
    }

    // Accuracy gauge: full sweep to the adjusted target.
    let target = accuracy_gauge_target(game.final_accuracy, rank, failed_s);
    let acc_thickness = 0.2 * r_out;
    draw_gradient_arc(list, px(cc), r_out - acc_thickness * 0.5, acc_thickness, r_out, target);

    // Rank badges around the gauge. The badge container is the circle box
    // padded -20 horizontally / -15 vertically (elliptical placement), and
    // each badge sits at `lerpa(cutoff, nextCutoff, 0.25|0.5)` (X exactly
    // at 100%).
    let bx = (CIRCLE_SIZE * 0.5 + 20.0) * s;
    let by = (CIRCLE_SIZE * 0.5 + 15.0) * s;
    for (pos, b_rank) in [
        (ACC_D_BADGE + (ACC_C - ACC_D_BADGE) * 0.5, Rank::D),
        (ACC_C + (ACC_B - ACC_C) * 0.5, Rank::C),
        (ACC_B + (ACC_A - ACC_B) * 0.5, Rank::B),
        (ACC_A + (ACC_S - ACC_A) * 0.25, Rank::A),
        (ACC_S + (ACC_X - VIRTUAL_SS_PERCENTAGE - ACC_S) * 0.25, Rank::S),
        (ACC_X, Rank::X),
    ] {
        // Badges above the achieved rank stay hidden; the failed-S state
        // pulls the S badge away.
        let badge_rank = if game.mods.hidden {
            match b_rank {
                Rank::X => Rank::XH,
                Rank::S => Rank::SH,
                other => other,
            }
        } else {
            b_rank
        };
        if badge_rank > rank {
            continue;
        }
        if failed_s && b_rank == Rank::S {
            continue;
        }
        let ang = angle_for_accuracy(pos).to_radians();
        let bc = [cc[0] * s + ang.cos() * bx, cc[1] * s + ang.sin() * by];
        // Soft additive glow (the badge's EdgeEffect).
        list.glow(bc, 16.0 * s, badge_rank.colour().opacity(0.18));
        list.rounded_rect(
            bc,
            [28.0 * s, 14.0 * s],
            7.0 * s,
            badge_rank.colour(),
            badge_rank.colour(),
            Blend::Alpha,
        );
        draw_ttf_text(
            list,
            assets.atlas,
            assets.bold,
            true,
            b_rank.letter(),
            bc,
            10.0 * s,
            b_rank.badge_letter_colour(),
            -0.5,
            Blend::Alpha,
        );
    }

    // Rank letter (lazer `RankText`: OsuFont.Numeric = Venera, size 76,
    // spacing -15, white with the rank-coloured glow).
    list.glow(px(cc), 95.0 * s, rank.colour().opacity(0.30));
    let rank_font = assets.venera;
    draw_ttf_text(
        list,
        assets.atlas,
        rank_font,
        true,
        rank.letter(),
        px(cc),
        76.0 * s,
        Colour::WHITE,
        -15.0,
        Blend::Alpha,
    );

    y += CIRCLE_SIZE;

    // Total score (lazer `TotalScoreCounter`: Torus 60 LIGHT fixedWidth).
    let score_font = assets.light;
    draw_ttf_text(
        list,
        assets.atlas,
        score_font,
        false,
        &score_text,
        px([c[0], y + (score_h - 5.0) * 0.5]),
        60.0 * s,
        Colour::WHITE,
        -5.0,
        Blend::Alpha,
    );
    y += score_h;

    // ------------------------------------------------------------------
    // Star rating pill + difficulty icon + mods (spacing 5).
    // ------------------------------------------------------------------
    let row_c = y + mods_row_h * 0.5;
    let mut rx = c[0];
    // Measure the row to centre it as a whole.
    let stars = game.stars;
    let star_text = if stars.is_nan() { "-".to_string() } else { fmt_stars(stars) };
    let pill_w = 12.0 + 8.0 + 3.0 + ttf_measure(assets.bold, &star_text, 14.4, -1.4).0.max(25.0) + 12.0;
    let mod_w = |n: usize| if n == 0 { 0.0 } else { n as f32 * 40.0 + (n - 1) as f32 * 5.0 };
    let row_w = pill_w + 5.0 + if mods.is_empty() { 0.0 } else { 5.0 + mod_w(mods.len()) };
    rx -= row_w * 0.5;
    // Star rating pill.
    let pill_c = [rx + pill_w * 0.5, row_c];
    list.rounded_rect(
        px(pill_c),
        [pill_w * s, 22.0 * s],
        11.0 * s,
        star_colour(stars),
        star_colour(stars),
        Blend::Alpha,
    );
    draw_star(list, px([rx + 12.0 + 4.0, row_c]), 4.2 * s, star_text_colour(stars));
    draw_ttf_text(
        list,
        assets.atlas,
        assets.bold,
        true,
        &star_text,
        px([rx + 12.0 + 8.0 + 3.0 + ttf_measure(assets.bold, &star_text, 14.4, -1.4).0.max(25.0) * 0.5, row_c + 0.8]),
        14.4 * s,
        star_text_colour(stars),
        -1.4,
        Blend::Alpha,
    );
    rx += pill_w + 5.0;
    // Mod chips (`ModIcon` at ModDisplay scale 0.5 -> 40px wide): the
    // white hexagonal background tinted with the mod type colour, and
    // the icon glyph in the near-black lerp lazer derives from it
    // (`Interpolation.ValueAt(0.1, Black, backgroundColour)`).
    if !mods.is_empty() {
        rx += 5.0;
        for (_, col, icon) in &mods {
            let mc = px([rx + 20.0, row_c]);
            list.image(
                assets.atlas,
                crate::draw::Region::ModIconBg,
                mc,
                [40.0 * s, 40.0 * (100.0 / 135.0) * s],
                0.0,
                Colour::from_hex(*col),
                Blend::Alpha,
            );
            list.image(
                assets.atlas,
                crate::draw::Region::ModIcon(*icon),
                mc,
                [40.0 * (131.0 / 135.0) * s, 40.0 * (92.0 / 135.0) * s],
                0.0,
                Colour::lerp(Colour::from_hex(0x000000), Colour::from_hex(*col), 0.1),
                Blend::Alpha,
            );
            rx += 45.0;
        }
    }
    y += mods_row_h;

    // Difficulty name + "mapped by".
    draw_ttf_text(
        list,
        assets.atlas,
        assets.semibold,
        false,
        &diff,
        px([c[0], y + diff_h * 0.5]),
        16.0 * s,
        Colour::WHITE,
        0.0,
        Blend::Alpha,
    );
    y += diff_h;
    if !creator_line.is_empty() {
        // "mapped by " regular + the creator's name semibold
        // (lazer's OsuTextFlowContainer weight override).
        let w_prefix = ttf_measure(assets.regular, "mapped by ", 12.0, 0.0).0;
        let w_name = ttf_measure(assets.semibold, &creator_name, 12.0, 0.0).0;
        let x0 = c[0] - (w_prefix + w_name) * 0.5;
        draw_ttf_text(
            list,
            assets.atlas,
            assets.regular,
            false,
            "mapped by ",
            px([x0 + w_prefix * 0.5, y + creator_h * 0.5]),
            12.0 * s,
            Colour::from_hex(0xDDDDDD),
            0.0,
            Blend::Alpha,
        );
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            &creator_name,
            px([x0 + w_prefix + w_name * 0.5, y + creator_h * 0.5]),
            12.0 * s,
            Colour::from_hex(0xDDDDDD),
            0.0,
            Blend::Alpha,
        );
        y += creator_h;
    }
    y += flow_gap;

    // ------------------------------------------------------------------
    // Statistics grids.
    // ------------------------------------------------------------------
    let col_gap = 0.0; // GridContainer: equal columns, no spacing.
    let draw_stat = |list: &mut DrawList, x0: f32, w: f32, ytop: f32, header: &str, hcol: Colour, value: &str, suffix: Option<&str>, perfect: bool| {
        let cx = x0 + w * 0.5;
        // Header pill (#222, height 12, fully rounded).
        list.rounded_rect(
            px([cx, ytop + 6.0]),
            [w * s, 12.0 * s],
            6.0 * s,
            Colour::from_hex(0x222222),
            Colour::from_hex(0x222222),
            Blend::Alpha,
        );
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            header,
            px([cx, ytop + 6.0]),
            12.0 * s,
            hcol,
            0.0,
            Blend::Alpha,
        );
        // Value counter (+ optional /max suffix and PERFECT tag): the whole
        // run is centred under the header.
        let vh = line_h(assets.regular, value, 20.0);
        let vy = ytop + 12.0 + 2.0 + vh * 0.5;
        let vw = ttf_measure(assets.regular, value, 20.0, -2.0).0;
        let perfect_w = if perfect {
            10.0 + ttf_measure(assets.semibold, "PERFECT", 11.0, 0.0).0
        } else {
            0.0
        };
        let mut vx = cx - (vw + suffix_w(assets, suffix) + perfect_w) * 0.5;
        draw_ttf_text(
            list,
            assets.atlas,
            assets.regular,
            false,
            value,
            px([vx + vw * 0.5, vy]),
            20.0 * s,
            Colour::WHITE,
            -2.0,
            Blend::Alpha,
        );
        vx += vw;
        if let Some(sfx) = suffix {
            let sw = ttf_measure(assets.regular, sfx, 12.0, -2.0).0;
            vx += 2.0;
            draw_ttf_text(
                list,
                assets.atlas,
                assets.regular,
                false,
                sfx,
                px([vx + sw * 0.5, vy + vh * 0.5 - 6.0]),
                12.0 * s,
                Colour::WHITE,
                -2.0,
                Blend::Alpha,
            );
            vx += sw;
        }
        if perfect {
            vx += 10.0;
            draw_gradient_text(
                list,
                assets.atlas,
                assets.semibold,
                "PERFECT",
                px([vx + 21.0, vy - 1.0]),
                11.0 * s,
                Colour::from_hex(0x66FFCC),
                Colour::from_hex(0xFF9AD7),
                0.0,
            );
        }
    };

    // Top row: accuracy / combo / pp.
    let top_w = (inner_w - col_gap * 2.0) / 3.0;
    let acc_str = fmt_accuracy(game.final_accuracy);
    let combo = game.final_max_combo;
    let combo_max = game.max_combo_achievable;
    let combo_val = combo.to_string();
    let combo_sfx = format!("/{}", combo_max);
    let pp_val = if game.pp.is_nan() { "0".to_string() } else { format!("{}", game.pp.round()) };
    let th = 12.0 + 2.0 + line_h(assets.regular, "0", 20.0);
    draw_stat(
        list,
        c[0] - inner_w * 0.5,
        top_w,
        y,
        "ACCURACY",
        Colour::WHITE,
        &acc_str,
        None,
        false,
    );
    draw_stat(
        list,
        c[0] - inner_w * 0.5 + top_w + col_gap,
        top_w,
        y,
        "COMBO",
        Colour::WHITE,
        &combo_val,
        Some(&combo_sfx),
        combo == combo_max,
    );
    draw_stat(
        list,
        c[0] - inner_w * 0.5 + (top_w + col_gap) * 2.0,
        top_w,
        y,
        "PP",
        Colour::WHITE,
        &pp_val,
        None,
        false,
    );
    y += th + 5.0;

    // Hit-result rows.
    let draw_row = |list: &mut DrawList, row: &[HitStat], y: f32| {
        if row.is_empty() {
            return;
        }
        let w = inner_w / row.len() as f32;
        for (i, st) in row.iter().enumerate() {
            let sfx = st.max_count.map(|m| format!("/{}", m));
            draw_stat(
                list,
                c[0] - inner_w * 0.5 + w * i as f32,
                w,
                y,
                st.name,
                colour_for_hit_result(st.result),
                &st.count.to_string(),
                sfx.as_deref(),
                false,
            );
        }
    };
    draw_row(list, &row1, y);
    y += th + 5.0;
    draw_row(list, &row2, y);

    // ------------------------------------------------------------------
    // StatisticsPanel (expanded state): the extended statistics to the
    // right of the detached panel.
    // ------------------------------------------------------------------
    draw_statistics(game, assets, m, list, cw);
}

fn suffix_w(assets: &Assets, suffix: Option<&str>) -> f32 {
    match suffix {
        Some(sfx) => ttf_measure(assets.regular, sfx, 12.0, -2.0).0,
        None => 0.0,
    }
}

/// A filled five-point star (FontAwesome `star`, 8px at unit scale).
fn draw_star(list: &mut DrawList, centre: [f32; 2], r: f32, colour: Colour) {
    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(10);
    for i in 0..10 {
        let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        let rad = if i % 2 == 0 { r } else { r * 0.45 };
        pts.push([centre[0] + ang.cos() * rad, centre[1] + ang.sin() * rad]);
    }
    list.polygon(&pts, colour, Blend::Alpha);
}

// ---------------------------------------------------------------------------
// StatisticsPanel (expanded): the extended statistics to the right of the
// detached score panel (`Screens/Ranking/Statistics`).
// ---------------------------------------------------------------------------

/// `StatisticItem.FONT_SIZE`.
const STAT_FONT: f32 = 13.0;
/// `HitEventTimingDistributionGraph`: bins per side.
const TIMING_BINS: usize = 50;
/// `AccuracyHeatmap` constants.
const HEATMAP_POINTS: i32 = 33;
const HEATMAP_INNER: f32 = 0.8;
const HEATMAP_ROTATION: f32 = 45.0;

/// One `StatisticItemContainer`: rounded gradient box + top/bottom
/// highlight strips + header, returning the content area (virtual units).
fn stat_item_box(
    list: &mut DrawList,
    m: &Mapper,
    x: f32,
    y: f32,
    w: f32,
    content_h: f32,
    header: &str,
    assets: &Assets,
) -> ([f32; 2], [f32; 2]) {
    let s = m.virt;
    // Outer padding 5, inner padding 2, content padding 15 (top 40).
    let box_x = x + 5.0;
    let box_y = y + 5.0;
    let box_w = w - 10.0;
    let box_h = 4.0 + 40.0 + content_h + 15.0;
    let (bx, by) = (box_x * s, box_y * s);
    let (bw, bh) = (box_w * s, box_h * s);
    let r = 20.0 * s;
    // Gradient body.
    list.rounded_rect(
        [bx + bw * 0.5, by + bh * 0.5],
        [bw, bh],
        r,
        Colour::from_hex(0x404040).opacity(0.8),
        Colour::from_hex(0x2E2E2E).opacity(0.95),
        Blend::Alpha,
    );
    // Top highlight (30px, fading down) and bottom shadow (20px, fading
    // up), clipped to the rounded corners.
    list.rounded_rect(
        [bx + bw * 0.5, by + 15.0 * s],
        [bw, 30.0 * s],
        r,
        Colour::from_hex(0x404040),
        Colour::from_hex(0x404040).opacity(0.0),
        Blend::Alpha,
    );
    list.rounded_rect(
        [bx + bw * 0.5, by + bh - 10.0 * s],
        [bw, 20.0 * s],
        r,
        Colour::from_hex(0x2E2E2E).opacity(0.0),
        Colour::from_hex(0x2E2E2E),
        Blend::Alpha,
    );
    // Header (Torus Bold 16, left-aligned at margin 12/8).
    let hw = ttf_measure(assets.bold, header, 16.0 * s, 0.0).0;
    draw_ttf_text(
        list,
        assets.atlas,
        assets.bold,
        true,
        header,
        [bx + 12.0 * s + hw * 0.5, by + (8.0 + 11.0) * s],
        16.0 * s,
        Colour::WHITE,
        0.0,
        Blend::Alpha,
    );
    // Content area (virtual units).
    let cx = box_x + 2.0 + 15.0;
    let cy = box_y + 2.0 + 40.0;
    ([cx, cy], [box_w - 4.0 - 30.0, content_h])
}

fn draw_statistics(game: &GameData, assets: &Assets, m: &Mapper, list: &mut DrawList, cw: f32) {
    let s = m.virt;
    // The panel's padded content area: Left = EXPANDED_WIDTH + 2*20,
    // Right = 20 (virtual units).
    let left = EXPANDED_WIDTH + SIDE_PADDING * 2.0;
    let content_w = cw - left - SIDE_PADDING;
    if content_w < 200.0 {
        return;
    }

    // -- Item heights (content) ------------------------------------------
    let ev = &game.results_hit_events;
    // lazer's timed hit events: hits only (misses exist on the heatmap as
    // `MissPoint` x-marks but carry no timing-graph / UR sample).
    let timed: Vec<crate::game::ResultsHitEvent> = ev
        .iter()
        .filter(|e| matches!(e.result, HitResult::Meh | HitResult::Ok | HitResult::Good | HitResult::Great | HitResult::Perfect))
        .copied()
        .collect();
    // Performance Breakdown rows.
    let breakdown = game.pp_breakdown.unwrap_or_default();
    let attr_rows: [(&str, f64, f64); 5] = [
        ("Aim", breakdown.0.aim, breakdown.1.aim),
        ("Speed", breakdown.0.speed, breakdown.1.speed),
        ("Accuracy", breakdown.0.accuracy, breakdown.1.accuracy),
        ("Flashlight Bonus", breakdown.0.flashlight, breakdown.1.flashlight),
        ("Reading", breakdown.0.reading, breakdown.1.reading),
    ];
    let shown: Vec<&(&str, f64, f64)> = attr_rows.iter().filter(|r| r.2.abs() > 1e-3).collect();
    let row_h = STAT_FONT * 1.25;
    let pb_content_h = (shown.len() as f32 * (row_h + 4.0)).max(2.0 * row_h + 6.0).max(96.0);

    // Fill the LEFT panel's vertical extent: top aligned with the panel's
    // top strip, bottom with the panel body (lazer centres a shorter
    // flow; stretched to match the panel reads better in a video).
    let panel_top = (768.0 - BOTTOM_BAR_HEIGHT) * 0.5 + VERTICAL_FUDGE
        - EXPANDED_HEIGHT * 0.5
        - EXPANDED_TOP_LAYER_HEIGHT * 0.5;
    let panel_h = EXPANDED_HEIGHT + EXPANDED_TOP_LAYER_HEIGHT;
    // Per-card chrome: outer padding 10 + inner padding 4 + header zone 45.
    let chrome = 59.0;
    // Visible breathing room between the cards (the flow slots touch;
    // the boxes' own 5px insets alone read as cramped).
    let gap = 8.0;
    let y0 = panel_top;
    // Bottom row: the difficulty-over-time graph (rosu-pp strains) with
    // miss markers. Only when the strain data is available.
    let has_strains = game.strain_points.len() > 1;
    let strain_content_h = if has_strains { 110.0 } else { 0.0 };
    // The graphs row takes whatever the breakdown and strain rows leave.
    let row2_content = ((panel_h - pb_content_h - strain_content_h) - 2.0 * chrome - if has_strains { chrome + gap } else { 0.0 } - gap)
        .max(200.0);
    // Timing: graph (incl. its axis row) + gap + the two-line table.
    let graph_h = row2_content - 15.0 - 2.0 * row_h;
    // Heatmap: square side bounded by the content width and the height
    // left under the (15 + 2*FONT) caption strip.
    let heatmap_side_max = row2_content - 15.0 - STAT_FONT * 2.0;

    // ------------------------------------------------------------------
    // Row 1: Performance Breakdown (full width).
    // ------------------------------------------------------------------
    let (area, size) = stat_item_box(list, m, left, y0, content_w, pb_content_h, "Performance Breakdown", assets);
    {
        let chart_w = size[0] - 50.0 - 230.0;
        let right_x = area[0] + chart_w + 50.0;
        // The 3px #222 spacer pill centred in the 50px gap.
        list.rounded_rect(
            [(area[0] + chart_w + 25.0) * s, (area[1] + pb_content_h * 0.5) * s],
            [3.0 * s, pb_content_h * s],
            1.5 * s,
            Colour::from_hex(0x222222),
            Colour::from_hex(0x222222),
            Blend::Alpha,
        );
        // Rows: name | bar | percentage.
        let name_w = 110.0;
        let pct_w = 44.0;
        for (i, (name, achieved, max)) in shown.iter().enumerate() {
            let ry = area[1] + row_h * 0.5 + i as f32 * (row_h + 4.0);
            draw_ttf_text(
                list,
                assets.atlas,
                assets.semibold,
                false,
                name,
                [(area[0] + name_w * 0.5) * s, ry * s],
                STAT_FONT * s,
                Colour::WHITE,
                0.0,
                Blend::Alpha,
            );
            let bar_x0 = area[0] + name_w + 10.0;
            let bar_w = chart_w - name_w - 10.0 - pct_w - 10.0;
            let (bx, bw) = (bar_x0 * s, bar_w * s);
            list.rounded_rect(
                [bx + bw * 0.5, ry * s],
                [bw, 5.0 * s],
                2.5 * s,
                Colour::WHITE.opacity(0.5),
                Colour::WHITE.opacity(0.5),
                Blend::Alpha,
            );
            let len = (*achieved / max).clamp(0.0, 1.0) as f32;
            list.rounded_rect(
                [bx + bw * len * 0.5, ry * s],
                [bw * len, 5.0 * s],
                2.5 * s,
                Colour::from_hex(0x66FFCC),
                Colour::from_hex(0x66FFCC),
                Blend::Alpha,
            );
            let pct = format!("{}%", ((*achieved / max) * 100.0).round().clamp(0.0, 999.0));
            draw_ttf_text(
                list,
                assets.atlas,
                assets.semibold,
                false,
                &pct,
                [(area[0] + chart_w - pct_w * 0.5) * s, ry * s],
                STAT_FONT * s,
                Colour::WHITE,
                0.0,
                Blend::Alpha,
            );
        }
        // Right column: Achieved PP / Maximum.
        let round_away = |v: f64| (v + 0.5).floor() as i64;
        let rows = [
            ("Achieved PP", round_away(breakdown.0.total), Colour::from_hex(0x66FFCC)),
            ("Maximum", round_away(breakdown.1.total), Colour::from_hex(0xB3B3B3)),
        ];
        for (i, (label, value, col)) in rows.iter().enumerate() {
            let ry = area[1] + row_h * 0.5 + i as f32 * (row_h + 6.0);
            draw_ttf_text(
                list,
                assets.atlas,
                assets.semibold,
                false,
                label,
                [(right_x + 62.0) * s, ry * s],
                STAT_FONT * s,
                *col,
                0.0,
                Blend::Alpha,
            );
            draw_ttf_text(
                list,
                assets.atlas,
                assets.bold,
                true,
                &value.to_string(),
                [(right_x + 230.0 - 40.0) * s, ry * s],
                STAT_FONT * s,
                *col,
                0.0,
                Blend::Alpha,
            );
        }
    }

    // ------------------------------------------------------------------
    // Row 2: Timing Distribution (left half) + Accuracy Heatmap (right).
    // ------------------------------------------------------------------
    let half_w = (content_w - gap) * 0.5;
    let y2 = y0 + pb_content_h + chrome + gap;
    let (t_area, t_size) = stat_item_box(list, m, left, y2, half_w, row2_content, "Timing Distribution", assets);
    let (h_area, h_size) = stat_item_box(list, m, left + half_w + gap, y2, half_w, row2_content, "Accuracy Heatmap", assets);

    // -- Timing distribution graph (`HitEventTimingDistributionGraph`).
    {
        let graph_x = t_area[0] + 5.0;
        let graph_w = t_size[0] - 10.0;
        let bars_h = graph_h - STAT_FONT;
        let bottom = t_area[1] + bars_h;
        // Bin size from the worst offset.
        let max_off = timed.iter().map(|e| e.offset.abs()).fold(0.0f64, f64::max);
        let bin_size = ((max_off / TIMING_BINS as f64).ceil() as i64).max(1) as f64;
        let total_bins = TIMING_BINS * 2 + 1;
        // (result, count) per bin, stacked bottom-up in display order.
        let order = [
            HitResult::Perfect,
            HitResult::Great,
            HitResult::Good,
            HitResult::Ok,
            HitResult::Meh,
        ];
        let mut bins: Vec<[i32; 5]> = vec![[0; 5]; total_bins];
        let mut max_count: i32 = 0;
        for e in &timed {
            let idx = (TIMING_BINS as f64 + (e.offset / bin_size).round()) as i64;
            if idx < 0 || idx >= total_bins as i64 {
                continue;
            }
            let slot = order.iter().position(|r| *r == e.result).unwrap_or(1);
            let b = &mut bins[idx as usize];
            b[slot] += 1;
            max_count = max_count.max(b.iter().sum());
        }
        let bin_w = graph_w / total_bins as f32;
        for (i, b) in bins.iter().enumerate() {
            let bx = graph_x + bin_w * (i as f32 + 0.5);
            let total: i32 = b.iter().sum();
            if total == 0 {
                // Grey dot at the 2% minimum height.
                list.disc(
                    [bx * s, (bottom - bars_h * 0.01) * s],
                    (bin_w * 0.28).max(1.2) * s,
                    Colour::from_hex(0x808080).opacity(0.5),
                    Colour::from_hex(0x808080).opacity(0.5),
                    Blend::Alpha,
                );
                continue;
            }
            let mut acc = 0.0f32;
            for (slot, &count) in b.iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let frac = count as f32 / max_count as f32;
                let h = frac * bars_h;
                let y = bottom - acc - h * 0.5;
                let colour = if i == TIMING_BINS && slot == 0 {
                    Colour::WHITE
                } else {
                    colour_for_hit_result(order[slot])
                };
                list.rounded_rect(
                    [bx * s, y * s],
                    [(bin_w * 0.72).max(1.5) * s, h.max(1.5) * s],
                    (bin_w * 0.36).max(0.75) * s,
                    colour,
                    colour,
                    Blend::Alpha,
                );
                acc += h;
            }
        }
        // Axis: "0" centred, ±steps fading outward. The outermost labels
        // are clamped into the graph box so their halves stay inside the
        // card (lazer lets them overhang the padded edges).
        let max_value = TIMING_BINS as f64 * bin_size;
        let axis_y = t_area[1] + graph_h - STAT_FONT * 0.5;
        let cx = graph_x + graph_w * 0.5;
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            "0",
            [cx * s, axis_y * s],
            STAT_FONT * s,
            Colour::WHITE,
            0.0,
            Blend::Alpha,
        );
        for i in 1..=5 {
            let p = i as f32 / 5.0;
            let alpha = (1.0 - p * 0.8).max(0.0);
            let value = (max_value / 5.0 * i as f64).round() as i64;
            for dir in [-1.0f32, 1.0] {
                let label = format!("{}{}", if dir < 0.0 { "-" } else { "+" }, value);
                let lw = ttf_measure(assets.semibold, &label, STAT_FONT, 0.0).0;
                let lx = (cx + dir * p * graph_w * 0.5)
                    .clamp(graph_x + lw * 0.5, graph_x + graph_w - lw * 0.5);
                draw_ttf_text(
                    list,
                    assets.atlas,
                    assets.semibold,
                    false,
                    &label,
                    [lx * s, axis_y * s],
                    STAT_FONT * s,
                    Colour::WHITE.opacity(alpha),
                    0.0,
                    Blend::Alpha,
                );
            }
        }
        // Simple statistic table: Average Hit Error / Unstable Rate.
        let table_y = t_area[1] + graph_h + 15.0;
        let (avg, ur) = hit_error_stats(&timed, game.rate);
        let avg_text = match avg {
            Some(v) => format!("{:.2} ms {}", v.abs(), if v < 0.0 { "early" } else { "late" }),
            None => "(not available)".to_string(),
        };
        let ur_text = match ur {
            Some(v) => format!("{:.2}", v),
            None => "(not available)".to_string(),
        };
        for (i, (name, value)) in [("Average Hit Error", avg_text.as_str()), ("Unstable Rate", ur_text.as_str())].iter().enumerate() {
            let ry = table_y + row_h * 0.5 + i as f32 * row_h;
            draw_ttf_text(
                list,
                assets.atlas,
                assets.semibold,
                false,
                name,
                [(t_area[0] + ttf_measure(assets.semibold, name, STAT_FONT, 0.0).0 * 0.5) * s, ry * s],
                STAT_FONT * s,
                Colour::WHITE,
                0.0,
                Blend::Alpha,
            );
            let vw = ttf_measure(assets.bold, &value, STAT_FONT, 0.0).0;
            draw_ttf_text(
                list,
                assets.atlas,
                assets.bold,
                true,
                &value,
                [(t_area[0] + t_size[0] - vw * 0.5) * s, ry * s],
                STAT_FONT * s,
                Colour::WHITE,
                0.0,
                Blend::Alpha,
            );
        }
    }

    // -- Accuracy heatmap (`AccuracyHeatmap`).
    {
        // Square side: fit the content box (width-bound when narrow),
        // centred both ways (lazer: Anchor Centre + FillMode Fit).
        let inner_w = h_size[0];
        let side = heatmap_side_max.min(inner_w);
        let hc = [h_area[0] + inner_w * 0.5, h_area[1] + row2_content * 0.5];
        let (hx, hy) = (hc[0] * s, hc[1] * s);
        let hs = side * s;
        // Inner circle: dark fill + white border (0.8 portion).
        let inner_r = HEATMAP_INNER * hs * 0.5;
        list.disc([hx, hy], inner_r, Colour::from_hex(0x202624), Colour::from_hex(0x202624), Blend::Alpha);
        list.ring([hx, hy], inner_r, 2.0 * s, Colour::WHITE, Colour::WHITE, Blend::Alpha);
        // The movement axis (up-right/down-left diagonal, the direction
        // `FindRelativeHitPosition` normalises onto) at full alpha; the
        // crossing diagonal dimmed. Screen y is down, so up-right is
        // (cos45, -sin45). Stroke = lazer's `line_thickness` 2.
        let axis = [0.70710678, -0.70710678];
        let half_len = (HEATMAP_INNER + 0.2) * hs * 0.5;
        for (dir, alpha) in [(axis, 1.0), ([-axis[1], axis[0]], 0.6)] {
            let p0 = [hx - dir[0] * half_len, hy - dir[1] * half_len];
            let p1 = [hx + dir[0] * half_len, hy + dir[1] * half_len];
            list.capsule(p0, p1, 2.0 * s, Colour::WHITE.opacity(alpha), Blend::Alpha);
        }
        // End ticks at the overshoot (up-right) tip: Height-10 Width-2
        // circles rotated ±45 WITHIN the 45°-rotated container, i.e.
        // horizontal + vertical on screen (a '+' crosshair).
        let (ex, ey) = (hx + axis[0] * half_len, hy + axis[1] * half_len);
        for (dx, dy) in [(1.0f32, 0.0), (0.0, 1.0)] {
            let p0 = [ex - dx * 5.0 * s, ey - dy * 5.0 * s];
            let p1 = [ex + dx * 5.0 * s, ey + dy * 5.0 * s];
            list.capsule(p0, p1, 2.0 * s, Colour::WHITE, Blend::Alpha);
        }
        // The point grid (33x33); only non-zero cells render.
        let mut grid = vec![0i32; (HEATMAP_POINTS * HEATMAP_POINTS) as usize];
        let centre = (HEATMAP_POINTS - 1) as f32 * 0.5;
        let local_inner = centre * HEATMAP_INNER;
        let mut peak = 0i32;
        for e in ev {
            let Some(last) = e.last_pos else { continue };
            let rel = find_relative_hit_position(last, e.pos, e.cursor, e.radius as f64, HEATMAP_ROTATION);
            let px = centre + local_inner * rel[0];
            let py = centre + local_inner * rel[1];
            let c = px.round() as i32;
            let r = py.round() as i32;
            if c < 0 || r < 0 || c >= HEATMAP_POINTS || r >= HEATMAP_POINTS {
                continue;
            }
            let cell = &mut grid[(r * HEATMAP_POINTS + c) as usize];
            *cell += 1;
            peak = peak.max(*cell);
        }
        let cell = hs / HEATMAP_POINTS as f32;
        // (hx, hy) is the square's CENTRE; the grid spans it from the
        // top-left corner at (hx - hs/2, hy - hs/2).
        let (gx, gy) = (hx - hs * 0.5, hy - hs * 0.5);
        for r in 0..HEATMAP_POINTS {
            for c in 0..HEATMAP_POINTS {
                let count = grid[(r * HEATMAP_POINTS + c) as usize];
                if count == 0 {
                    continue;
                }
                // `GridPoint` hit test: distance from the grid centre
                // (16.5, 16.5) against the 0.8 inner radius (16.5 * 0.8),
                // exactly the drawn circle's radius.
                let grid_centre = HEATMAP_POINTS as f32 * 0.5;
                let dx = c as f32 + 0.5 - grid_centre;
                let dy = r as f32 + 0.5 - grid_centre;
                let dist = (dx * dx + dy * dy).sqrt();
                let is_hit = dist <= grid_centre * HEATMAP_INNER;
                let cx = gx + (c as f32 + 0.5) * cell;
                let cy = gy + (r as f32 + 0.5) * cell;
                if is_hit {
                    // HitPoint: alpha/colour by count vs peak.
                    let mut amount = 0.2 * (count as f32 / 10.0).min(1.0);
                    amount += 0.8 * count as f32 / peak.max(1) as f32;
                    amount = crate::draw::Easing::OutQuint.apply(amount.min(1.0) as f64) as f32;
                    let alpha = (amount / 0.95).min(1.0);
                    let base = Colour::from_hex(0x66FFCC);
                    let colour = if amount > 0.95 { base.lighten((amount - 0.95).min(1.0)) } else { base };
                    list.disc(
                        [cx, cy],
                        cell * 0.5,
                        colour.opacity(alpha),
                        colour.opacity(alpha),
                        Blend::Alpha,
                    );
                } else {
                    // MissPoint: an x-mark in red.
                    let col = Colour::from_hex(0xFF6666).opacity(0.8);
                    let e = cell * 0.28;
                    for rot in [45.0f32, -45.0] {
                        let (sn, cs) = rot.to_radians().sin_cos();
                        list.capsule(
                            [cx - cs * e, cy - sn * e],
                            [cx + cs * e, cy + sn * e],
                            0.8 * s,
                            col,
                            Blend::Alpha,
                        );
                    }
                }
            }
        }
        // Overshoot / Undershoot labels: lazer anchors upright text at the
        // axis tips (`Origin = Anchor.BottomLeft` / `Anchor.TopRight`,
        // `Y = ±(inner + ext)/2`, `Padding = 2`): Overshoot's bottom-left
        // corner just past the up-right tip, Undershoot's top-right just
        // past the down-left tip. Both line segments END at the tips, so
        // the text stays clear of the diagonals and the ticks.
        let (ow, o_top, o_bot) = ttf_measure(assets.semibold, "Overshoot", 12.0 * s, 0.0);
        let oh = o_bot - o_top;
        let (tx, ty) = (hx + axis[0] * half_len, hy + axis[1] * half_len);
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            "Overshoot",
            [tx + 2.0 * s + ow * 0.5, ty - 2.0 * s - oh * 0.5],
            12.0 * s,
            Colour::WHITE,
            0.0,
            Blend::Alpha,
        );
        let (uw, u_top, u_bot) = ttf_measure(assets.semibold, "Undershoot", 12.0 * s, 0.0);
        let uh = u_bot - u_top;
        let (ux, uy) = (hx - axis[0] * half_len, hy - axis[1] * half_len);
        draw_ttf_text(
            list,
            assets.atlas,
            assets.semibold,
            false,
            "Undershoot",
            [ux - 2.0 * s - uw * 0.5, uy + 2.0 * s + uh * 0.5],
            12.0 * s,
            Colour::WHITE,
            0.0,
            Blend::Alpha,
        );
    }

    // ------------------------------------------------------------------
    // Bottom row: Difficulty Graph - the rosu-pp strain curves over time
    // (aim as the filled curve, speed as the thin overlay), with a red x
    // on the curve at every miss time. The strain points are per-object
    // (time, aim, speed) from the local rosu-pp fork.
    // ------------------------------------------------------------------
    let pts = &game.strain_points;
    if pts.len() > 1 {
        let y3 = y2 + row2_content + chrome + gap;
        let (d_area, d_size) = stat_item_box(list, m, left, y3, content_w, strain_content_h, "Difficulty Graph", assets);
        // The time axis starts at the FIRST strain point: the map's intro
        // lead-in (no objects, no strain) is cut, and the curve begins at
        // the plot's left edge instead of wedging back to the origin.
        let t0 = pts[0].0;
        let last_end = game
            .objects
            .last()
            .map(|o| o.end_time)
            .filter(|t| *t > t0)
            .unwrap_or(pts.last().unwrap().0);
        // Headroom above the curve + the strip below the axis where the
        // miss x-marks live.
        let top_pad = 12.0;
        let x_strip = 15.0;
        let plot_h = strain_content_h - top_pad - x_strip;
        let gx0 = d_area[0];
        let gw = d_size[0];
        let base_y = d_area[1] + top_pad + plot_h;
        let max_aim = pts.iter().map(|p| p.1).fold(0.0f64, f64::max).max(1e-9);
        let x_at = |t: f64| -> f32 { (((t - t0) / (last_end - t0)).clamp(0.0, 1.0) * gw as f64) as f32 };
        let curve_colour = star_colour(game.stars);

        // Aim skill: JUST the curve line - no filled area and no speed
        // overlay, so nothing connects down to the origin/baseline.
        let mut aim_pts: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
        for &(t, aim, _) in pts.iter() {
            aim_pts.push([(gx0 + x_at(t)) * s, (base_y - (aim / max_aim) as f32 * plot_h) * s]);
        }
        list.stroke_band(&aim_pts, 1.3 * s, 0.0, curve_colour, curve_colour, 1.0);

        // Baseline.
        list.capsule(
            [gx0 * s, base_y * s],
            [(gx0 + gw) * s, base_y * s],
            0.7 * s,
            Colour::WHITE.opacity(0.25),
            Blend::Alpha,
        );

        // Miss markers (`miss_times`: Miss / slider-break judgement
        // times): a vertical DASHED line through the plot at the miss
        // time, with the red x sitting BELOW the axis.
        let miss_colour = Colour::from_hex(0xED1121);
        for &t in &game.miss_times {
            if t < t0 || t > last_end {
                continue;
            }
            let mx = (gx0 + x_at(t)) * s;
            // Gray dashed vertical line: plot top to the baseline.
            let (dash, gap) = (5.0 * s, 4.0 * s);
            let mut y = (base_y - plot_h) * s;
            let base_px = base_y * s;
            while y < base_px {
                let y2 = (y + dash).min(base_px);
                list.capsule(
                    [mx, y + dash * 0.5],
                    [mx, y2 - dash * 0.5],
                    0.8 * s,
                    Colour::from_hex(0x9E9E9E).opacity(0.75),
                    Blend::Alpha,
                );
                y += dash + gap;
            }
            // The x below the axis.
            let my = base_y * s + 7.5 * s;
            for rot in [45.0f32, -45.0] {
                let (sn, cs) = rot.to_radians().sin_cos();
                let e = 4.5 * s;
                list.capsule(
                    [mx - cs * e, my - sn * e],
                    [mx + cs * e, my + sn * e],
                    1.1 * s,
                    miss_colour,
                    Blend::Alpha,
                );
            }
        }
    }
}

/// `HitEventExtensions.CalculateAverageHitError` (raw clock offsets - NO
/// rate division) + `CalculateUnstableRate` (10 x stddev with each offset
/// divided by the gameplay rate: "Division by gameplay rate is to account
/// for TimeOffset scaling with gameplay rate").
fn hit_error_stats(ev: &[crate::game::ResultsHitEvent], rate: f64) -> (Option<f64>, Option<f64>) {
    if ev.is_empty() {
        return (None, None);
    }
    let n = ev.len() as f64;
    let mean = ev.iter().map(|e| e.offset).sum::<f64>() / n;
    let var = ev.iter().map(|e| (e.offset - mean).powi(2)).sum::<f64>() / n;
    (Some(mean), Some(10.0 * var.sqrt() / rate))
}

/// `AccuracyHeatmap.FindRelativeHitPosition`: the hit point normalised
/// against the previous-object -> this-object movement, rotated by
/// `rotation` degrees.
fn find_relative_hit_position(
    previous: [f32; 2],
    next: [f32; 2],
    hit: [f32; 2],
    radius: f64,
    rotation: f32,
) -> [f32; 2] {
    let angle1 = ((next[1] - hit[1]) as f64).atan2((hit[0] - next[0]) as f64);
    let angle2 = ((next[1] - previous[1]) as f64).atan2((previous[0] - next[0]) as f64);
    let final_angle = angle2 - angle1;
    let dist = (((hit[0] - next[0]).powi(2) + (hit[1] - next[1]).powi(2)).sqrt() as f64) / radius;
    let rotated = final_angle - rotation.to_radians() as f64;
    [-dist as f32 * rotated.cos() as f32, -dist as f32 * rotated.sin() as f32]
}
