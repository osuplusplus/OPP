//! Port of the skin lookup types:
//!
//! - `GlobalSkinColours.cs` / `SkinComboColourLookup.cs` /
//!   `SkinCustomColourLookup.cs`
//! - `SkinConfiguration.LegacySetting` (see [`LegacySetting`])
//! - `LegacyManiaSkinConfigurationLookup.cs`
//!
//! lazer's `GetConfig<TLookup, TValue>(lookup)` generic pair maps to the
//! tagged enums [`SkinLookup`] (TLookup) and [`SkinValue`] (TValue): the
//! full lookup surface without runtime typing.

use crate::draw::Colour;

use super::configuration::LegacySetting;

/// `GlobalSkinColours`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GlobalSkinColours {
    ComboColours,
    MenuGlow,
}

impl GlobalSkinColours {
    /// `colour.ToString()` - the custom-colour dictionary key.
    pub fn key(self) -> &'static str {
        match self {
            GlobalSkinColours::ComboColours => "ComboColours",
            GlobalSkinColours::MenuGlow => "MenuGlow",
        }
    }
}

/// `SkinComboColourLookup`: the preferred combo colour index plus the
/// combo requesting it (`IHasComboInformation` reduces to the object's
/// combo index for this renderer).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SkinComboColourLookup {
    pub colour_index: usize,
    pub combo_index: usize,
}

/// `SkinCustomColourLookup`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SkinCustomColourLookup(pub String);

/// `LegacyManiaSkinConfigurationLookup`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LegacyManiaSkinConfigurationLookup {
    /// `TotalColumns`.
    pub total_columns: usize,
    /// `ColumnIndex` (None for non-column lookups).
    pub column_index: Option<usize>,
    pub lookup: LegacyManiaSkinConfigurationLookups,
}

/// `LegacyManiaSkinConfigurationLookups`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LegacyManiaSkinConfigurationLookups {
    ColumnWidth,
    LightImage,
    HitPosition,
    ComboPosition,
    ScorePosition,
    LightPosition,
    StagePaddingTop,
    StagePaddingBottom,
    HitTargetImage,
    ShowJudgementLine,
    KeyImage,
    KeyImageDown,
    NoteImage,
    HoldNoteHeadImage,
    HoldNoteTailImage,
    HoldNoteBodyImage,
    HoldNoteLightImage,
    WidthForNoteHeightScale,
    ExplosionImage,
    ColumnLineColour,
    JudgementLineColour,
    ColumnBackgroundColour,
    ColumnLightColour,
    ComboBreakColour,
    MinimumColumnWidth,
    LeftStageImage,
    RightStageImage,
    BottomStageImage,
    BarLineHeight,
    BarLineColour,
    Hit300g,
    Hit300,
    Hit200,
    Hit100,
    Hit50,
    Hit0,
    KeysUnderNotes,
    NoteBodyStyle,
    LightFramePerSecond,
    LeftColumnSpacing,
    RightColumnSpacing,
    LeftLineWidth,
    RightLineWidth,
    ExplosionScale,
    HoldNoteLightScale,
}

