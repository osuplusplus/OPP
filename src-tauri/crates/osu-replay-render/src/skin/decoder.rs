//! Port of the skin.ini decoders:
//!
//! - `osu.Game/Beatmaps/Formats/LegacyDecoder.cs` (section splitting,
//!   comment stripping, colour parsing, `SplitKeyVal`)
//! - `osu.Game/Skinning/LegacySkinDecoder.cs` (`skin.ini` →
//!   [`SkinConfiguration`])
//! - `osu.Game/Skinning/LegacyManiaSkinDecoder.cs` (`[Mania]` sections →
//!   [`Vec<LegacyManiaSkinConfiguration>`], the second pass
//!   `LegacySkin.ParseConfigurationStream` runs over the same stream)
//!
//! `LegacySkin` always decodes with BOTH decoders: the skin decoder
//! stores unknown keys into `ConfigDictionary`, the mania decoder pulls
//! the `[Mania]` blocks out so they don't pollute it.

use std::collections::HashMap;

use crate::draw::Colour;

use super::configuration::{SkinConfiguration, LATEST_VERSION};

/// `LegacySkin.STABLE_MAGIC_SCALE_FACTOR`: legacy positioning values are
/// based in x480 dimensions and convert to x768 by multiplying by 1.6.
pub const STABLE_MAGIC_SCALE_FACTOR: f32 = 1.6;

/// `LegacyDecoder.MAX_COMBO_COLOUR_COUNT`.
pub const MAX_COMBO_COLOUR_COUNT: usize = 8;

/// `LegacyDecoder.Section`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    General,
    Editor,
    Metadata,
    Difficulty,
    Events,
    TimingPoints,
    Colours,
    HitObjects,
    Variables,
    Fonts,
    CatchTheBeat,
    Mania,
}

impl Section {
    /// `Enum.TryParse` on the `[...]` header body; unknown sections log
    /// and leave the current section unchanged (lazer keeps parsing into
    /// the previous section).
    fn parse(name: &str) -> Option<Section> {
        Some(match name {
            "General" => Section::General,
            "Editor" => Section::Editor,
            "Metadata" => Section::Metadata,
            "Difficulty" => Section::Difficulty,
            "Events" => Section::Events,
            "TimingPoints" => Section::TimingPoints,
            "Colours" => Section::Colours,
            "HitObjects" => Section::HitObjects,
            "Variables" => Section::Variables,
            "Fonts" => Section::Fonts,
            "CatchTheBeat" => Section::CatchTheBeat,
            "Mania" => Section::Mania,
            _ => return None,
        })
    }
}

/// `LegacyDecoder.ShouldSkipLine`.
fn should_skip_line(line: &str) -> bool {
    line.trim().is_empty() || line.trim_start().starts_with("//")
}

/// `LegacyDecoder.StripComments` (index > 0: a line starting with `//` is
/// already skipped).
fn strip_comments(line: &str) -> &str {
    match line.find("//") {
        Some(idx) if idx > 0 => &line[..idx],
        _ => line,
    }
}

/// `LegacyDecoder.SplitKeyVal` (separator ':', trimmed).
fn split_key_val(line: &str) -> (&str, &str) {
    match line.split_once(':') {
        Some((k, v)) => (k.trim(), v.trim()),
        None => (line.trim(), ""),
    }
}

