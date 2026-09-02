//! Port of `osu.Game/Skinning/LegacySkin.cs`: a user skin loaded from a
//! directory of stable-format files (the unpacked `.osk` content).
//!
//! Covers the full surface lazer's `LegacySkin` exposes:
//!
//! - `GetTexture`: `@2x` high-resolution sprites first (`ScaleAdjust` 2),
//!   `@2x` stripping from lookup names, the stable sprite-name aliases,
//!   and the taiko grayscale conversions of
//!   `LegacyTextureLoaderStore` (BT.601)
//! - `GetConfig`: the whole lookup switch - global/combo/custom colours,
//!   legacy settings (Version / InputOverlayText special cases), the
//!   generic `ConfigDictionary` path, and the complete mania
//!   configuration lookup (`lookupForMania`), with its lazy per-keycount
//!   config creation
//! - `GetSample`: name resolution in stable's `wav -> mp3 -> ogg` order
//!   (path resolution only - see the module docs)
//!
//! Textures are decoded once by [`SkinTextureSource::texture_images`],
//! packed into the shared atlas, and handed back via
//! [`SkinTextureSource::assign_regions`]; `get_texture` then serves
//! atlas handles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::draw::{Colour, Image};

use super::configuration::{SkinConfiguration, LATEST_VERSION};
use super::decoder::{decode_mania_configurations, decode_skin_configuration, LegacyManiaSkinConfiguration, LegacyNoteBodyStyle, DEFAULT_COLUMN_SIZE};
use super::lookup::{
    GlobalSkinColours, LegacyManiaSkinConfigurationLookup, LegacyManiaSkinConfigurationLookups as ManiaLookup,
    SkinComboColourLookup, SkinCustomColourLookup, SkinLookup, SkinValue,
};
use super::texture::SkinTexture;
use super::{Skin, SkinTextureSource};

/// Texture extensions the framework `TextureStore` probes, in order.
const TEXTURE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];
/// Sample extensions, stable's `wav -> mp3 -> ogg` ordering (the ogg tail
/// of `Skin.RecycleSamples`).
const SAMPLE_EXTENSIONS: [&str; 3] = ["wav", "mp3", "ogg"];

/// `LegacySkin.STABLE_MAGIC_SCALE_FACTOR` (re-exported for the mania
/// lookups below).
pub use super::decoder::STABLE_MAGIC_SCALE_FACTOR;

/// Sprites stable stores in colour and lazer converts to grayscale on
/// load (`LegacyTextureLoaderStore.grayscale_sprites`), matched with or
/// without an `@2x` suffix, case-insensitively.
const GRAYSCALE_SPRITES: [&str; 4] =
    ["taiko-bar-right", "taikobigcircle", "taikohitcircle", "taikohitcircleoverlay"];

/// A user skin directory. File lookups are case-insensitive (stable
/// behaviour) and walk subdirectories, keyed by each file's path relative
/// to the skin dir without extension (lazer's `RealmBackedResourceStore`
/// keys imported files by their full standardised filename, so skin.ini
/// prefixes may name subdirectories, e.g. `HitCirclePrefix:
/// Assets/default/default`).
pub struct LegacySkin {
    name: String,
    dir: PathBuf,
    configuration: SkinConfiguration,
    /// `LegacySkin.ManiaConfigurations` + the lazy creation
    /// `lookupForMania` performs on misses.
    mania_configurations: RefCell<HashMap<usize, LegacyManiaSkinConfiguration>>,
    /// lowercase filename stem (or `stem@2x`) -> path, image files only.
    files: HashMap<String, PathBuf>,
    /// lowercase stem -> path, audio files only (first extension hit in
    /// wav -> mp3 -> ogg order wins, per `SampleStore.Get`).
    samples: HashMap<String, PathBuf>,
    /// Lookup name (`stem` / `stem@2x`) -> atlas handle, filled by
    /// `assign_regions`.
    textures: HashMap<String, SkinTexture>,
}

/// The extension ranking shared by textures and samples: the first
/// extension in lookup order (`ResourceStore.GetFilenames` probes
/// `searchExtensions` in registration order) wins for a given key.
fn extension_rank(ext: &str, order: &[&str]) -> Option<usize> {
    order.iter().position(|x| x.eq_ignore_ascii_case(ext))
}

