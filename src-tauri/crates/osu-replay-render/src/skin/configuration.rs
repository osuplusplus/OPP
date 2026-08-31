//! Port of `osu.Game/Skinning/SkinConfiguration.cs`.
//!
//! A skin's parsed `skin.ini` state: legacy version, combo colours,
//! named custom colours and the raw `ConfigDictionary` every other
//! key/value pair lands in (`LegacySkin.GetConfig`'s `genericLookup`
//! reads that dictionary).

use std::collections::HashMap;

use crate::draw::Colour;

/// `SkinConfiguration.LATEST_VERSION`.
pub const LATEST_VERSION: f64 = 2.7;

/// `SkinConfiguration.LegacySetting` - legacy skin.ini keys that get
/// first-class lookups (enum name == ini key, as `genericLookup` uses
/// `lookup.ToString()`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LegacySetting {
    Version,
    ComboPrefix,
    ComboOverlap,
    ScorePrefix,
    ScoreOverlap,
    HitCirclePrefix,
    HitCircleOverlap,
    AnimationFramerate,
    LayeredHitSounds,
    AllowSliderBallTint,
    InputOverlayText,
}

impl LegacySetting {
    /// The ini key / dictionary key for this setting.
    pub fn key(self) -> &'static str {
        match self {
            LegacySetting::Version => "Version",
            LegacySetting::ComboPrefix => "ComboPrefix",
            LegacySetting::ComboOverlap => "ComboOverlap",
            LegacySetting::ScorePrefix => "ScorePrefix",
            LegacySetting::ScoreOverlap => "ScoreOverlap",
            LegacySetting::HitCirclePrefix => "HitCirclePrefix",
            LegacySetting::HitCircleOverlap => "HitCircleOverlap",
            LegacySetting::AnimationFramerate => "AnimationFramerate",
            LegacySetting::LayeredHitSounds => "LayeredHitSounds",
            LegacySetting::AllowSliderBallTint => "AllowSliderBallTint",
            LegacySetting::InputOverlayText => "InputOverlayText",
        }
    }
}

/// `SkinConfiguration.DefaultComboColours` (== `DefaultLegacySkin.DEFAULT_COMBO_COLOURS`).
pub fn default_combo_colours() -> [Colour; 4] {
    [
        Colour::rgba_bytes(255, 192, 0, 255),
        Colour::rgba_bytes(0, 202, 0, 255),
        Colour::rgba_bytes(18, 124, 255, 255),
        Colour::rgba_bytes(242, 24, 57, 255),
    ]
}

/// An empty skin configuration (port of the `SkinConfiguration` model).
#[derive(Clone, Debug)]
pub struct SkinConfiguration {
    /// `[General] Name` (`SkinInfo.Name`).
    pub name: String,
    /// `[General] Author` (`SkinInfo.Creator`).
    pub creator: String,
    /// Legacy version of this skin (`[General] Version`, "latest" maps to
    /// [`LATEST_VERSION`]).
    pub legacy_version: Option<f64>,
    /// `IsLatestVersion`: set when Version was "latest".
    pub is_latest_version: bool,
    /// `AllowDefaultComboColoursFallback`.
    pub allow_default_combo_colours_fallback: bool,
    /// `CustomComboColours` (`Combo1`..`Combo8` lines, in file order).
    pub custom_combo_colours: Vec<Colour>,
    /// `CustomColours` (named `[Colours]` entries, e.g. `SliderBall`).
    pub custom_colours: HashMap<String, Colour>,
    /// `ConfigDictionary`: every other `key: value` pair of the ini.
    pub config_dictionary: HashMap<String, String>,
}

impl Default for SkinConfiguration {
    fn default() -> SkinConfiguration {
        SkinConfiguration {
            name: String::new(),
            creator: String::new(),
            legacy_version: None,
            is_latest_version: false,
            allow_default_combo_colours_fallback: true,
            custom_combo_colours: Vec::new(),
            custom_colours: HashMap::new(),
            config_dictionary: HashMap::new(),
        }
    }
}

impl SkinConfiguration {
    /// `LegacySkinDecoder.CreateTemplateObject`: the parse starts at
    /// legacy version 1.0.
    pub fn template() -> SkinConfiguration {
        SkinConfiguration { legacy_version: Some(1.0), ..Default::default() }
    }

    /// `SkinConfiguration.ComboColours`: custom colours when present, the
    /// stable defaults as fallback (when allowed), else none.
    pub fn combo_colours(&self) -> Option<Vec<Colour>> {
        if !self.custom_combo_colours.is_empty() {
            return Some(self.custom_combo_colours.clone());
        }
        if self.allow_default_combo_colours_fallback {
            return Some(default_combo_colours().to_vec());
        }
        None
    }

    /// `SkinConfiguration.LegacyVersion ?? LATEST_VERSION` - what
    /// `LegacySetting::Version` lookups resolve to.
    pub fn effective_legacy_version(&self) -> f64 {
        self.legacy_version.unwrap_or(LATEST_VERSION)
    }
}