/// `LegacyDecoder.HandleColours`: `R,G,B[,A]` with 8-bit components.
/// `Combo1..Combo8` keys append to the custom combo colours (in file
/// order); anything else becomes a named custom colour. Mirrors lazer in
/// that a combo index out of 1..=8 silently degrades to a custom colour
/// named e.g. "Combo9".
fn handle_colours(config: &mut SkinConfiguration, line: &str, allow_alpha: bool) {
    let (key, value) = split_key_val(line);
    let split: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
    if split.len() != 3 && split.len() != 4 {
        return;
    }
    let parse_byte = |s: &str| s.parse::<u8>().ok();
    let (Some(r), Some(g), Some(b)) = (parse_byte(split[0]), parse_byte(split[1]), parse_byte(split[2]))
    else {
        return;
    };
    let a = if allow_alpha && split.len() == 4 {
        match parse_byte(split[3]) {
            Some(a) => a,
            None => return,
        }
    } else {
        255
    };
    let colour = Colour::rgba_bytes(r, g, b, a);

    // `pair.Key.StartsWith("Combo") && int.TryParse(pair.Key[5..])` in
    // 1..=MAX_COMBO_COLOUR_COUNT.
    let combo_index = key
        .strip_prefix("Combo")
        .and_then(|rest| rest.parse::<i64>().ok());
    match combo_index {
        Some(n) if (1..=MAX_COMBO_COLOUR_COUNT as i64).contains(&n) => {
            config.custom_combo_colours.push(colour);
        }
        _ => {
            config.custom_colours.insert(key.to_string(), colour);
        }
    }
}

/// `LegacySkinDecoder.Decode`: parse a full `skin.ini` (or a beatmap's
/// `.osu`, which the decoder also accepts) into a [`SkinConfiguration`].
pub fn decode_skin_configuration(content: &str) -> SkinConfiguration {
    let mut config = SkinConfiguration::template();
    let mut section = Section::General;

    for raw in content.lines() {
        let line = raw.trim_start_matches('\u{feff}');
        if should_skip_line(line) {
            continue;
        }
        // Comments are not stripped from metadata lines (song metadata may
        // contain "//" as valid data).
        let line = if section != Section::Metadata { strip_comments(line) } else { line };
        let line = line.trim_end();

        if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
            match Section::parse(&line[1..line.len() - 1]) {
                Some(s) => section = s,
                // Unknown section: lazer logs and keeps the current one.
                None => {}
            }
            continue;
        }

        parse_skin_line(&mut config, section, line);
    }

    config
}

/// `LegacySkinDecoder.ParseLine`.
fn parse_skin_line(config: &mut SkinConfiguration, section: Section, line: &str) {
    if section != Section::Colours {
        let (key, value) = split_key_val(line);

        if section == Section::General {
            match key {
                "Name" => {
                    config.name = value.to_string();
                    return;
                }
                "Author" => {
                    config.creator = value.to_string();
                    return;
                }
                "Version" => {
                    if value == "latest" {
                        config.legacy_version = Some(LATEST_VERSION);
                        config.is_latest_version = true;
                    } else if let Ok(version) = parse_decimal(value) {
                        config.legacy_version = Some(version);
                        config.is_latest_version = false;
                    }
                    return;
                }
                _ => {}
            }
        } else if section == Section::CatchTheBeat {
            // osu!catch section only has colour settings, so no harm in
            // handling the entire section as colours (alpha allowed).
            handle_colours(config, line, true);
            return;
        }

        if !key.is_empty() {
            config.config_dictionary.insert(key.to_string(), value.to_string());
        }
    }

    // `LegacyDecoder.ParseLine`: the Colours section (no alpha).
    if section == Section::Colours {
        handle_colours(config, line, false);
    }
}

/// `decimal.TryParse(..., NumberStyles.AllowDecimalPoint, Invariant)`:
/// digits with an optional single `.`; no exponent, no signs.
fn parse_decimal(s: &str) -> Result<f64, ()> {
    let mut seen_dot = false;
    let mut seen_digit = false;
    for (i, c) in s.char_indices() {
        match c {
            '0'..='9' => seen_digit = true,
            '.' if !seen_dot && i > 0 => seen_dot = true,
            _ => return Err(()),
        }
    }
    if seen_digit {
        s.parse::<f64>().map_err(|_| ())
    } else {
        Err(())
    }
}

// ---------------------------------------------------------------------------
// Mania skin configuration (`[Mania]` sections)
// ---------------------------------------------------------------------------

