//! Port of `Skinning/SkinTransformer.cs` (+`ISkinTransformer.cs`) and the
//! current-skin resolution of `Skinning/SkinManager.cs`.
//!
//! lazer resolves a drawable's skin queries through a chain of
//! `SkinProvidingContainer`s: the user skin (wrapped in the ruleset's
//! `LegacySkinTransformer`) first, the game default skin as the outer
//! fallback. [`ResolvedSkin`] is that chain: the user's
//! [`LegacySkin`] when one is loaded (every lookup asks it first), then
//! the built-in [`ArgonSkin`].

use std::path::Path;

use super::argon::ArgonSkin;
use super::configuration::SkinConfiguration;
use super::legacy::LegacySkin;
use super::lookup::SkinLookup;
use super::texture::SkinTexture;
use super::{Skin, SkinTextureSource};

/// The skin a render (or host) resolves skins against: the user skin
/// with the default skin as fallback, lazer's container chain collapsed
/// into one object.
pub struct ResolvedSkin {
    legacy: Option<LegacySkin>,
    builtin: ArgonSkin,
}

impl ResolvedSkin {
    /// The user legacy skin, when one is loaded.
    pub fn legacy(&self) -> Option<&LegacySkin> {
        self.legacy.as_ref()
    }

    /// Whether legacy (stable-format) resources should drive the visuals
    /// - lazer's `skin is LegacySkin` checks, the gate legacy-skinned
    /// drawables use.
    pub fn is_legacy(&self) -> bool {
        self.legacy.is_some()
    }

    /// A stable-element texture lookup against the USER legacy skin only.
    ///
    /// lazer's ruleset transformers (`OsuLegacySkinTransformer` etc.) wrap
    /// the legacy skin alone and decide element presence on it
    /// (`GetTexture("approachcircle") != null` gates `LegacyApproachCircle`).
    /// A missing element makes the transformer return null, and the outer
    /// default skin then provides its own component with its own sizing
    /// (e.g. `DefaultApproachCircle` fills the 128-unit object box) - the
    /// default skin's textures never feed legacy authored-size drawing.
    /// [`Self::get_texture`] chains to the builtin for other callers, so
    /// legacy element slots must use this instead to keep that split.
    pub fn legacy_texture(&self, name: &str) -> Option<SkinTexture> {
        self.legacy.as_ref().and_then(|l| l.get_texture(name))
    }

    /// The user legacy skin, for animation probing under the same
    /// legacy-only rule as [`Self::legacy_texture`].
    pub fn legacy_skin(&self) -> Option<&LegacySkin> {
        self.legacy.as_ref()
    }
}

impl Skin for ResolvedSkin {
    fn name(&self) -> &str {
        match &self.legacy {
            Some(l) => l.name(),
            None => self.builtin.name(),
        }
    }

    fn configuration(&self) -> &SkinConfiguration {
        match &self.legacy {
            Some(l) => l.configuration(),
            None => self.builtin.configuration(),
        }
    }

    fn is_legacy(&self) -> bool {
        self.legacy.is_some()
    }

    /// Ask the user skin first; on a miss fall back to the default skin
    /// (`SkinTransformer`'s pass-through per layer).
    fn get_texture(&self, name: &str) -> Option<SkinTexture> {
        match &self.legacy {
            Some(l) => l.get_texture(name).or_else(|| self.builtin.get_texture(name)),
            None => self.builtin.get_texture(name),
        }
    }

    /// Same chain for configuration values. Note the user skin's own
    /// misses already fall back internally where lazer's do (combo
    /// colours default inside `SkinConfiguration`).
    fn get_config(&self, lookup: SkinLookup) -> Option<super::lookup::SkinValue> {
        match &self.legacy {
            Some(l) => l.get_config(lookup.clone()).or_else(|| self.builtin.get_config(lookup)),
            None => self.builtin.get_config(lookup),
        }
    }

    fn get_sample(&self, name: &str) -> Option<std::path::PathBuf> {
        match &self.legacy {
            Some(l) => l.get_sample(name).or_else(|| self.builtin.get_sample(name)),
            None => self.builtin.get_sample(name),
        }
    }
}