/// Walk the skin tree, mapping each file's relative path (no extension,
/// lowercased, `/`-separated - the lookup-key form of
/// `RealmBackedResourceStore`'s standardised filenames) to its absolute
/// path, split by texture / sample use.
fn visit_skin_tree(
    dir: &Path,
    rel: &str,
    files: &mut HashMap<String, PathBuf>,
    samples: &mut HashMap<String, PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if file_type.is_dir() {
            let child_rel = if rel.is_empty() { name.to_string() } else { format!("{}/{}", rel, name) };
            visit_skin_tree(&path, &child_rel, files, samples);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase) else {
            continue;
        };
        let is_texture = TEXTURE_EXTENSIONS.contains(&ext.as_str());
        let order: &[&str] = if is_texture { &TEXTURE_EXTENSIONS } else { &SAMPLE_EXTENSIONS };
        let key = if rel.is_empty() {
            name.to_lowercase()
        } else {
            format!("{}/{}", rel, name).to_lowercase()
        };
        // Strip the extension from the key ("hitcircle.png" -> "hitcircle",
        // "Assets/x/default-0.png" -> "assets/x/default-0"); `@2x` stays,
        // it is part of the stem.
        let key = match key.rfind('.') {
            Some(i) => key[..i].to_string(),
            None => key,
        };
        let better = |store: &HashMap<String, PathBuf>| match extension_rank(&ext, order) {
            Some(rank) => match store.get(&key) {
                Some(existing) => existing
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(|cur| extension_rank(cur, order))
                    .map(|r| rank < r)
                    .unwrap_or(true),
                None => true,
            },
            None => false,
        };
        if is_texture {
            if better(files) {
                files.insert(key, path);
            }
        } else if SAMPLE_EXTENSIONS.contains(&ext.as_str()) {
            if better(samples) {
                samples.insert(key, path);
            }
        }
    }
}

impl LegacySkin {
    /// Construct from a skin directory. Parses `skin.ini` when present
    /// (both decoder passes, like `LegacySkin.ParseConfigurationStream`);
    /// a missing ini leaves the `LATEST_VERSION` default of the `Skin`
    /// constructor. Fails only when `dir` is not a directory.
    pub fn from_directory(dir: &Path) -> Result<LegacySkin, String> {
        let mut files = HashMap::new();
        let mut samples = HashMap::new();
        visit_skin_tree(dir, "", &mut files, &mut samples);

        // skin.ini: the store lookup is case-insensitive.
        let ini_path = ["skin.ini", "Skin.ini", "SKIN.INI"]
            .iter()
            .map(|f| dir.join(f))
            .find(|p| p.is_file());
        let mut configuration = match &ini_path {
            Some(p) => {
                let content = std::fs::read_to_string(p)
                    .map_err(|e| format!("cannot read {}: {}", p.display(), e))?;
                decode_skin_configuration(&content)
            }
            None => {
                // `Skin` constructor, no configuration stream: latest.
                let mut c = SkinConfiguration::default();
                c.legacy_version = Some(LATEST_VERSION);
                c.is_latest_version = true;
                c
            }
        };
        if configuration.name.is_empty() {
            configuration.name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("user skin")
                .to_string();
        }

        let mut mania_configurations = HashMap::new();
        if let Some(p) = &ini_path {
            let content = std::fs::read_to_string(p).unwrap_or_default();
            for config in decode_mania_configurations(&content) {
                mania_configurations.insert(config.keys, config);
            }
        }

        Ok(LegacySkin {
            name: configuration.name.clone(),
            dir: dir.to_path_buf(),
            configuration,
            mania_configurations: RefCell::new(mania_configurations),
            files,
            samples,
            textures: HashMap::new(),
        })
    }

    /// The skin directory (for diagnostics).
    pub fn directory(&self) -> &Path {
        &self.dir
    }

    /// Whether the skin directory actually contains a file for this
    /// (lookup-key form) texture name - `assign_regions` filters against
    /// this so builtin fallback sprites never leak into the legacy skin's
    /// texture table (they would be drawn with legacy authored-size
    /// semantics and render oversized).
    pub fn provides(&self, name: &str) -> bool {
        self.files.contains_key(&name.to_lowercase().replace('\\', "/"))
    }

    /// Number of image files discovered (diagnostics).
    pub fn texture_count(&self) -> usize {
        self.files.len()
    }

    /// `shouldConvertToGrayscale` (name with or without `@2x`).
    fn should_convert_to_grayscale(name: &str) -> bool {
        GRAYSCALE_SPRITES
            .iter()
            .any(|s| name.eq_ignore_ascii_case(s) || name.eq_ignore_ascii_case(&format!("{}@2x", s)))
    }

    /// BT.601 luma (`0.299 r + 0.587 g + 0.114 b`, stable's pTexture).
    fn convert_to_grayscale(image: &mut Image) {
        for px in image.rgba.chunks_exact_mut(4) {
            let luma = (px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114) as u8;
            px[0] = luma;
            px[1] = luma;
            px[2] = luma;
        }
    }

    /// `LegacySkin.GetComboColour`: `comboColours[colourIndex % len]`.
    fn get_combo_colour(&self, colour_index: usize) -> Option<Colour> {
        let colours = self.configuration.combo_colours()?;
        colours.get(colour_index % colours.len()).copied()
    }

    /// `LegacySkin.legacySettingLookup`.
    fn legacy_setting_lookup(&self, setting: super::configuration::LegacySetting) -> Option<SkinValue> {
        use super::configuration::LegacySetting;
        match setting {
            LegacySetting::Version => Some(SkinValue::F64(
                self.configuration.effective_legacy_version(),
            )),
            LegacySetting::InputOverlayText => Some(SkinValue::Colour(
                self.configuration
                    .custom_colours
                    .get("InputOverlayText")
                    .copied()
                    .unwrap_or(Colour::BLACK),
            )),
            _ => self.generic_lookup(&SkinLookup::LegacySetting(setting)),
        }
    }