/// `LegacyManiaSkinConfiguration`.
#[derive(Clone, Debug)]
pub struct LegacyManiaSkinConfiguration {
    pub keys: usize,
    pub custom_colours: HashMap<String, Colour>,
    pub image_lookups: HashMap<String, String>,
    pub width_for_note_height_scale: f32,
    pub column_line_width: Vec<f32>,
    pub column_spacing: Vec<f32>,
    pub column_width: Vec<f32>,
    pub explosion_width: Vec<f32>,
    pub hold_note_light_width: Vec<f32>,
    pub hit_position: f32,
    pub light_position: f32,
    pub combo_position: f32,
    pub score_position: f32,
    pub bar_line_height: f32,
    pub show_judgement_line: bool,
    pub keys_under_notes: bool,
    pub light_frame_per_second: i32,
    pub note_body_style: Option<LegacyNoteBodyStyle>,
    /// Unimplemented upstream as well ("present primarily for
    /// encode-decode stability").
    pub special_style: Option<LegacySpecialStyle>,
    pub column_start: f32,
    pub column_right: f32,
    pub upside_down: bool,
    pub separate_score: bool,
    pub split_stages: bool,
    pub stage_separation: f32,
    pub combo_burst_style: Option<LegacyComboBurstStyle>,
    pub flip_settings: HashMap<String, String>,
}

/// `LegacyManiaSkinConfiguration.LegacyNoteBodyStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LegacyNoteBodyStyle {
    Stretch = 0,
    RepeatTop = 2,
    RepeatBottom = 3,
    RepeatTopAndBottom = 4,
}

/// `LegacySpecialStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LegacySpecialStyle {
    None = 0,
    Left = 1,
    Right = 2,
}

/// `LegacyComboBurstStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LegacyComboBurstStyle {
    Left = 0,
    Right = 1,
    Both = 2,
}

/// `LegacyManiaSkinConfiguration.DEFAULT_COLUMN_SIZE`.
pub const DEFAULT_COLUMN_SIZE: f32 = 30.0 * STABLE_MAGIC_SCALE_FACTOR;
/// `LegacyManiaSkinConfiguration.DEFAULT_HIT_POSITION`.
pub const DEFAULT_HIT_POSITION: f32 = (480.0 - 402.0) * STABLE_MAGIC_SCALE_FACTOR;

impl LegacyManiaSkinConfiguration {
    pub fn new(keys: usize) -> LegacyManiaSkinConfiguration {
        LegacyManiaSkinConfiguration {
            keys,
            custom_colours: HashMap::new(),
            image_lookups: HashMap::new(),
            width_for_note_height_scale: 0.0,
            column_line_width: vec![2.0; keys + 1],
            column_spacing: vec![0.0; keys.saturating_sub(1)],
            column_width: vec![DEFAULT_COLUMN_SIZE; keys],
            explosion_width: vec![0.0; keys],
            hold_note_light_width: vec![0.0; keys],
            hit_position: DEFAULT_HIT_POSITION,
            light_position: (480.0 - 413.0) * STABLE_MAGIC_SCALE_FACTOR,
            combo_position: 111.0 * STABLE_MAGIC_SCALE_FACTOR,
            score_position: 300.0 * STABLE_MAGIC_SCALE_FACTOR,
            bar_line_height: 1.2,
            show_judgement_line: true,
            keys_under_notes: false,
            light_frame_per_second: 60,
            note_body_style: None,
            special_style: None,
            column_start: 136.0,
            column_right: 19.0,
            upside_down: false,
            separate_score: true,
            split_stages: false,
            stage_separation: 40.0,
            combo_burst_style: None,
            flip_settings: HashMap::new(),
        }
    }

    /// `MinimumColumnWidth`.
    pub fn minimum_column_width(&self) -> f32 {
        self.column_width.iter().copied().fold(f32::MAX, f32::min)
    }
}