impl SkinTextureSource for ResolvedSkin {
    fn texture_images(&self) -> Vec<(String, crate::draw::Image)> {
        // User skin first; builtin sprites whose names the user skin
        // already provides are DROPPED. `assign_regions` keys by name, so
        // a duplicate would make the later (builtin) handle overwrite the
        // user's inside the skin's texture table - a user `approachcircle`
        // (140px) would silently become the builtin 256px one and render
        // oversized. Dropping matches the lookup chain: `get_texture`
        // never reaches the builtin for a name the legacy skin serves.
        let mut images = self.legacy.as_ref().map(|l| l.texture_images()).unwrap_or_default();
        let taken: std::collections::HashSet<String> = images.iter().map(|(n, _)| n.clone()).collect();
        images.extend(
            self.builtin
                .texture_images()
                .into_iter()
                .filter(|(n, _)| !taken.contains(n)),
        );
        images
    }

    fn assign_regions(&mut self, regions: &[(String, SkinTexture)]) {
        if let Some(l) = &mut self.legacy {
            // Only hand the legacy skin the regions its own files produced:
            // `regions` also carries the builtin sprites packed for the
            // argon fallback, and a builtin "approachcircle"/"cursortrail"
            // inside the legacy table would be drawn with legacy
            // authored-size semantics (the oversized-fallback-ring bug).
            let own: Vec<(String, SkinTexture)> = regions
                .iter()
                .filter(|(name, _)| l.provides(name))
                .cloned()
                .collect();
            l.assign_regions(&own);
        }
        self.builtin.assign_regions(regions);
    }
}

/// `SkinManager`'s current-skin resolution for this renderer: load a
/// user skin directory when given (the unpacked `.osk` content, or the
/// game's `Skins/<name>` folder), else the built-in default.
pub fn load_skin(path: Option<&Path>) -> Result<ResolvedSkin, String> {
    let builtin = ArgonSkin::new();
    match path {
        Some(p) => {
            let legacy = LegacySkin::from_directory(p)?;
            eprintln!(
                "skin: \"{}\" (legacy v{}, {} texture files) - missing elements fall back to argon",
                legacy.name(),
                legacy.configuration().effective_legacy_version(),
                legacy.texture_count()
            );
            Ok(ResolvedSkin { legacy: Some(legacy), builtin })
        }
        None => Ok(ResolvedSkin { legacy: None, builtin }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&[255; 16]).unwrap();
    }

    fn assign_atlas(skin: &mut ResolvedSkin) {
        let images = super::super::SkinTextureSource::texture_images(skin);
        let regions: Vec<(String, SkinTexture)> = images
            .iter()
            .enumerate()
            .map(|(i, (name, img))| {
                (
                    name.clone(),
                    SkinTexture {
                        region: crate::draw::Region::Skin(i as u32),
                        width: img.width,
                        height: img.height,
                        scale_adjust: 1.0,
                    },
                )
            })
            .collect();
        super::super::SkinTextureSource::assign_regions(skin, &regions);
    }

    /// A skin without `approachcircle` must NOT pick the builtin sprite up
    /// through the legacy element path - neither at lookup time nor inside
    /// the legacy skin's texture table after the merged atlas assignment
    /// (the argon 256px ring drawn with legacy authored-size semantics is
    /// the oversized-approach-circle bug; lazer's transformer decides
    /// presence on the user skin alone).
    #[test]
    fn legacy_texture_does_not_leak_builtin_sprites() {
        let dir = std::env::temp_dir().join(format!("orr_resolved_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Empty skin: after the atlas round the builtin sprites are packed
        // (for the argon fallback chain) but must not enter the legacy
        // skin's table.
        let mut skin = load_skin(Some(&dir)).unwrap();
        assign_atlas(&mut skin);
        assert!(skin.legacy_texture("approachcircle").is_none());
        assert!(skin.legacy_texture("cursortrail").is_none());
        // The chained lookup still serves the builtin fallback.
        assert!(skin.get_texture("approachcircle").is_some());

        // An element the skin ships resolves through the legacy path.
        write_png(&dir.join("approachcircle.png"), 2, 2);
        let mut skin = load_skin(Some(&dir)).unwrap();
        assign_atlas(&mut skin);
        assert!(skin.legacy_texture("approachcircle").is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