impl LegacyManiaSkinConfigurationLookups {
    /// `maniaLookup.Lookup.ToString()` - used for the image lookups that
    /// return the mania `ImageLookups` entries verbatim.
    pub fn key(self) -> &'static str {
        use LegacyManiaSkinConfigurationLookups as L;
        match self {
            L::ColumnWidth => "ColumnWidth",
            L::LightImage => "LightImage",
            L::HitPosition => "HitPosition",
            L::ComboPosition => "ComboPosition",
            L::ScorePosition => "ScorePosition",
            L::LightPosition => "LightPosition",
            L::StagePaddingTop => "StagePaddingTop",
            L::StagePaddingBottom => "StagePaddingBottom",
            L::HitTargetImage => "HitTargetImage",
            L::ShowJudgementLine => "ShowJudgementLine",
            L::KeyImage => "KeyImage",
            L::KeyImageDown => "KeyImageDown",
            L::NoteImage => "NoteImage",
            L::HoldNoteHeadImage => "HoldNoteHeadImage",
            L::HoldNoteTailImage => "HoldNoteTailImage",
            L::HoldNoteBodyImage => "HoldNoteBodyImage",
            L::HoldNoteLightImage => "HoldNoteLightImage",
            L::WidthForNoteHeightScale => "WidthForNoteHeightScale",
            L::ExplosionImage => "ExplosionImage",
            L::ColumnLineColour => "ColumnLineColour",
            L::JudgementLineColour => "JudgementLineColour",
            L::ColumnBackgroundColour => "ColumnBackgroundColour",
            L::ColumnLightColour => "ColumnLightColour",
            L::ComboBreakColour => "ComboBreakColour",
            L::MinimumColumnWidth => "MinimumColumnWidth",
            L::LeftStageImage => "LeftStageImage",
            L::RightStageImage => "RightStageImage",
            L::BottomStageImage => "BottomStageImage",
            L::BarLineHeight => "BarLineHeight",
            L::BarLineColour => "BarLineColour",
            L::Hit300g => "Hit300g",
            L::Hit300 => "Hit300",
            L::Hit200 => "Hit200",
            L::Hit100 => "Hit100",
            L::Hit50 => "Hit50",
            L::Hit0 => "Hit0",
            L::KeysUnderNotes => "KeysUnderNotes",
            L::NoteBodyStyle => "NoteBodyStyle",
            L::LightFramePerSecond => "LightFramePerSecond",
            L::LeftColumnSpacing => "LeftColumnSpacing",
            L::RightColumnSpacing => "RightColumnSpacing",
            L::LeftLineWidth => "LeftLineWidth",
            L::RightLineWidth => "RightLineWidth",
            L::ExplosionScale => "ExplosionScale",
            L::HoldNoteLightScale => "HoldNoteLightScale",
        }
    }
}

/// The `TLookup` side of `GetConfig<TLookup, TValue>`.
#[derive(Clone, PartialEq, Debug)]
pub enum SkinLookup {
    GlobalColour(GlobalSkinColours),
    ComboColour(SkinComboColourLookup),
    CustomColour(SkinCustomColourLookup),
    LegacySetting(LegacySetting),
    Mania(LegacyManiaSkinConfigurationLookup),
    /// `genericLookup`: any other key against `ConfigDictionary`.
    Generic(String),
}

impl SkinLookup {
    /// The `ConfigDictionary` key for lookups that end in
    /// `genericLookup` (`lookup.ToString()`).
    pub fn generic_key(&self) -> Option<String> {
        match self {
            SkinLookup::LegacySetting(s) => Some(s.key().to_string()),
            SkinLookup::Generic(k) => Some(k.clone()),
            _ => None,
        }
    }
}

/// The `TValue` side of `GetConfig<TLookup, TValue>` (the boxed
/// `IBindable<TValue>` result).
#[derive(Clone, PartialEq, Debug)]
pub enum SkinValue {
    Colour(Colour),
    ComboColours(Vec<Colour>),
    F64(f64),
    F32(f32),
    Bool(bool),
    I64(i64),
    Str(String),
    /// Mania image lookups resolve to a texture name.
    ManiaImage(String),
    /// `LegacyNoteBodyStyle` resolved per the version rules.
    NoteBodyStyle(super::decoder::LegacyNoteBodyStyle),
}

impl SkinValue {
    /// `Bindable<float/double>.Parse` over `ConfigDictionary` strings.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            SkinValue::F64(v) => Some(*v),
            SkinValue::F32(v) => Some(*v as f64),
            SkinValue::I64(v) => Some(*v as f64),
            SkinValue::Str(v) => v.trim().parse::<f64>().ok(),
            _ => None,
        }
    }

    /// `Bindable<bool>.Parse` plus lazer's numeric special case for
    /// skins using 1/0 (or 2) to signify a boolean state: `true`/`false`
    /// first (case-insensitive), then any non-zero integer is true.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SkinValue::Bool(v) => Some(*v),
            SkinValue::Str(v) => {
                let s = v.trim();
                if s.eq_ignore_ascii_case("true") {
                    Some(true)
                } else if s.eq_ignore_ascii_case("false") {
                    Some(false)
                } else {
                    s.parse::<i64>().ok().map(|n| n != 0)
                }
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            SkinValue::Str(v) => Some(v),
            SkinValue::ManiaImage(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_colour(&self) -> Option<Colour> {
        match self {
            SkinValue::Colour(v) => Some(*v),
            _ => None,
        }
    }

    /// `SkinUtils.As<string>`: the value as an owned string.
    pub fn into_string(self) -> Option<String> {
        match self {
            SkinValue::Str(v) => Some(v),
            SkinValue::ManiaImage(v) => Some(v),
            _ => None,
        }
    }
}