/// `LegacyManiaSkinDecoder.Decode`: parse every `[Mania]` block of the
/// stream. Lines seen before a `Keys:` line are held and flushed once the
/// block's key count is known (stable writes `Keys` after layout values
/// sometimes); lines left pending when a new section starts are discarded.
pub fn decode_mania_configurations(content: &str) -> Vec<LegacyManiaSkinConfiguration> {
    let mut output: Vec<LegacyManiaSkinConfiguration> = Vec::new();
    let mut section = Section::General;
    let mut pending: Vec<String> = Vec::new();
    let mut current: Option<usize> = None; // index into output

    for raw in content.lines() {
        let line = raw.trim_start_matches('\u{feff}');
        if should_skip_line(line) {
            continue;
        }
        let line = strip_comments(line).trim_end();

        if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
            match Section::parse(&line[1..line.len() - 1]) {
                Some(s) => section = s,
                None => {}
            }
            // OnBeginNewSection: pending lines without a config are
            // discarded.
            pending.clear();
            current = None;
            continue;
        }

        if section != Section::Mania {
            continue;
        }

        let (key, value) = split_key_val(line);
        if key == "Keys" {
            if let Ok(keys) = value.parse::<usize>() {
                // Silently ignore duplicate key counts.
                if !output.iter().any(|c| c.keys == keys) {
                    output.push(LegacyManiaSkinConfiguration::new(keys));
                    current = Some(output.len() - 1);
                } else {
                    current = output.iter().position(|c| c.keys == keys);
                }
                if let Some(idx) = current {
                    flush_pending(&mut output[idx], &pending);
                }
                pending.clear();
            }
            continue;
        }

        pending.push(line.to_string());
        if let Some(idx) = current {
            flush_pending(&mut output[idx], &pending);
            pending.clear();
        }
    }

    output
}

/// `LegacyManiaSkinDecoder.flushPendingLines`.
fn flush_pending(config: &mut LegacyManiaSkinConfiguration, pending: &[String]) {
    for line in pending {
        let (key, value) = split_key_val(line);
        let f = match value.parse::<f32>() {
            Ok(v) => v,
            Err(_) => f32::NAN,
        };
        match key {
            "ColumnLineWidth" => parse_array_value(value, &mut config.column_line_width, false),
            "ColumnSpacing" => parse_array_value(value, &mut config.column_spacing, true),
            "ColumnWidth" => parse_array_value(value, &mut config.column_width, true),
            "BarlineHeight" => config.bar_line_height = f,
            "HitPosition" => {
                config.hit_position = (480.0 - f.clamp(240.0, 480.0)) * STABLE_MAGIC_SCALE_FACTOR
            }
            "LightPosition" => config.light_position = (480.0 - f) * STABLE_MAGIC_SCALE_FACTOR,
            "ComboPosition" => config.combo_position = f * STABLE_MAGIC_SCALE_FACTOR,
            "ScorePosition" => config.score_position = f * STABLE_MAGIC_SCALE_FACTOR,
            "JudgementLine" => config.show_judgement_line = value == "1",
            "KeysUnderNotes" => config.keys_under_notes = value == "1",
            "LightingNWidth" => parse_array_value(value, &mut config.explosion_width, true),
            "LightingLWidth" => parse_array_value(value, &mut config.hold_note_light_width, true),
            "NoteBodyStyle" => config.note_body_style = LegacyNoteBodyStyle::parse(value),
            "WidthForNoteHeightScale" => {
                config.width_for_note_height_scale = f * STABLE_MAGIC_SCALE_FACTOR
            }
            "LightFramePerSecond" => {
                let fps = value.parse::<i32>().unwrap_or(0);
                config.light_frame_per_second = if fps > 0 { fps } else { 24 };
            }
            "SpecialStyle" => config.special_style = LegacySpecialStyle::parse(value),
            "ColumnStart" => config.column_start = f,
            "ColumnRight" => config.column_right = f,
            "UpsideDown" => config.upside_down = value == "1",
            "SeparateScore" => config.separate_score = value == "1",
            "SplitStages" => config.split_stages = value == "1",
            "StageSeparation" => config.stage_separation = f,
            "ComboBurstStyle" => config.combo_burst_style = LegacyComboBurstStyle::parse(value),
            _ if key.starts_with("Colour") => {
                handle_mania_colour(config, line, true);
            }
            // Custom sprite paths.
            _ if key.starts_with("NoteImage")
                || key.starts_with("KeyImage")
                || key.starts_with("Hit")
                || key.starts_with("Stage")
                || key.starts_with("Lighting")
                || key == "WarningArrow" =>
            {
                config.image_lookups.insert(key.to_string(), value.to_string());
            }
            _ if key.starts_with("KeyFlipWhenUpsideDown")
                || key.starts_with("NoteFlipWhenUpsideDown") =>
            {
                config.flip_settings.insert(key.to_string(), value.to_string());
            }
            _ => {}
        }
    }
}