    /// `LegacySkin.genericLookup`: raw `ConfigDictionary` string lookups.
    /// The value stays a string; the typed conversion (including the
    /// 0/non-zero boolean special case) happens in `SkinValue::as_bool`
    /// and friends - the same place lazer's `Bindable<TValue>.Parse`
    /// does it.
    fn generic_lookup(&self, lookup: &SkinLookup) -> Option<SkinValue> {
        let key = lookup.generic_key()?;
        self.configuration
            .config_dictionary
            .get(&key)
            .map(|v| SkinValue::Str(v.clone()))
    }

    /// `LegacySkin.lookupForMania` - the full per-keycount switch. Creates
    /// the keycount's default config on first access like lazer does.
    fn lookup_for_mania(&self, lookup: LegacyManiaSkinConfigurationLookup) -> Option<SkinValue> {
        // Materialise (or create) the config for this keycount.
        let existing = {
            let mut configs = self.mania_configurations.borrow_mut();
            configs
                .entry(lookup.total_columns)
                .or_insert_with(|| LegacyManiaSkinConfiguration::new(lookup.total_columns))
                .clone()
        };

        let version_below_2_5 = self
            .legacy_setting_lookup(super::configuration::LegacySetting::Version)
            .and_then(|v| v.as_f64())
            .is_some_and(|v| v < 2.5);

        let custom_colour = |name: &str| -> Option<SkinValue> {
            existing
                .custom_colours
                .get(name)
                .copied()
                .map(SkinValue::Colour)
        };
        let mania_image = |name: &str| -> Option<SkinValue> {
            existing
                .image_lookups
                .get(name)
                .cloned()
                .map(SkinValue::ManiaImage)
        };
        let column = |i: Option<usize>| i.filter(|i| *i < lookup.total_columns);

        match lookup.lookup {
            ManiaLookup::ColumnWidth => {
                let i = column(lookup.column_index)?;
                Some(SkinValue::F32(existing.column_width[i]))
            }
            ManiaLookup::WidthForNoteHeightScale => {
                let mut width = existing.width_for_note_height_scale;
                if width <= 0.0 {
                    width = existing.minimum_column_width();
                }
                Some(SkinValue::F32(width))
            }
            ManiaLookup::HitPosition => Some(SkinValue::F32(existing.hit_position)),
            ManiaLookup::ComboPosition => Some(SkinValue::F32(existing.combo_position)),
            ManiaLookup::ScorePosition => Some(SkinValue::F32(existing.score_position)),
            ManiaLookup::LightPosition => Some(SkinValue::F32(existing.light_position)),
            ManiaLookup::ShowJudgementLine => Some(SkinValue::Bool(existing.show_judgement_line)),
            ManiaLookup::ExplosionImage => mania_image("LightingN"),
            ManiaLookup::ColumnLineColour => custom_colour("ColourColumnLine"),
            ManiaLookup::JudgementLineColour => custom_colour("ColourJudgementLine"),
            ManiaLookup::ColumnBackgroundColour => {
                let i = column(lookup.column_index)?;
                custom_colour(&format!("Colour{}", i + 1))
            }
            ManiaLookup::ColumnLightColour => {
                let i = column(lookup.column_index)?;
                custom_colour(&format!("ColourLight{}", i + 1))
            }
            ManiaLookup::ComboBreakColour => custom_colour("ColourBreak"),
            ManiaLookup::BarLineColour => custom_colour("ColourBarline"),
            ManiaLookup::MinimumColumnWidth => Some(SkinValue::F32(existing.minimum_column_width())),
            ManiaLookup::BarLineHeight => Some(SkinValue::F32(existing.bar_line_height)),
            ManiaLookup::NoteBodyStyle => {
                if let Some(style) = existing.note_body_style {
                    return Some(SkinValue::NoteBodyStyle(style));
                }
                if version_below_2_5 {
                    // `new Bindable<LegacyNoteBodyStyle>()` = default(0) = Stretch.
                    Some(SkinValue::NoteBodyStyle(LegacyNoteBodyStyle::Stretch))
                } else {
                    Some(SkinValue::NoteBodyStyle(LegacyNoteBodyStyle::RepeatBottom))
                }
            }
            ManiaLookup::NoteImage => {
                let i = column(lookup.column_index)?;
                mania_image(&format!("NoteImage{}", i))
            }
            ManiaLookup::HoldNoteHeadImage => {
                let i = column(lookup.column_index)?;
                mania_image(&format!("NoteImage{}H", i))
            }
            ManiaLookup::HoldNoteTailImage => {
                let i = column(lookup.column_index)?;
                mania_image(&format!("NoteImage{}T", i))
            }
            ManiaLookup::HoldNoteBodyImage => {
                let i = column(lookup.column_index)?;
                mania_image(&format!("NoteImage{}L", i))
            }
            ManiaLookup::HoldNoteLightImage => mania_image("LightingL"),
            ManiaLookup::KeyImage => {
                let i = column(lookup.column_index)?;
                mania_image(&format!("KeyImage{}", i))
            }
            ManiaLookup::KeyImageDown => {
                let i = column(lookup.column_index)?;
                mania_image(&format!("KeyImage{}D", i))
            }
            ManiaLookup::LeftStageImage => mania_image("StageLeft"),
            ManiaLookup::RightStageImage => mania_image("StageRight"),
            ManiaLookup::BottomStageImage => mania_image("StageBottom"),
            ManiaLookup::LightImage => mania_image("StageLight"),
            ManiaLookup::HitTargetImage => mania_image("StageHint"),
            ManiaLookup::Hit0 => mania_image("Hit0"),
            ManiaLookup::Hit50 => mania_image("Hit50"),
            ManiaLookup::Hit100 => mania_image("Hit100"),
            ManiaLookup::Hit200 => mania_image("Hit200"),
            ManiaLookup::Hit300 => mania_image("Hit300"),
            ManiaLookup::Hit300g => mania_image("Hit300g"),
            ManiaLookup::KeysUnderNotes => Some(SkinValue::Bool(existing.keys_under_notes)),
            ManiaLookup::LightFramePerSecond => Some(SkinValue::I64(existing.light_frame_per_second as i64)),
            ManiaLookup::LeftColumnSpacing => {
                let i = column(lookup.column_index)?;
                if i == 0 {
                    Some(SkinValue::F32(0.0))
                } else {
                    Some(SkinValue::F32(existing.column_spacing[i - 1] / 2.0))
                }
            }
            ManiaLookup::RightColumnSpacing => {
                let i = lookup.column_index?;
                if i == existing.column_spacing.len() {
                    Some(SkinValue::F32(0.0))
                } else if i < existing.column_spacing.len() {
                    Some(SkinValue::F32(existing.column_spacing[i] / 2.0))
                } else {
                    None
                }
            }
            ManiaLookup::LeftLineWidth => {
                let i = lookup.column_index?;
                existing.column_line_width.get(i).map(|w| SkinValue::F32(*w))
            }
            ManiaLookup::RightLineWidth => {
                let i = lookup.column_index?;
                existing.column_line_width.get(i + 1).map(|w| SkinValue::F32(*w))
            }
            ManiaLookup::ExplosionScale => {
                let i = column(lookup.column_index)?;
                if version_below_2_5 {
                    return Some(SkinValue::F32(1.0));
                }
                if existing.explosion_width[i] != 0.0 {
                    Some(SkinValue::F32(existing.explosion_width[i] / DEFAULT_COLUMN_SIZE))
                } else {
                    Some(SkinValue::F32(existing.column_width[i] / DEFAULT_COLUMN_SIZE))
                }
            }
            ManiaLookup::HoldNoteLightScale => {
                let i = column(lookup.column_index)?;
                if version_below_2_5 {
                    return Some(SkinValue::F32(1.0));
                }
                if existing.hold_note_light_width[i] != 0.0 {
                    Some(SkinValue::F32(existing.hold_note_light_width[i] / DEFAULT_COLUMN_SIZE))
                } else {
                    Some(SkinValue::F32(existing.column_width[i] / DEFAULT_COLUMN_SIZE))
                }
            }
            // Lookups lazer's `lookupForMania` does not handle (stage
            // paddings were added for non-legacy skins): resolve as null.
            ManiaLookup::StagePaddingTop | ManiaLookup::StagePaddingBottom => None,
        }
    }
}

