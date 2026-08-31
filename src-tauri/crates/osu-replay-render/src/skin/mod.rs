//! The osu! skinning abstraction, ported from osu!(lazer):
//!
//! | lazer source | port |
//! |---|---|
//! | `Skinning/ISkin.cs` + `Skinning/Skin.cs` | [`Skin`] trait |
//! | `Skinning/SkinConfiguration.cs` | [`configuration::SkinConfiguration`] |
//! | `Skinning/LegacySkin.cs` | [`legacy::LegacySkin`] |
//! | `Skinning/ArgonSkin.cs` (default skin) | [`argon::ArgonSkin`] |
//! | `Beatmaps/Formats/LegacyDecoder.cs` + `Skinning/LegacySkinDecoder.cs` | [`decoder`] |
//! | `Skinning/LegacySkinDecoder`'s mania pass | [`decoder::decode_mania_configurations`] |
//! | `Skinning/LegacySkinExtensions.cs` | [`texture::get_animation`] and font helpers |
//! | `Skinning/LegacyTextureLoaderStore.cs` | [`legacy`] grayscale conversion |
//! | `Skinning/SkinTransformer.cs` / `ISkinTransformer.cs` | [`transformer::SkinTransformer`] |
//! | `Skinning/SkinManager.cs` | [`transformer::load_skin`] |
//!
//! A user skin is a directory of stable-format skin files (the unpacked
//! content of an `.osk`, or the game's `Skins/<name>` folder). The
//! [`Skin`] interface exposes exactly what lazer's does, reduced to what
//! an offline renderer consumes: textures, configuration values and
//! sample paths (no `GetDrawableComponent` - this renderer resolves
//! components to textures/draw calls in `scene.rs`, the way the osu!
//! ruleset's skinnable drawables do).
//!
//! Audio: `get_sample` resolves sample file paths with the stable
//! `wav -> mp3 -> ogg` extension ordering. The current hitsound pipeline
//! (`crate::hitsound`) still synthesizes from the embedded ArgonPro set;
//! skin sample playback would require an audio decoder and is not wired
//! into rendering yet.

pub mod argon;
pub mod configuration;
pub mod decoder;
pub mod legacy;
pub mod lookup;
pub mod texture;
pub mod transformer;

pub use configuration::{default_combo_colours, SkinConfiguration, LATEST_VERSION};
pub use lookup::{
    GlobalSkinColours, LegacyManiaSkinConfigurationLookup, LegacyManiaSkinConfigurationLookups,
    SkinComboColourLookup, SkinCustomColourLookup, SkinLookup, SkinValue,
};
pub use texture::{
    get_animation, get_font_overlap, get_font_prefix, has_font, SkinAnimation, SkinTexture,
    SIXTY_FRAME_TIME,
};
pub use transformer::{load_skin, ResolvedSkin};

use std::path::PathBuf;

/// Port of `ISkin` (merged with the abstract `Skin` base class's surface):
/// everything a consumer can ask a skin for.
pub trait Skin {
    /// `Skin.Name`.
    fn name(&self) -> &str;

    /// `Skin.Configuration`.
    fn configuration(&self) -> &SkinConfiguration;

    /// Whether this skin provides legacy (stable-format) resources - the
    /// `skin is LegacySkin` checks of lazer's transformers.
    fn is_legacy(&self) -> bool {
        false
    }

    /// `ISkin.GetTexture(componentName)`: a texture by its stable element
    /// name (e.g. `"hitcircle"`). Implementations handle `@2x` resolution
    /// and set `scale_adjust` accordingly; `None` means the skin does not
    /// provide the element (consumers fall back, lazer's
    /// `SkinProvidingContainer` chain).
    fn get_texture(&self, name: &str) -> Option<SkinTexture>;

    /// `ISkin.GetConfig<TLookup, TValue>(lookup)`: configuration values.
    fn get_config(&self, lookup: SkinLookup) -> Option<SkinValue>;

    /// `ISkin.GetSample`: resolve a sample lookup name to a file in the
    /// skin, trying extensions in stable's `wav -> mp3 -> ogg` order.
    /// Sample DECODING is out of scope for the renderer (see the module
    /// docs) - this only resolves the path.
    fn get_sample(&self, name: &str) -> Option<PathBuf>;
}

/// Skin texture loading interface used by the atlas builder: a skin
/// describes the textures it wants packed, then receives the assigned
/// atlas handles. Split this way because the atlas is one CPU-built
/// texture shared by everything (fonts, counters, skin sprites).
pub trait SkinTextureSource {
    /// All textures the skin can provide, decoded: `(texture name,
    /// image)`. Names are the lookup names (post `@2x` resolution).
    fn texture_images(&self) -> Vec<(String, crate::draw::Image)>;

    /// Called with `(name -> atlas handle + pixel size)` assignments
    /// after the atlas is built; the skin stores them for
    /// `get_texture`.
    fn assign_regions(&mut self, regions: &[(String, SkinTexture)]);

    /// Animation frame names (`{name}-{i}` sequences) discovered while
    /// scanning, so `get_animation` can serve them. Subsumed by
    /// `get_texture` in the concrete implementations; part of the source
    /// contract so packers know all frames are atlas residents.
    fn animation_names(&self) -> Vec<String> {
        Vec::new()
    }
}