/// `HandleColours` against a mania config (colour goes to its custom
/// colours; `Combo1..8` handling is upstream too via `IHasComboColours`,
/// but `LegacyManiaSkinConfiguration` only implements `IHasCustomColours`
/// so combo keys land as named colours named "Combo1"... matching lazer's
/// behaviour for this target type).
fn handle_mania_colour(config: &mut LegacyManiaSkinConfiguration, line: &str, allow_alpha: bool) {
    let mut skin = SkinConfiguration::template();
    handle_colours(&mut skin, line, allow_alpha);
    for (k, v) in skin.custom_colours {
        config.custom_colours.insert(k, v);
    }
    // Combo entries: lazer's HandleColours checks `output is
    // IHasComboColours` and returns early for mania configs, so
    // Combo1..8 lines are DROPPED there. Mirror that.
}

/// `LegacyManiaSkinDecoder.parseArrayValue`: comma-separated floats into a
/// fixed-length array; unparsable entries read as zero (stable behaviour),
/// extras ignored.
fn parse_array_value(value: &str, output: &mut [f32], apply_scale_factor: bool) {
    for (i, part) in value.split(',').enumerate() {
        if i >= output.len() {
            break;
        }
        let mut parsed = part.trim().parse::<f32>().unwrap_or(0.0);
        if apply_scale_factor {
            parsed *= STABLE_MAGIC_SCALE_FACTOR;
        }
        output[i] = parsed;
    }
}

impl LegacyNoteBodyStyle {
    fn parse(s: &str) -> Option<LegacyNoteBodyStyle> {
        Some(match s {
            "0" => LegacyNoteBodyStyle::Stretch,
            "2" => LegacyNoteBodyStyle::RepeatTop,
            "3" => LegacyNoteBodyStyle::RepeatBottom,
            "4" => LegacyNoteBodyStyle::RepeatTopAndBottom,
            _ => return None,
        })
    }
}

impl LegacySpecialStyle {
    fn parse(s: &str) -> Option<LegacySpecialStyle> {
        Some(match s {
            "0" => LegacySpecialStyle::None,
            "1" => LegacySpecialStyle::Left,
            "2" => LegacySpecialStyle::Right,
            _ => return None,
        })
    }
}