impl Skin for LegacySkin {
    fn name(&self) -> &str {
        &self.name
    }

    fn configuration(&self) -> &SkinConfiguration {
        &self.configuration
    }

    fn is_legacy(&self) -> bool {
        true
    }

    /// `LegacySkin.GetTexture` (`AllowHighResolutionSprites` = true):
    /// alias, strip `@2x`, try the `@2x` sprite first with
    /// `ScaleAdjust = 2`, then the base sprite. The lookup name is
    /// lowercased first - `RealmBackedResourceStore.getPathForFile`
    /// lowercases lookups, making file access case-insensitive (mixed-case
    /// font prefixes from skin.ini rely on this).
    fn get_texture(&self, component_name: &str) -> Option<SkinTexture> {
        let mut component_name = match component_name {
            "Menu/fountain-star" => "star2",
            "Intro/Welcome/welcome_text" => "welcome_text",
            other => other,
        }
        .to_string();

        // Some component names (user-controlled ones like mania `HitX`)
        // may contain `@2x` scale specifications; stable strips them.
        component_name = component_name.replace("@2x", "");
        // `RealmBackedResourceStore` lowercases lookups and standardises
        // their separators (`ToStandardisedPath`), making file access
        // case-insensitive and letting ini prefixes name subdirectories.
        let component_name = component_name.to_lowercase().replace('\\', "/");

        let lookup = |name: &str| self.textures.get(name).copied();

        let texture = lookup(&format!("{}@2x", component_name));
        match texture {
            Some(mut t) => {
                t.scale_adjust = 2.0;
                Some(t)
            }
            None => lookup(&component_name),
        }
    }

