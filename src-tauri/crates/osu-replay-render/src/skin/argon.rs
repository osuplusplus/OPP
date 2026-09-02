//! Port of `osu.Game/Skinning/ArgonSkin.cs` reduced to what this renderer
//! consumes: the game's default skin. Its hit objects are vector-drawn
//! (the existing `scene.rs` Argon visuals ARE this skin), so the only
//! textures it serves are the three embedded sprites the Argon components
//! use. `GetConfig` answers the default combo colours - everything else
//! misses so lookups fall through to the user skin first when chained
//! (lazer: user skin container wraps the default skin container).

use std::path::PathBuf;

use crate::draw::Image;

use super::configuration::SkinConfiguration;
use super::lookup::{GlobalSkinColours, SkinComboColourLookup, SkinLookup, SkinValue};
use super::texture::SkinTexture;
use super::{Skin, SkinTextureSource};

/// Embedded Argon sprites (the same bytes `lib.rs` packs into the atlas
/// as fixed regions).
const CURSOR_TRAIL_PNG: &[u8] = include_bytes!("../../assets/cursor/cursortrail.png");
const REPEAT_EDGE_PNG: &[u8] = include_bytes!("../../assets/cursor/repeat-edge-piece.png");
const APPROACH_CIRCLE_PNG: &[u8] = include_bytes!("../../assets/cursor/approachcircle.png");

/// The built-in Argon (default) skin.
pub struct ArgonSkin {
    configuration: SkinConfiguration,
    textures: Vec<(&'static str, Image, Option<SkinTexture>)>,
}

impl ArgonSkin {
    pub fn new() -> ArgonSkin {
        // `Skin` constructor with no configuration stream: latest version.
        let mut configuration = SkinConfiguration::default();
        configuration.legacy_version = Some(super::configuration::LATEST_VERSION);
        configuration.is_latest_version = true;

        let decode = |bytes: &[u8]| -> Image {
            let (w, h, rgba) = decode_png_rgba(bytes);
            Image { width: w, height: h, rgba }
        };
        ArgonSkin {
            configuration,
            textures: vec![
                ("cursortrail", decode(CURSOR_TRAIL_PNG), None),
                ("repeat-edge-piece", decode(REPEAT_EDGE_PNG), None),
                ("approachcircle", decode(APPROACH_CIRCLE_PNG), None),
            ],
        }
    }

    fn find(&self, name: &str) -> Option<SkinTexture> {
        self.textures.iter().find(|(n, _, _)| *n == name).and_then(|(_, _, t)| *t)
    }
}

impl Default for ArgonSkin {
    fn default() -> ArgonSkin {
        ArgonSkin::new()
    }
}

impl Skin for ArgonSkin {
    fn name(&self) -> &str {
        "osu! \"argon\""
    }

    fn configuration(&self) -> &SkinConfiguration {
        &self.configuration
    }

    /// `ArgonSkin.GetTexture`: a plain resource lookup against the
    /// embedded set.
    fn get_texture(&self, name: &str) -> Option<SkinTexture> {
        self.find(name)
    }

    /// `ArgonSkin.GetConfig`: `GlobalSkinColours.ComboColours` and
    /// `SkinComboColourLookup` only (the defaults from
    /// `SkinConfiguration.ComboColours`); every other lookup misses.
    fn get_config(&self, lookup: SkinLookup) -> Option<SkinValue> {
        match lookup {
            SkinLookup::GlobalColour(GlobalSkinColours::ComboColours) => {
                self.configuration.combo_colours().map(SkinValue::ComboColours)
            }
            SkinLookup::ComboColour(SkinComboColourLookup { colour_index, .. }) => {
                let colours = self.configuration.combo_colours()?;
                Some(SkinValue::Colour(colours[colour_index % colours.len()]))
            }
            _ => None,
        }
    }

    /// The default skin's samples are compiled into `crate::hitsound`
    /// directly; no file-based sample resolution.
    fn get_sample(&self, _name: &str) -> Option<PathBuf> {
        None
    }
}

impl SkinTextureSource for ArgonSkin {
    fn texture_images(&self) -> Vec<(String, Image)> {
        self.textures
            .iter()
            .map(|(name, img, _)| (name.to_string(), img.clone()))
            .collect()
    }

    fn assign_regions(&mut self, regions: &[(String, SkinTexture)]) {
        for (name, tex) in regions {
            if let Some(entry) = self.textures.iter_mut().find(|(n, _, _)| n == name) {
                entry.2 = Some(*tex);
            }
        }
    }
}

fn decode_png_rgba(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    crate::decode_png_bytes(bytes).expect("embedded argon png")
}