impl LegacyComboBurstStyle {
    fn parse(s: &str) -> Option<LegacyComboBurstStyle> {
        Some(match s {
            "0" => LegacyComboBurstStyle::Left,
            "1" => LegacyComboBurstStyle::Right,
            "2" => LegacyComboBurstStyle::Both,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::configuration::default_combo_colours;

    #[test]
    fn parses_general_and_version() {
        let config = decode_skin_configuration(
            "[General]\nName: Test Skin\nAuthor: someone\nVersion: 2.5\nCursorExpand: 0\n",
        );
        assert_eq!(config.name, "Test Skin");
        assert_eq!(config.creator, "someone");
        assert_eq!(config.legacy_version, Some(2.5));
        assert!(!config.is_latest_version);
        assert_eq!(config.config_dictionary.get("CursorExpand").map(String::as_str), Some("0"));
    }

    #[test]
    fn version_latest_maps_to_latest() {
        let config = decode_skin_configuration("[General]\nVersion: latest\n");
        assert_eq!(config.legacy_version, Some(LATEST_VERSION));
        assert!(config.is_latest_version);
    }

    #[test]
    fn missing_ini_defaults_are_applied_by_skin_not_decoder() {
        // The decoder's template starts at 1.0 (CreateTemplateObject); the
        // LATEST_VERSION default for missing files is applied by the Skin
        // constructor, not here.
        let config = decode_skin_configuration("");
        assert_eq!(config.legacy_version, Some(1.0));
    }

    #[test]
    fn parses_colours_section() {
        let config = decode_skin_configuration(
            "[Colours]\nCombo1: 255,128,64\nCombo2 : 10, 20, 30\nSliderBall: 1,2,3\nCombo9: 9,9,9\n",
        );
        assert_eq!(config.custom_combo_colours.len(), 2);
        // No alpha allowed in [Colours].
        assert_eq!(config.custom_combo_colours[0], Colour::rgba_bytes(255, 128, 64, 255));
        assert_eq!(
            config.custom_colours.get("SliderBall").copied(),
            Some(Colour::rgba_bytes(1, 2, 3, 255))
        );
        // Combo9 is out of range: stored as a NAMED colour.
        assert!(config.custom_colours.contains_key("Combo9"));
    }

    #[test]
    fn catch_thebeat_allows_alpha() {
        let config = decode_skin_configuration("[CatchTheBeat]\nHyperFruit: 255,0,0,128\n");
        assert_eq!(
            config.custom_colours.get("HyperFruit").copied(),
            Some(Colour::rgba_bytes(255, 0, 0, 128))
        );
    }

    #[test]
    fn comments_and_unknown_sections() {
        let config = decode_skin_configuration(
            "[General]\n// full line comment\nName: x // trailing\n[NotASection]\nWhatever: 1\n",
        );
        assert_eq!(config.name, "x");
        // Unknown section header keeps the previous section (General), so
        // the pair still lands in the dictionary.
        assert_eq!(config.config_dictionary.get("Whatever").map(String::as_str), Some("1"));
    }

    #[test]
    fn combo_colours_fallback() {
        let config = decode_skin_configuration("[General]\nName: x\n");
        assert_eq!(config.combo_colours().unwrap(), default_combo_colours().to_vec());
        let config = decode_skin_configuration("[Colours]\nCombo1: 1,1,1\n");
        assert_eq!(config.combo_colours().unwrap().len(), 1);
    }

    #[test]
    fn mania_sections_parse_per_keycount() {
        let mania = decode_mania_configurations(
            "[Mania]\nColumnWidth: 20,14,14,20\nKeys: 4\nHitPosition: 402\n[Mania]\nKeys: 7\nKeys: 4\n",
        );
        assert_eq!(mania.len(), 2);
        let four = &mania[0];
        assert_eq!(four.keys, 4);
        assert_eq!(four.column_width, vec![32.0, 22.4, 22.4, 32.0]); // values x1.6 (parseArrayValue)
        assert_eq!(four.hit_position, DEFAULT_HIT_POSITION);
        assert_eq!(four.column_line_width, vec![2.0; 5]);
        assert_eq!(mania[1].keys, 7);
        assert_eq!(mania[1].column_width, vec![DEFAULT_COLUMN_SIZE; 7]);
    }

    #[test]
    fn mania_pending_lines_before_keys() {
        let mania = decode_mania_configurations(
            "[Mania]\nNoteImage0: custom-note.png\nKeys: 1\n[General]\nName: late\n",
        );
        assert_eq!(mania.len(), 1);
        assert_eq!(
            mania[0].image_lookups.get("NoteImage0").map(String::as_str),
            Some("custom-note.png")
        );
    }
}