    /// `LegacySkin.GetConfig`: the full lookup switch.
    fn get_config(&self, lookup: SkinLookup) -> Option<SkinValue> {
        match lookup {
            SkinLookup::GlobalColour(colour) => match colour {
                GlobalSkinColours::ComboColours => {
                    self.configuration.combo_colours().map(SkinValue::ComboColours)
                }
                _ => self
                    .configuration
                    .custom_colours
                    .get(GlobalSkinColours::MenuGlow.key())
                    .copied()
                    .map(SkinValue::Colour),
            },
            SkinLookup::ComboColour(SkinComboColourLookup { colour_index, .. }) => {
                self.get_combo_colour(colour_index).map(SkinValue::Colour)
            }
            SkinLookup::CustomColour(SkinCustomColourLookup(name)) => {
                self.configuration.custom_colours.get(&name).copied().map(SkinValue::Colour)
            }
            SkinLookup::Mania(mania_lookup) => self.lookup_for_mania(mania_lookup),
            SkinLookup::LegacySetting(setting) => self.legacy_setting_lookup(setting),
            SkinLookup::Generic(_) => self.generic_lookup(&lookup),
        }
    }

    /// `LegacySkin.GetSample` reduced to path resolution: the stem lookup
    /// with the last path piece fallback (`getFallbackSampleNames`).
    fn get_sample(&self, name: &str) -> Option<PathBuf> {
        let names = [name, name.rsplit('/').next().unwrap_or(name)];
        for n in names {
            if let Some(p) = self.samples.get(&n.to_lowercase()) {
                return Some(p.clone());
            }
        }
        None
    }
}

/// Texture element names the renderer consumes (the osu!-ruleset
/// skinnable components `scene.rs` draws - the texture side of lazer's
/// `SkinComponentsRepository` for this renderer). Only these (plus their
/// `@2x` variants, animation frames and number-font digits) are decoded
/// and packed into the atlas, like lazer's on-demand texture loads;
/// packing a whole skin directory would overflow the atlas (GPU max
/// texture height).
const CONSUMED_TEXTURE_NAMES: &[&str] = &[
    "hitcircle",
    "hitcircleoverlay",
    "sliderstartcircle",
    "sliderstartcircleoverlay",
    "approachcircle",
    "reversearrow",
    "sliderb",
    "sliderb-nd",
    "sliderb-spec",
    "sliderfollowcircle",
    "sliderscorepoint",
    "cursor",
    "cursormiddle",
    "cursortrail",
    "followpoint",
    "hit0",
    "hit50",
    "hit100",
    "hit300",
    "spinner-glow",
    "spinner-background",
    "spinner-circle",
    "spinner-metre",
    "spinner-approachcircle",
    "spinner-top",
    "spinner-bottom",
    "spinner-middle",
    "spinner-middle2",
    "spinner-rpm",
    "spinner-spin",
    "spinner-clear",
    // Legacy HUD pieces (LegacyHealthDisplay / LegacyKeyCounterDisplay).
    "scorebar-bg",
    "scorebar-colour",
    "scorebar-marker",
    "scorebar-ki",
    "scorebar-kidanger",
    "scorebar-kidanger2",
    "inputoverlay-background",
    "inputoverlay-key",
];

/// Names whose `{name}{sep}{i}` frame sequences are probed (the
/// `GetAnimation` callers' element set; the slider ball requests no
/// separator).
const ANIMATABLE_TEXTURE_NAMES: &[(&str, &str)] = &[
    ("hit0", "-"),
    ("hit50", "-"),
    ("hit100", "-"),
    ("hit300", "-"),
    ("sliderfollowcircle", "-"),
    ("cursor", "-"),
    ("cursortrail", "-"),
    ("reversearrow", "-"),
    ("followpoint", "-"), // OsuLegacySkinTransformer: GetAnimation("followpoint", true, true, true)
    ("spinner-background", "-"),
    ("spinner-circle", "-"),
    ("spinner-metre", "-"),
    ("sliderb", ""),
    // LegacyHealthDisplay's fill is a frame sequence when the skin ships
    // `scorebar-colour-0..` (`LegacyFill`'s GetAnimation probe).
    ("scorebar-colour", "-"),
];

impl LegacySkin {
    /// The set of lookup names to decode: consumed elements, animation
    /// frames, number-font digits - each with its `@2x` variant.
    fn consumed_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut push = |name: String, names: &mut Vec<String>| {
            names.push(format!("{}@2x", name));
            names.push(name);
        };

        for &name in CONSUMED_TEXTURE_NAMES {
            push(name.to_string(), &mut names);
        }

        // Animation frames: probe sequential frames while they exist
        // (capped well above any real skin).
        for &(name, sep) in ANIMATABLE_TEXTURE_NAMES {
            for i in 0..64 {
                let frame = format!("{}{}{}", name, sep, i);
                let exists = self.files.contains_key(&frame)
                    || self.files.contains_key(&format!("{}@2x", frame));
                if !exists {
                    break;
                }
                push(frame, &mut names);
            }
        }

        // Number fonts (`GetFontPrefix`): hitcircle / score / combo
        // digits plus the score punctuation sprites.
        use super::configuration::LegacySetting;
        let prefix = |setting: LegacySetting, default: &str| {
            self.get_config(SkinLookup::LegacySetting(setting))
                .and_then(|v| v.into_string())
                .unwrap_or_else(|| default.to_string())
                .to_lowercase()
                .replace('\\', "/")
        };
        for font_prefix in [
            prefix(LegacySetting::HitCirclePrefix, "default"),
            prefix(LegacySetting::ScorePrefix, "score"),
            prefix(LegacySetting::ComboPrefix, "score"),
        ] {
            for d in 0..10 {
                push(format!("{}-{}", font_prefix, d), &mut names);
            }
            // `LegacySpriteText`'s punctuation glyphs: comma / dot / percent
            // via `getLookupName`, plus the plain `x` the combo counter
            // formats onto every value (`{count}x`).
            for extra in ["comma", "dot", "percent", "x"] {
                push(format!("{}-{}", font_prefix, extra), &mut names);
            }
        }

