//! Port of the texture/animation side of skinning:
//!
//! - `SkinTexture` mirrors the framework `Texture` handle the renderer
//!   needs (atlas region, pixel size, `ScaleAdjust` for @2x sprites whose
//!   display size is half the pixel size)
//! - `get_animation` ports `LegacySkinExtensions.GetAnimation` /
//!   `GetTextures` (frame probing `{name}-{i}`, frame-length rules)
//! - `LegacyFont` + `get_font_prefix` / `get_font_overlap` / `has_font`
//!   port the number-sprite font lookups

use crate::draw::Region;

use super::configuration::LegacySetting;
use super::lookup::SkinLookup;
use super::Skin;

/// The default frame length of legacy skin animations at a 60 FPS rate
/// (`LegacySkinExtensions.SIXTY_FRAME_TIME`).
pub const SIXTY_FRAME_TIME: f64 = 1000.0 / 60.0;

/// A single skin texture, packed into the atlas at load time. The
/// framework `Texture.ScaleAdjust` becomes [`Self::scale_adjust`]: @2x
/// sprites carry 2 and display at half their pixel size.
#[derive(Clone, Copy, Debug)]
pub struct SkinTexture {
    pub region: Region,
    pub width: u32,
    pub height: u32,
    pub scale_adjust: f32,
}

impl SkinTexture {
    /// `Texture.DisplayWidth`.
    pub fn display_width(&self) -> f32 {
        self.width as f32 / self.scale_adjust
    }

    /// `Texture.DisplayHeight`.
    pub fn display_height(&self) -> f32 {
        self.height as f32 / self.scale_adjust
    }
}

/// The resolved form of `LegacySkinExtensions.GetAnimation`: either a
/// single sprite or a fixed-frame-length frame sequence.
#[derive(Clone, Debug)]
pub struct SkinAnimation {
    pub frames: Vec<SkinTexture>,
    pub frame_length: f64,
    pub looping: bool,
}

impl SkinAnimation {
    /// `TextureAnimation.CurrentFrame` at time `playback_position` (ms
    /// since the animation start). Non-looping animations clamp to the
    /// last frame.
    pub fn frame_at(&self, playback_position: f64) -> SkinTexture {
        let idx = (playback_position / self.frame_length).floor() as i64;
        let last = self.frames.len() as i64 - 1;
        let idx = if self.looping {
            idx.rem_euclid(self.frames.len() as i64) as usize
        } else {
            idx.clamp(0, last.max(0)) as usize
        };
        self.frames[idx.min(self.frames.len() - 1)]
    }
}

/// `LegacySkinExtensions.GetTextures` + `GetAnimation`: probe
/// `{name}{separator}{i}` frames from 0; when more than one exists build
/// an animation whose frame length follows `getFrameLength`, otherwise
/// return the plain sprite.
pub fn get_animation(
    skin: &dyn Skin,
    component_name: &str,
    animatable: bool,
    looping: bool,
    apply_config_frame_rate: bool,
    animation_separator: &str,
) -> Option<SkinAnimation> {
    let mut frames = Vec::new();
    if animatable {
        let mut i = 0;
        loop {
            let frame_name = format!("{}{}{}", component_name, animation_separator, i);
            match skin.get_texture(&frame_name) {
                Some(t) => frames.push(t),
                None => break,
            }
            i += 1;
        }
    }

    if frames.is_empty() {
        // Not allowed or not found: fall back to a sprite retrieval.
        let single = skin.get_texture(component_name)?;
        return Some(SkinAnimation { frames: vec![single], frame_length: SIXTY_FRAME_TIME, looping });
    }

    let frame_length = get_frame_length(skin, apply_config_frame_rate, &frames);
    Some(SkinAnimation { frames, frame_length, looping })
}

/// `LegacySkinExtensions.getFrameLength`.
fn get_frame_length(skin: &dyn Skin, apply_config_frame_rate: bool, textures: &[SkinTexture]) -> f64 {
    if apply_config_frame_rate {
        let ini_rate = skin
            .get_config(SkinLookup::LegacySetting(LegacySetting::AnimationFramerate))
            .and_then(|v| v.as_f64());
        if ini_rate.is_some_and(|r| r > 0.0) {
            return 1000.0 / ini_rate.unwrap();
        }
        return 1000.0 / textures.len() as f64;
    }
    SIXTY_FRAME_TIME
}

/// `LegacyFont`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LegacyFont {
    ScoreEntry,
    Score,
    Combo,
    HitCircle,
}

/// `LegacySkinExtensions.GetFontPrefix`.
pub fn get_font_prefix(skin: &dyn Skin, font: LegacyFont) -> String {
    let setting = |s: LegacySetting| skin.get_config(SkinLookup::LegacySetting(s));
    match font {
        LegacyFont::ScoreEntry => "scoreentry".to_string(),
        LegacyFont::Score => setting(LegacySetting::ScorePrefix)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "score".to_string()),
        LegacyFont::Combo => setting(LegacySetting::ComboPrefix)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "score".to_string()),
        LegacyFont::HitCircle => setting(LegacySetting::HitCirclePrefix)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "default".to_string()),
    }
}

/// `LegacySkinExtensions.GetFontOverlap`.
pub fn get_font_overlap(skin: &dyn Skin, font: LegacyFont) -> f32 {
    let setting = |s: LegacySetting| {
        skin.get_config(SkinLookup::LegacySetting(s)).and_then(|v| v.as_f64())
    };
    match font {
        LegacyFont::ScoreEntry => 1.0,
        LegacyFont::Score => setting(LegacySetting::ScoreOverlap).unwrap_or(0.0) as f32,
        LegacyFont::Combo => setting(LegacySetting::ComboOverlap).unwrap_or(0.0) as f32,
        LegacyFont::HitCircle => setting(LegacySetting::HitCircleOverlap).unwrap_or(-2.0) as f32,
    }
}

/// `LegacySkinExtensions.HasFont`.
pub fn has_font(skin: &dyn Skin, font: LegacyFont) -> bool {
    skin.get_texture(&format!("{}-0", get_font_prefix(skin, font))).is_some()
}