        // The key-counter entry font is a fixed prefix (no skin.ini key).
        for d in 0..10 {
            push(format!("scoreentry-{}", d), &mut names);
        }

        names.sort();
        names.dedup();
        names
    }
}

impl SkinTextureSource for LegacySkin {
    /// Decode the consumed textures of the directory. `@2x` files are
    /// keyed by their suffixed stem so `get_texture` can prefer them;
    /// oversize images are downscaled to the atlas width limit
    /// (`MaxDimensionLimitedTextureLoaderStore`'s role).
    fn texture_images(&self) -> Vec<(String, Image)> {
        const MAX_DIM: u32 = 4096;
        let mut images = Vec::new();
        for name in self.consumed_names() {
            let Some(path) = self.files.get(&name) else {
                continue;
            };
            let mut image = match crate::decode_image_file(path) {
                Ok(img) => img,
                Err(e) => {
                    eprintln!("skin: skipping {}: {}", path.display(), e);
                    continue;
                }
            };

            if Self::should_convert_to_grayscale(&name) {
                Self::convert_to_grayscale(&mut image);
            }

            if image.width > MAX_DIM || image.height > MAX_DIM {
                let scale = (MAX_DIM as f32 / image.width.max(image.height) as f32).min(1.0);
                image = downscale(&image, scale);
            }
            images.push((name, image));
        }
        images
    }

    fn assign_regions(&mut self, regions: &[(String, SkinTexture)]) {
        self.textures.clear();
        for (name, tex) in regions {
            self.textures.insert(name.clone(), *tex);
        }
    }

    fn animation_names(&self) -> Vec<String> {
        // Frame files are declared through `consumed_names`; nothing
        // extra to report here.
        Vec::new()
    }
}

/// Bilinear downscale (lazer's ImageSharp resize uses a box-pass; for
/// atlas fitting the filter choice is invisible at these ratios).
pub(crate) fn downscale(image: &Image, scale: f32) -> Image {
    let w = ((image.width as f32 * scale).ceil() as u32).max(1);
    let h = ((image.height as f32 * scale).ceil() as u32).max(1);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let sy = (y as f32 + 0.5) / scale - 0.5;
        let y0 = sy.floor().clamp(0.0, image.height as f32 - 1.0) as u32;
        let y1 = (y0 + 1).min(image.height - 1);
        let fy = (sy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..w {
            let sx = (x as f32 + 0.5) / scale - 0.5;
            let x0 = sx.floor().clamp(0.0, image.width as f32 - 1.0) as u32;
            let x1 = (x0 + 1).min(image.width - 1);
            let fx = (sx - x0 as f32).clamp(0.0, 1.0);
            let px = |xx: u32, yy: u32| -> [f32; 4] {
                let i = ((yy * image.width + xx) * 4) as usize;
                [
                    image.rgba[i] as f32,
                    image.rgba[i + 1] as f32,
                    image.rgba[i + 2] as f32,
                    image.rgba[i + 3] as f32,
                ]
            };
            let lerp4 = |a: [f32; 4], b: [f32; 4], f: f32| -> [f32; 4] {
                std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
            };
            let top = lerp4(px(x0, y0), px(x1, y0), fx);
            let bottom = lerp4(px(x0, y1), px(x1, y1), fx);
            let out: [u8; 4] = std::array::from_fn(|i| lerp4(top, bottom, fy)[i] as u8);
            let d = ((y * w + x) * 4) as usize;
            rgba[d..d + 4].copy_from_slice(&out);
        }
    }
    Image { width: w, height: h, rgba }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Region;
    use crate::skin::configuration::LegacySetting;

    fn skin_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orr_skin_test_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 2x2 PNG (white opaque) generated with the `png` crate.
    fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(rgba).unwrap();
    }

    /// Decode + hand back atlas regions so `get_texture` serves handles.
    fn assign_atlas(skin: &mut LegacySkin) {
        let images = SkinTextureSource::texture_images(skin);
        let regions: Vec<(String, SkinTexture)> = images
            .iter()
            .enumerate()
            .map(|(i, (name, img))| {
                (
                    name.clone(),
                    SkinTexture {
                        region: Region::Skin(i as u32),
                        width: img.width,
                        height: img.height,
                        scale_adjust: 1.0,
                    },
                )
            })
            .collect();
        SkinTextureSource::assign_regions(skin, &regions);
    }

    #[test]
    fn loads_configuration_and_colours() {
        let dir = skin_dir("colours");
        std::fs::write(
            dir.join("skin.ini"),
            "[General]\nName: MySkin\nVersion: 2.4\nAllowSliderBallTint: 1\n[Colours]\nCombo1: 20,30,40\nSliderBall: 2,170,255\n",
        )
        .unwrap();
        let skin = LegacySkin::from_directory(&dir).unwrap();
        assert_eq!(skin.name(), "MySkin");
        assert_eq!(skin.configuration().legacy_version, Some(2.4));
        let colours = skin.get_config(SkinLookup::GlobalColour(GlobalSkinColours::ComboColours)).unwrap();
        assert_eq!(colours, SkinValue::ComboColours(vec![Colour::rgba_bytes(20, 30, 40, 255)]));
        let combo = skin
            .get_config(SkinLookup::ComboColour(SkinComboColourLookup { colour_index: 3, combo_index: 0 }))
            .unwrap();
        assert_eq!(combo, SkinValue::Colour(Colour::rgba_bytes(20, 30, 40, 255)));
        let ball = skin.get_config(SkinLookup::CustomColour(SkinCustomColourLookup("SliderBall".into()))).unwrap();
        assert_eq!(ball, SkinValue::Colour(Colour::rgba_bytes(2, 170, 255, 255)));
        // Generic dictionary lookup, including the 0/1 boolean special case.
        let slider_tint = skin
            .get_config(SkinLookup::LegacySetting(LegacySetting::AllowSliderBallTint))
            .unwrap();
        assert_eq!(slider_tint, SkinValue::Str("1".to_string()));
        assert_eq!(slider_tint.as_bool(), Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_ini_defaults_to_latest() {
        let dir = skin_dir("missing_ini");
        let skin = LegacySkin::from_directory(&dir).unwrap();
        assert_eq!(skin.configuration().legacy_version, Some(LATEST_VERSION));
        assert!(skin.configuration().is_latest_version);
        assert_eq!(skin.name(), dir.file_name().unwrap().to_str().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn texture_lookup_prefers_2x_and_reports_scale() {
        let dir = skin_dir("2x");
        write_png(&dir.join("hitcircle.png"), 2, 2, &[255; 4 * 4]);
        write_png(&dir.join("hitcircle@2x.png"), 4, 4, &[255; 4 * 16]);
        write_png(&dir.join("HitCircleOverlay.PNG"), 2, 2, &[255; 4 * 4]);
        let mut skin = LegacySkin::from_directory(&dir).unwrap();

        // Atlas round: decode + assign regions.
        let images = SkinTextureSource::texture_images(&skin);
        let regions: Vec<(String, SkinTexture)> = images
            .iter()
            .enumerate()
            .map(|(i, (name, img))| {
                (
                    name.clone(),
                    SkinTexture {
                        region: Region::Skin(i as u32),
                        width: img.width,
                        height: img.height,
                        scale_adjust: 1.0,
                    },
                )
            })
            .collect();
        SkinTextureSource::assign_regions(&mut skin, &regions);

        let tex = skin.get_texture("hitcircle").unwrap();
        assert_eq!(tex.scale_adjust, 2.0);
        // File names are case-insensitive.
        assert!(skin.get_texture("hitcircleoverlay").is_some());
        // @2x in the lookup name is stripped (stable behaviour).
        let tex = skin.get_texture("hitcircle@2x").unwrap();
        assert_eq!(tex.scale_adjust, 2.0);
        assert!(skin.get_texture("nonexistent").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grayscale_sprites_convert() {
        // The conversion targets taiko sprites, which the osu!-standard
        // consumer list never requests - so it is tested on the function
        // level (`should_convert_to_grayscale` + BT.601 luma).
        assert!(LegacySkin::should_convert_to_grayscale("taikohitcircle"));
        assert!(LegacySkin::should_convert_to_grayscale("taikohitcircle@2x"));
        assert!(!LegacySkin::should_convert_to_grayscale("hitcircle"));
        let mut image = Image {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
        };
        LegacySkin::convert_to_grayscale(&mut image);
        // BT.601: red -> 76, green -> 149 (0.587 * 255 = 149.7).
        assert_eq!(image.rgba[0], 76);
        assert_eq!(image.rgba[4], 149);
    }

    #[test]
    fn mania_lookup_uses_ini_config() {
        let dir = skin_dir("mania");
        std::fs::write(
            dir.join("skin.ini"),
            "[Mania]\nKeys: 4\nColumnWidth: 20,14,14,20\nNoteImage2: mine.png\nColourColumnLine: 1,2,3\n",
        )
        .unwrap();
        let skin = LegacySkin::from_directory(&dir).unwrap();
        let lookup = |l: ManiaLookup| {
            skin.get_config(SkinLookup::Mania(LegacyManiaSkinConfigurationLookup {
                total_columns: 4,
                column_index: Some(2),
                lookup: l,
            }))
        };
        assert_eq!(lookup(ManiaLookup::ColumnWidth).unwrap(), SkinValue::F32(22.4)); // 14 x1.6
        assert_eq!(
            lookup(ManiaLookup::NoteImage).unwrap(),
            SkinValue::ManiaImage("mine.png".to_string())
        );
        assert_eq!(
            lookup(ManiaLookup::ColumnLineColour).unwrap(),
            SkinValue::Colour(Colour::rgba_bytes(1, 2, 3, 255))
        );
        // Unconfigured keycounts lazily get the default config.
        let fresh = skin.get_config(SkinLookup::Mania(LegacyManiaSkinConfigurationLookup {
            total_columns: 9,
            column_index: None,
            lookup: ManiaLookup::HitPosition,
        }));
        assert_eq!(fresh, Some(SkinValue::F32(DEFAULT_HIT_POSITION_FOR_TEST)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `LegacyManiaSkinConfiguration.DEFAULT_HIT_POSITION` =
    /// (480 - 402) * 1.6.
    const DEFAULT_HIT_POSITION_FOR_TEST: f32 = (480.0 - 402.0) * STABLE_MAGIC_SCALE_FACTOR;

    #[test]
    fn samples_resolve_wav_first() {
        let dir = skin_dir("samples");
        std::fs::write(dir.join("normal-hitnormal.mp3"), b"x").unwrap();
        std::fs::write(dir.join("normal-hitnormal.wav"), b"x").unwrap();
        std::fs::write(dir.join("soft-hitclap.ogg"), b"x").unwrap();
        let skin = LegacySkin::from_directory(&dir).unwrap();
        assert!(skin.get_sample("normal-hitnormal").unwrap().ends_with("normal-hitnormal.wav"));
        assert!(skin.get_sample("soft-hitclap").unwrap().ends_with("soft-hitclap.ogg"));
        assert!(skin.get_sample("nope").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Placeholder-empty and corrupt pngs (skins ship them to disable an
    /// element) must be skipped like the framework's `TextureLoaderStore`
    /// swallowing decode exceptions - not panic.
    #[test]
    fn invalid_pngs_are_skipped() {
        let dir = skin_dir("invalid_png");
        std::fs::write(dir.join("hitcircle.png"), b"").unwrap(); // 0 bytes
        std::fs::write(dir.join("cursor.png"), b"not a png at all").unwrap(); // garbage
        write_png(&dir.join("reversearrow.png"), 2, 2, &[255; 4 * 4]); // one valid file
        let mut skin = LegacySkin::from_directory(&dir).unwrap();
        let images = SkinTextureSource::texture_images(&skin);
        let names: Vec<&str> = images.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, &["reversearrow"]);
        assign_atlas(&mut skin);
        assert!(skin.get_texture("hitcircle").is_none());
        assert!(skin.get_texture("cursor").is_none());
        assert!(skin.get_texture("reversearrow").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `RealmBackedResourceStore` lowercases lookups: a mixed-case
    /// `HitCirclePrefix` from skin.ini must still find `Mine-0.png`.
    #[test]
    fn mixed_case_prefix_resolves() {
        let dir = skin_dir("case_prefix");
        std::fs::write(dir.join("skin.ini"), "[General]\nVersion: 2.5\nHitCirclePrefix: Mine\n").unwrap();
        write_png(&dir.join("Mine-0.png"), 2, 2, &[255; 4 * 4]);
        write_png(&dir.join("Mine-9.png"), 2, 2, &[255; 4 * 4]);
        let mut skin = LegacySkin::from_directory(&dir).unwrap();
        assign_atlas(&mut skin);
        assert_eq!(
            super::super::texture::get_font_prefix(&skin, super::super::texture::LegacyFont::HitCircle),
            "Mine"
        );
        assert!(skin.get_texture("Mine-0").is_some());
        assert!(skin.get_texture("MINE-9").is_some());
        assert!(super::super::texture::has_font(&skin, super::super::texture::LegacyFont::HitCircle));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prefixes may name subdirectories (`HitCirclePrefix:
    /// Assets/default/default` - Arona-style skins); files are keyed by
    /// their relative path like lazer's realm store.
    #[test]
    fn subfolder_prefix_digits_resolve() {
        let dir = skin_dir("subfolder_prefix");
        std::fs::create_dir_all(dir.join("Assets/default")).unwrap();
        std::fs::write(
            dir.join("skin.ini"),
            "[General]\nVersion: 2.5\nHitCirclePrefix: Assets/default/default\nScorePrefix: Assets\\score\\score\n",
        )
        .unwrap();
        write_png(&dir.join("Assets/default/default-0.png"), 2, 2, &[255; 4 * 4]);
        write_png(&dir.join("Assets/default/default-1@2x.png"), 4, 4, &[255; 4 * 16]);
        std::fs::create_dir_all(dir.join("Assets/score")).unwrap();
        write_png(&dir.join("Assets/score/score-0.png"), 2, 2, &[255; 4 * 4]);
        let mut skin = LegacySkin::from_directory(&dir).unwrap();
        assign_atlas(&mut skin);
        // Mixed case + subpath resolve through the lowercased relative key.
        assert!(skin.get_texture("Assets/default/default-0").is_some());
        assert!(skin.get_texture("assets/DEFAULT/default-1").is_some()); // prefers the @2x file
        let hit = skin.get_texture("Assets/default/default-1").unwrap();
        assert_eq!(hit.scale_adjust, 2.0);
        // Backslash form of the prefix (Windows-authored ini) normalises.
        assert!(skin.get_texture("Assets\\score\\score-0").is_some());
        // Subfolder digit frames get packed (consumed_names probes them).
        let images = SkinTextureSource::texture_images(&skin);
        let names: Vec<&str> = images.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"assets/default/default-0"));
        assert!(names.contains(&"assets/default/default-1@2x"));
        assert!(names.contains(&"assets/score/score-0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
