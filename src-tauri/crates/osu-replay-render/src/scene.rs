//! Per-frame scene construction: the Argon skin visuals.
//!
//! Layer order (matching `OsuPlayfield`): spinners, follow points, judgement
//! explosions, hit objects (reverse start-time order: earlier on top),
//! judgement text, approach circles, cursor + trail, HUD.

use crate::draw::{draw_ttf_text, lerp, value_at, Atlas, Blend, Colour, DrawList, Easing, TtfFont};
use crate::game::{EventView, GameData, JudgementDisplay, ObjKind, ObjView};
use crate::hud;
use crate::skin::{self, Skin, SkinAnimation, SkinTexture, SIXTY_FRAME_TIME};
use osu_replay_judge::process::NestedKind;
use osu_replay_judge::score::hit_result_ext;
use osu_replay_judge::score::HitResult;

// ---------------------------------------------------------------------------
// Argon metrics (128-space units, i.e. relative to a diameter-128 circle).
// ---------------------------------------------------------------------------

pub const BORDER_THICKNESS: f32 = 128.0 * (2.0 / 58.0); // 4.4138
pub const GRADIENT_THICKNESS: f32 = BORDER_THICKNESS * 2.5; // 11.034
pub const OUTER_GRADIENT_SIZE: f32 = 128.0 - BORDER_THICKNESS * 4.0; // 110.345
pub const INNER_GRADIENT_SIZE: f32 = OUTER_GRADIENT_SIZE - GRADIENT_THICKNESS * 2.0; // 88.276
pub const INNER_FILL_SIZE: f32 = INNER_GRADIENT_SIZE - GRADIENT_THICKNESS * 2.0; // 66.207

// ---------------------------------------------------------------------------
// Legacy skin metrics (osu.Game.Rulesets.Osu Skinning/Legacy).
// ---------------------------------------------------------------------------

/// `OsuLegacySkinTransformer.LEGACY_CIRCLE_RADIUS` = `OsuHitObject.OBJECT_RADIUS` (64) - 5.
pub const LEGACY_CIRCLE_RADIUS: f32 = 59.0;
/// `LegacySpinner.SPRITE_SCALE`.
pub const LEGACY_SPINNER_SCALE: f32 = 0.625;
/// `LegacySpinner.SPINNER_TOP_OFFSET` (window space 640x480).
pub const LEGACY_SPINNER_TOP_OFFSET: f32 = 45.0 - 16.0;
/// `LegacySpinner.SPINNER_Y_CENTRE`.
pub const LEGACY_SPINNER_Y_CENTRE: f32 = LEGACY_SPINNER_TOP_OFFSET + 219.0;
/// `LegacyCursor.REVOLUTION_DURATION`.
pub const LEGACY_CURSOR_REVOLUTION: f64 = 10_000.0;
/// `LegacyDrawableSliderPath` gradient stops (position 0 = path EDGE,
/// 1 = centre, per `DefaultDrawableSliderPath`): a faint transparent-black
/// rim over `[0, shadow]`, the border colour over `(shadow, border]`,
/// then the darkened->lightened body gradient. `shadow = 1 -
/// LEGACY_CIRCLE_RADIUS / OBJECT_RADIUS`.
pub const LEGACY_SLIDER_SHADOW_PORTION: f32 = 1.0 - LEGACY_CIRCLE_RADIUS / 64.0; // 0.078
pub const LEGACY_SLIDER_BORDER_PORTION: f32 = 0.1875;

const FOLLOW_AREA: f32 = 2.4;
const PLAYFIELD_SHIFT: f32 = 12.8; // 8 virtual px * 1.6
const PLAYFIELD_SCALE: f32 = 1.6;

const CURSOR_SIZE: f32 = 28.0;
const TRAIL_SIZE: f32 = 5.0;
const TRAIL_SPACING: f32 = 2.0;
const TRAIL_DURATION: f64 = 300.0;
const TRAIL_ALPHA: f32 = 0.8;
const TRAIL_FADE_EXPONENT: f32 = 4.0;

const SPINNER_DISC: f32 = 384.0; // playfield units
const FOLLOW_POINT_SPACING: f32 = 32.0;
const FOLLOW_POINT_PREEMPT: f64 = 800.0;

// ---------------------------------------------------------------------------
// Hidden mod (`OsuModHidden`). HD changes no judgement; it only rewrites the
// visibility timelines: non-slider objects (including slider nested head/
// ticks) fade in over `preempt * 0.4` instead of the default formula
// ("Sliders retain their default TimeFadeIn to match Stable"), then fade out
// over the windows below. Approach circles are hidden (except the first
// object). Follow points / judgements / HUD are untouched.
// ---------------------------------------------------------------------------

const HD_FADE_IN_MULTIPLIER: f64 = 0.4;
const HD_FADE_OUT_DURATION_MULTIPLIER: f64 = 0.3;
/// `DrawableSliderTick.ANIM_DURATION`.
const HD_TICK_ANIM_DURATION: f64 = 150.0;

/// HD fade-in span for non-slider objects (`preempt * 0.4`). Sliders keep
/// `obj.fade_in` (the default formula).
fn hd_fade_in(obj: &ObjView) -> f64 {
    if obj.kind == ObjKind::Slider {
        obj.fade_in
    } else {
        obj.preempt * HD_FADE_IN_MULTIPLIER
    }
}

/// `getFadeOutParameters` default (circle) case: linear fade to zero over
/// `preempt * 0.3`, starting `fade_in` after the lifetime start — the object
/// is fully invisible for the last 30% of its preempt. `fade_in` is the
/// HD-adjusted fade-in of the anchor object (circles: 0.4 × preempt; the
/// slider head/tail/repeat ride the HEAD's sequence, whose fade-in is also
/// 0.4 × preempt since it is a nested non-slider).
fn hd_fade_out(preempt: f64, start_time: f64, fade_in: f64, t: f64) -> f64 {
    let fs = start_time - preempt + fade_in;
    value_at(
        t,
        fs,
        fs + preempt * HD_FADE_OUT_DURATION_MULTIPLIER,
        1.0,
        0.0,
        Easing::Linear,
    )
}

pub fn colour_for_result(result: HitResult) -> Colour {
    match result {
        HitResult::Miss | HitResult::LargeTickMiss | HitResult::IgnoreMiss => {
            Colour::from_hex(0xFF0000)
        }
        HitResult::Meh => Colour::from_hex(0xFFCC22),
        HitResult::Ok => Colour::from_hex(0x88B300),
        HitResult::Good => Colour::from_hex(0xB3D944),
        HitResult::Great | HitResult::Perfect | HitResult::SmallTickHit | HitResult::LargeTickHit
        | HitResult::SliderTailHit => Colour::from_hex(0x66CCFF),
        _ => Colour::from_hex(0x99EEFF),
    }
}

pub fn judgement_word(result: HitResult) -> &'static str {
    match result {
        HitResult::Miss => "MISS",
        HitResult::Meh => "MEH",
        HitResult::Ok => "OK",
        HitResult::Good => "GOOD",
        HitResult::Great => "GREAT",
        HitResult::Perfect => "PERFECT",
        HitResult::SmallTickHit => "S TICK",
        HitResult::LargeTickHit => "L TICK",
        HitResult::SmallBonus => "S BONUS",
        HitResult::LargeBonus => "L BONUS",
        HitResult::IgnoreMiss | HitResult::LargeTickMiss => "SLIDER BREAK",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Screen mapping
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Mapper {
    pub screen_w: f32,
    pub screen_h: f32,
    /// Playfield unit -> screen px.
    pub pf: f32,
    pub centre: [f32; 2],
    /// Virtual 1024x768 unit -> screen px (`OsuGameBase`'s root
    /// `DrawSizePreservingFillContainer`: TargetDrawSize 1024x768, strategy
    /// Minimum -> uniform scale `min(W/1024, H/768)`; the whole UI - HUD
    /// included - lives in a local canvas of (W/s, H/s) units, i.e.
    /// 1365.33x768 at 16:9).
    pub virt: f32,
}

impl Mapper {
    pub fn new(width: u32, height: u32) -> Mapper {
        let virt = (width as f32 / 1024.0).min(height as f32 / 768.0);
        let pf = virt * PLAYFIELD_SCALE;
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0 + PLAYFIELD_SHIFT * virt;
        Mapper { screen_w: width as f32, screen_h: height as f32, pf, centre: [cx, cy], virt }
    }

    pub fn pf(&self, p: [f32; 2]) -> [f32; 2] {
        [
            self.centre[0] + (p[0] - 256.0) * self.pf,
            self.centre[1] + (p[1] - 192.0) * self.pf,
        ]
    }

    /// HUD coordinates: the uniform local units of the root
    /// `DrawSizePreservingFillContainer` (1024x768 fitted by the Minimum
    /// strategy - wider windows grow the canvas to the right, they do not
    /// stretch it). Right-anchored positions must go through
    /// `screen_w - offset * virt` instead: the canvas width in units is
    /// `screen_w / virt` (1365.33 at 16:9), not 1024.
    pub fn virt(&self, v: [f32; 2]) -> [f32; 2] {
        [v[0] * self.virt, v[1] * self.virt]
    }

    /// Legacy spinner coordinates: stable's 640x480 window space. Lazer
    /// keeps this box SQUARE: `DrawableSpinner` hosts the skin component
    /// inside an `AspectContainer` (`RelativeSizeAxes = Y`, 384x384
    /// gamefield units) and the virtual screen itself scales uniformly
    /// (`DrawSizePreservingFillContainer`, `Minimum` strategy), so the box
    /// lands at full screen HEIGHT with a 4:3 aspect, centred on screen —
    /// never stretched to the window edges. The spinner's own
    /// `Position = (0, -8)` exactly cancels the playfield's +8*1.6 shift,
    /// putting the box centre at the screen centre.
    pub fn win(&self, v: [f32; 2]) -> [f32; 2] {
        let u = self.win_unit();
        [
            self.screen_w * 0.5 + (v[0] - 320.0) * u,
            self.screen_h * 0.5 + (v[1] - 240.0) * u,
        ]
    }

    /// Window-space scale (screen px per 640x480 unit). Uniform on both
    /// axes: the box is width-constrained below 4:3 and height-constrained
    /// at 4:3 and wider (`Minimum` strategy semantics).
    pub fn win_unit(&self) -> f32 {
        (self.screen_w / 640.0).min(self.screen_h / 480.0)
    }
}

// ---------------------------------------------------------------------------
// Cross-frame animation state
// ---------------------------------------------------------------------------

struct SliderAnim {
    follow_scale: f64,
    follow_alpha: f64,
    follow_anim: Option<(f64, f64, f64, f64, Easing)>,
    follow_alpha_anim: Option<(f64, f64, f64, f64, Easing)>,
    tick_pulse: Option<(f64, f64, f64, Easing)>, // (start, from, to, easing)
    was_tracking: bool,
    /// Legacy follow circle state (`LegacyFollowCircle`), independent of
    /// the argon values: (scale, alpha) + pending transforms.
    fc_scale: f64,
    fc_alpha: f64,
    fc_scale_anim: Option<(f64, f64, f64, f64, Easing)>,
    fc_alpha_anim: Option<(f64, f64, f64, f64, Easing)>,
    /// The tail judgement has been consumed (the exit animation fired).
    fc_tail_done: bool,
    /// Per-repeat arrow rotation, indexed by repeat index:
    /// (has rotation been set yet, smoothed Arrow.Rotation in degrees).
    /// `DrawableSliderRepeat.UpdateSnakingPosition` keeps a live rotation
    /// that eases toward the aim each frame while snaking.
    repeat_rots: Vec<(bool, f32)>,
}

impl SliderAnim {
    fn new() -> SliderAnim {
        SliderAnim {
            follow_scale: 1.0,
            follow_alpha: 0.0,
            follow_anim: None,
            follow_alpha_anim: None,
            tick_pulse: None,
            was_tracking: false,
            fc_scale: 1.0,
            fc_alpha: 0.0,
            fc_scale_anim: None,
            fc_alpha_anim: None,
            fc_tail_done: false,
            repeat_rots: Vec::new(),
        }
    }
}

struct SpinnerAnim {
    /// Display rotation (`RotationTracker.Rotation`): Damp toward the raw
    /// accumulated rotation with base 0.99 per elapsed MILLISECOND.
    display_rotation: f64,
    /// Tracking element interpolation (0.985 per elapsed ms), frozen once
    /// the spinner completes.
    tracking_lerp: f64,
    fill_scale: f32,
    ring_progress: f64,
    ring_inner: f64,
    side_alpha: f64,
    side_progress: f64,
    spm_history: Vec<(f64, f32)>,
    spm: f64,
    bonus_score: i64,
    bonus_flash: Option<(f64, bool)>,
    /// `Result.TimeCompleted`: latched the moment the required spins are met.
    complete_at: Option<f64>,
    /// Whole rotations seen so far (drives the complete fill pulse).
    whole_rotations: i64,
    /// Fill pulse start (0.4 -> 0.6 over 60ms OutExpo, back over 250ms).
    fill_pulse: Option<f64>,
    /// First moment tracking started (`Result.TimeStarted`, gates the SPM).
    spm_started: Option<f64>,
}

impl SpinnerAnim {
    fn new() -> SpinnerAnim {
        SpinnerAnim {
            display_rotation: 0.0,
            tracking_lerp: 0.0,
            fill_scale: 0.1,
            ring_progress: 0.31,
            ring_inner: 0.02,
            side_alpha: 0.0,
            side_progress: 0.0,
            spm_history: Vec::new(),
            spm: 0.0,
            bonus_score: 0,
            bonus_flash: None,
            complete_at: None,
            whole_rotations: 0,
            fill_pulse: None,
            spm_started: None,
        }
    }
}

struct TrailPart {
    pos: [f32; 2],
    time: f64,
}

pub struct Assets<'a> {
    pub atlas: &'a Atlas,
    pub bold: &'a TtfFont,
    pub semibold: &'a TtfFont,
    /// Torus Light (score counter) / Venera (rank letter).
    pub light: &'a TtfFont,
    pub venera: &'a TtfFont,
    /// Torus Regular (judgement/statistic counter values).
    pub regular: &'a TtfFont,
    /// The resolved skin (user legacy skin with argon fallback).
    pub skin: &'a skin::ResolvedSkin,
}

// ---------------------------------------------------------------------------
// Legacy skin sprite cache (resolved once per render)
// ---------------------------------------------------------------------------

/// Number sprite font: `{prefix}-{digit}` textures + the overlap setting.
struct LegacyDigits {
    prefix: String,
    digits: [Option<SkinTexture>; 10],
    overlap: f32,
}

/// Everything the legacy rendering paths need, resolved from the skin on
/// first use. Elements the skin does not provide stay `None` and the
/// argon vector visuals take over for that element (lazer's
/// `SkinnableDrawable` fallback semantics).
struct LegacyCache {
    /// `[General] Version` (`LegacySetting::Version`).
    version: f64,
    /// (circle, overlay) for hit circles; `sliderstartcircle` variants
    /// preferred for slider heads when the skin ships them
    /// (`LegacySliderHeadHitCircle`'s priority lookup).
    hitcircle: Option<(SkinTexture, Option<SkinTexture>)>,
    sliderstartcircle: Option<(SkinTexture, Option<SkinTexture>)>,
    approachcircle: Option<SkinTexture>,
    reversearrow: Option<SkinTexture>,
    /// `sliderb` frame sequence (`LegacySliderBall`: animatable, no
    /// separator) + the nd/spec layers and tint config.
    sliderball: Option<SkinAnimation>,
    sliderball_nd: Option<SkinTexture>,
    sliderball_spec: Option<SkinTexture>,
    sliderball_tint: bool,
    sliderball_colour: Colour,
    /// `LegacySliderBody.GetBorderColour`: `SliderBorder` ?? white.
    slider_border_colour: Colour,
    /// `SliderTrackOverride` (body accent), None = hit object accent.
    slider_track_colour: Option<Colour>,
    followcircle: Option<SkinAnimation>,
    scorepoint: Option<SkinTexture>,
    cursor: Option<SkinTexture>,
    cursormiddle: Option<SkinTexture>,
    /// `CursorExpand` (default true): skins may disable the press pop.
    cursor_expand: bool,
    /// `CursorCentre` (default true): anchor the sprite's centre vs its
    /// top-left corner at the cursor position.
    cursor_centre: bool,
    cursortrail: Option<SkinTexture>,
    /// Stable picks the trail style from the cursor texture's provider:
    /// disjoint (no `cursormiddle`) vs continuous additive.
    disjoint_trail: bool,
    /// `followpoint` frame sequence (`OsuLegacySkinTransformer`:
    /// `GetAnimation("followpoint", animatable, looping,
    /// applyConfigFrameRate)`; skins without a bare sprite ship only
    /// `followpoint-0..N` frames).
    followpoint: Option<SkinAnimation>,
    /// Judgement bursts (`LegacySkin.getJudgementAnimation`).
    judgement: [Option<SkinAnimation>; 4], // hit0, hit50, hit100, hit300
    /// Spinner sprites. `OsuLegacySkinTransformer.SpinnerBody`: a skin
    /// with `spinner-background` always uses the OLD style
    /// (`LegacyOldStyleSpinner`); `spinner-top` WITHOUT a background
    /// selects the NEW style (`LegacyNewStyleSpinner`). The old style
    /// never draws `spinner-glow` — that sprite belongs to the new style
    /// only.
    spinner_new_style: bool,
    spinner_glow: Option<SkinTexture>,
    spinner_background: Option<SkinTexture>,
    spinner_circle: Option<SkinTexture>,
    spinner_metre: Option<SkinTexture>,
    spinner_approachcircle: Option<SkinTexture>,
    spinner_top: Option<SkinTexture>,
    spinner_bottom: Option<SkinTexture>,
    spinner_middle: Option<SkinTexture>,
    spinner_middle2: Option<SkinTexture>,
    spinner_rpm: Option<SkinTexture>,
    spinner_spin: Option<SkinTexture>,
    spinner_clear: Option<SkinTexture>,
    spinner_background_colour: Colour,
    spinner_blink: bool,
    /// `HitCircleOverlayAboveNumber` (default true).
    overlay_above_number: bool,
    cursor_rotate: bool,
    hitcircle_digits: Option<LegacyDigits>,
    score_digits: Option<LegacyDigits>,
}

fn judgement_index(result: HitResult) -> Option<usize> {
    Some(match result {
        HitResult::Miss | HitResult::LargeTickMiss | HitResult::IgnoreMiss => 0,
        HitResult::Meh => 1,
        HitResult::Ok | HitResult::Good => 2,
        HitResult::Great | HitResult::Perfect => 3,
        _ => return None,
    })
}

impl LegacyCache {
    fn new(skin: &skin::ResolvedSkin) -> LegacyCache {
        let generic_bool = |key: &str, default: bool| {
            skin.get_config(skin_lookup_generic(key))
                .and_then(|v| v.as_bool())
                .unwrap_or(default)
        };
        let custom_colour = |key: &str| -> Option<Colour> {
            skin.get_config(skin::SkinLookup::CustomColour(skin::SkinCustomColourLookup(key.to_string())))
                .and_then(|v| v.as_colour())
        };

        let version = skin
            .get_config(skin::SkinLookup::LegacySetting(skin::configuration::LegacySetting::Version))
            .and_then(|v| v.as_f64())
            .unwrap_or(skin::configuration::LATEST_VERSION);

        // Element presence is decided on the USER skin alone (lazer's
        // OsuLegacySkinTransformer wraps only the legacy skin; a missing
        // element falls to the default skin's own components with their own
        // sizing, e.g. DefaultApproachCircle fills the 128-unit object box
        // instead of drawing a texture at its authored size).
        let ltex = |name: &str| skin.legacy_texture(name);
        let lanim = |name: &str, looping: bool, apply_rate: bool, sep: &str| {
            skin.legacy_skin()
                .and_then(|l| skin::get_animation(l, name, true, looping, apply_rate, sep))
        };

        let circle_pair = |name: &str| -> Option<(SkinTexture, Option<SkinTexture>)> {
            let circle = ltex(name)?;
            let overlay = ltex(&format!("{}overlay", name));
            Some((circle, overlay))
        };
        let hitcircle = circle_pair("hitcircle");
        let sliderstartcircle = circle_pair("sliderstartcircle");

        // LegacySliderBall: `GetTextures("sliderb", animatable: true,
        // separator: "")`; frame rate follows the slider velocity per
        // frame at draw time, so the cached 60fps base is re-scaled there.
        let sliderball = lanim("sliderb", true, false, "");
        let followcircle = lanim("sliderfollowcircle", true, true, "-");

        let judgement_names = ["hit0", "hit50", "hit100", "hit300"];
        let judgement = judgement_names.map(|name| lanim(name, false, false, "-"));

        // Stable picks the disjoint trail style based on the provider of
        // the cursor texture: no cursormiddle there => disjoint.
        let cursor = ltex("cursor");
        let cursormiddle = ltex("cursormiddle");
        let disjoint_trail = match (&cursor, &cursormiddle) {
            // A skin without any cursor sprite keeps the argon cursor;
            // its trail style is irrelevant then.
            (None, _) => false,
            (Some(_), None) => true,
            (Some(_), Some(_)) => false,
        };

        let digits = |font: skin::texture::LegacyFont| -> Option<LegacyDigits> {
            let prefix = skin::get_font_prefix(skin, font);
            let mut out: [Option<SkinTexture>; 10] = Default::default();
            let mut any = false;
            for (d, slot) in out.iter_mut().enumerate() {
                *slot = ltex(&format!("{}-{}", prefix, d));
                any |= slot.is_some();
            }
            any.then(|| LegacyDigits { prefix, digits: out, overlap: skin::get_font_overlap(skin, font) })
        };

        LegacyCache {
            version,
            hitcircle,
            sliderstartcircle,
            approachcircle: ltex("approachcircle"),
            reversearrow: ltex("reversearrow"),
            sliderball,
            sliderball_nd: ltex("sliderb-nd"),
            sliderball_spec: ltex("sliderb-spec"),
            sliderball_tint: skin
                .get_config(skin::SkinLookup::LegacySetting(
                    skin::configuration::LegacySetting::AllowSliderBallTint,
                ))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            sliderball_colour: custom_colour("SliderBall").unwrap_or(Colour::WHITE),
            slider_border_colour: custom_colour("SliderBorder").unwrap_or(Colour::WHITE),
            slider_track_colour: custom_colour("SliderTrackOverride"),
            followcircle,
            scorepoint: ltex("sliderscorepoint"),
            cursor,
            cursormiddle,
            cursor_expand: generic_bool("CursorExpand", true),
            cursor_centre: generic_bool("CursorCentre", true),
            cursortrail: ltex("cursortrail"),
            disjoint_trail,
            followpoint: lanim("followpoint", true, true, "-"),
            judgement,
            spinner_new_style: ltex("spinner-top").is_some() && ltex("spinner-background").is_none(),
            spinner_glow: ltex("spinner-glow"),
            spinner_background: ltex("spinner-background"),
            spinner_circle: ltex("spinner-circle"),
            spinner_metre: ltex("spinner-metre"),
            spinner_approachcircle: ltex("spinner-approachcircle"),
            spinner_top: ltex("spinner-top"),
            spinner_bottom: ltex("spinner-bottom"),
            spinner_middle: ltex("spinner-middle"),
            spinner_middle2: ltex("spinner-middle2"),
            spinner_rpm: ltex("spinner-rpm"),
            spinner_spin: ltex("spinner-spin"),
            spinner_clear: ltex("spinner-clear"),
            spinner_background_colour: custom_colour("SpinnerBackground")
                .unwrap_or(Colour::rgba_bytes(100, 100, 100, 255)),
            spinner_blink: !generic_bool("SpinnerNoBlink", false),
            overlay_above_number: generic_bool("HitCircleOverlayAboveNumber", true),
            cursor_rotate: generic_bool("CursorRotate", true),
            hitcircle_digits: digits(skin::texture::LegacyFont::HitCircle),
            score_digits: digits(skin::texture::LegacyFont::Score),
        }
    }
}

fn skin_lookup_generic(key: &str) -> skin::SkinLookup {
    skin::SkinLookup::Generic(key.to_string())
}

/// Horizontal anchor for [`draw_legacy_number`] (`LegacySpriteText`
/// Origin semantics).
#[derive(Clone, Copy, PartialEq)]
enum NumAlign {
    Centre,
    Right,
}

/// Draws a number with legacy digit sprites (`LegacySpriteText` layout:
/// advance = glyph width - overlap, positive overlap pulls closer).
#[allow(clippy::too_many_arguments)]
fn draw_legacy_number(
    list: &mut DrawList,
    atlas: &Atlas,
    digits: &LegacyDigits,
    text: &str,
    pos: [f32; 2],
    scale: f32,
    colour: Colour,
    blend: Blend,
    align: NumAlign,
) {
    let glyph = |c: char| digits.digits.get(c.to_digit(10).unwrap_or(0) as usize).copied().flatten();
    let widths: Vec<(f32, SkinTexture)> = text
        .chars()
        .filter_map(|c| {
            glyph(c).map(|t| (t.display_width() * scale, t))
        })
        .collect();
    if widths.is_empty() {
        return;
    }
    let total: f32 = widths
        .iter()
        .map(|&(w, _)| w - digits.overlap * scale)
        .sum::<f32>()
        + digits.overlap * scale;
    let mut pen = match align {
        NumAlign::Centre => pos[0] - total * 0.5,
        NumAlign::Right => pos[0] - total,
    };
    for (w, tex) in widths {
        let h = tex.display_height() * scale;
        list.image(
            atlas,
            tex.region,
            [pen + w * 0.5, pos[1]],
            [w, h],
            0.0,
            colour,
            blend,
        );
        pen += w - digits.overlap * scale;
    }
}

pub struct SceneState {
    pub pro_skin: bool,
    pub mapper: Mapper,
    /// Beatmap background opacity (`--bg`); None draws no background.
    pub bg_opacity: Option<f32>,
    /// A beatmap background exists in the atlas (drives the results
    /// screen's blurred background, which shows regardless of `--bg`).
    pub has_bg: bool,
    /// Storyboard below-layer (Background/Fail/Pass composite) draw
    /// opacity (`--storyboard`); None draws no storyboard. The atlas must
    /// carry `Region::Storyboard` (build-time slots) and the host must
    /// refresh it every frame via [`crate::storyboard`].
    pub storyboard: Option<f32>,
    /// The storyboard's Foreground/Overlay composite exists in the atlas
    /// (`Region::StoryboardForeground`); drawn above the playfield,
    /// under the HUD.
    pub storyboard_fg: bool,
    /// The storyboard is active: the beatmap background image must NOT
    /// draw — the storyboard (and its video) provides the backdrop.
    /// (This renderer's rule; lazer's narrower `ReplacesBackground` only
    /// hides it when the storyboard's Background layer re-declares the
    /// background file.)
    pub sb_replaces_bg: bool,
    /// A custom avatar image exists in the atlas (`--avatar` / config
    /// `avatar`); the results screen draws it instead of the initial.
    pub has_avatar: bool,
    /// 物件之间的引导线(follow points);默认开,实时预览可关。
    pub follow_points: bool,
    /// 光标尺寸倍率(lazer `OsuSetting.GameplayCursorSize`:默认 1.0,
    /// 范围 0.1..=2)。lazer 把它乘在光标容器的整体 Scale 上——argon 与
    /// legacy 皮肤光标、拖尾部件尺寸全部生效;legacy 拖尾的部件间隔
    /// 同时按 `1/max(size,1)` 稀疏化(`LegacyCursorTrail.IntervalMultiplier`,
    /// 部件变大时间距不再继续变密)。皮肤层面没有尺寸配置(skin.ini 只有
    /// CursorExpand/CursorRotate/CursorCentre 三个布尔),故默认值内置于此。
    pub cursor_size: f32,
    cursor_expand: f64,
    cursor_anim: Option<(f64, f64, f64, f64, Easing)>,
    was_pressed: bool,
    trail: Vec<TrailPart>,
    slider_anims: Vec<SliderAnim>,
    spinner_anims: Vec<SpinnerAnim>,
    pub hud: hud::HudState,
    /// Hidden mod active (from `GameData::hidden`); pub so a host can
    /// force HD visuals live on top of the replay's own mods.
    pub hidden: bool,
    /// Results-screen cutoff: at times >= this the frame draws the (static,
    /// expanded) lazer results screen instead of gameplay. `None` = never.
    pub results_at: Option<f64>,
    /// Gameplay FADE-OUT length in frames at the results handover.
    pub results_fade_frames: u32,
    /// Results screen FADE-IN length in frames (starts once the fade-out
    /// finished). 0 = appear instantly.
    pub results_fadein_frames: u32,
    /// Results frames drawn so far (drives the cross-fade).
    results_frame: u32,
    /// Resolved legacy skin sprites (lazily built on the first frame a
    /// legacy skin is active).
    legacy: Option<LegacyCache>,
    last_t: f64,
}

impl SceneState {
    pub fn new(game: &GameData, width: u32, height: u32) -> SceneState {
        SceneState {
            pro_skin: false,
            mapper: Mapper::new(width, height),
            bg_opacity: None,
            has_bg: false,
            storyboard: None,
            storyboard_fg: false,
            sb_replaces_bg: false,
            has_avatar: false,
            follow_points: true,
            cursor_size: 1.0,
            cursor_expand: 1.0,
            cursor_anim: None,
            was_pressed: false,
            trail: Vec::new(),
            slider_anims: (0..game.objects.len()).map(|_| SliderAnim::new()).collect(),
            spinner_anims: (0..game.objects.len()).map(|_| SpinnerAnim::new()).collect(),
            hud: hud::HudState::new(),
            hidden: game.hidden,
            results_at: None,
            results_fade_frames: 0,
            results_fadein_frames: 0,
            results_frame: 0,
            legacy: None,
            last_t: f64::NEG_INFINITY,
        }
    }

    pub fn build_frame(
        &mut self,
        game: &GameData,
        assets: &Assets,
        snap: &crate::game::FrameSnap,
        list: &mut DrawList,
    ) {
        let t = snap.time;

        // Sequential handover: the gameplay FADES OUT first, then the
        // results screen FADES IN over the background.
        let is_results = self.results_at.is_some_and(|ra| t >= ra);
        let (mut gameplay_alpha, mut results_alpha) = (1.0f32, 0.0f32);
        if is_results {
            let n = self.results_frame;
            self.results_frame += 1;
            let (fo, fi) = (self.results_fade_frames, self.results_fadein_frames);
            if n < fo {
                gameplay_alpha = 1.0 - n as f32 / fo as f32;
            } else if fi == 0 {
                results_alpha = 1.0;
            } else {
                results_alpha = ((n - fo) as f32 / fi as f32).min(1.0);
            }
        } else {
            self.results_frame = 0;
        }
        let prev_alpha = list.global_alpha;
        list.global_alpha = gameplay_alpha;

        let dt = if self.last_t > f64::NEG_INFINITY / 2.0 && t > self.last_t {
            (t - self.last_t).max(0.001)
        } else {
            1000.0 / 60.0
        };

        // Resolve the legacy sprite set on the first frame a legacy skin
        // is active (missing elements keep their argon fallbacks).
        if self.legacy.is_none() && assets.skin.is_legacy() {
            self.legacy = Some(LegacyCache::new(assets.skin));
        }

        // Cursor expand animation. `CursorExpand: 0` skins disable the pop
        // entirely (`OsuCursor.Expand` early-returns). Legacy cursors pop
        // to 1.3 over 100ms Out both ways (`LegacyCursor`); argon to 1.2
        // with an elastic pop.
        let pressed = snap.left || snap.right;
        let expand_enabled = self.legacy.as_ref().map(|l| l.cursor_expand).unwrap_or(true);
        if expand_enabled && pressed != self.was_pressed {
            let (target, dur, easing) = if self.legacy.is_some() {
                (if pressed { 1.3 } else { 1.0 }, 100.0, Easing::Out)
            } else if pressed {
                (1.2, 400.0, Easing::OutElasticHalf)
            } else {
                (1.0, 400.0, Easing::OutQuad)
            };
            self.cursor_anim = Some((t, t + dur, self.cursor_expand, target, easing));
            self.was_pressed = pressed;
        } else {
            self.was_pressed = pressed;
        }
        if let Some((a, b, from, to, e)) = self.cursor_anim {
            self.cursor_expand = value_at(t, a, b, from, to, e);
        }

        // Cursor trail.
        let cursor_screen = self.mapper.pf([snap.cursor.x, snap.cursor.y]);
        self.update_trail(cursor_screen, t);

        // Tracking lookup for this frame.
        let tracking: Vec<bool> = {
            let mut v = vec![false; game.objects.len()];
            for (i, tr) in &snap.sliders {
                if let Some(x) = v.get_mut(*i) {
                    *x = *tr;
                }
            }
            v
        };

        // 0. Beatmap background (`--bg`): full-screen behind everything at
        // the configured opacity (lazer's BackgroundScreen sprite fills the
        // screen; alpha = 1 - DimLevel). Skipped when the storyboard
        // replaces it (lazer `storyboardReplacesBackground`: the sb's own
        // Background-layer copy draws instead, dimming the background to 1).
        if let Some(op) = self.bg_opacity.filter(|_| !self.sb_replaces_bg) {
            let m = &self.mapper;
            list.image(
                assets.atlas,
                crate::draw::Region::Background,
                [m.screen_w * 0.5, m.screen_h * 0.5],
                [m.screen_w, m.screen_h],
                0.0,
                Colour::WHITE.opacity(op),
                Blend::Alpha,
            );
        }

        // 0.5 Storyboard below-layers (Background/Fail/Pass composite,
        // `--storyboard`): over the background image, under everything
        // else. osu! dims the storyboard together with the background
        // (DimLevel), hence the same opacity slot.
        if let Some(op) = self.storyboard {
            let m = &self.mapper;
            list.image(
                assets.atlas,
                crate::draw::Region::Storyboard,
                [m.screen_w * 0.5, m.screen_h * 0.5],
                [m.screen_w, m.screen_h],
                0.0,
                Colour::WHITE.opacity(op),
                Blend::Alpha,
            );
        }

        // 1. Spinners.
        for i in 0..game.objects.len() {
            if game.objects[i].kind == ObjKind::Spinner {
                self.draw_spinner(game, assets, list, i, t, dt, snap);
            }
        }

        // 2. Follow points.
        if self.follow_points {
            draw_follow_points(self.legacy.as_ref(), game, &self.mapper, list, assets.atlas, t);
        }

        // 3. Judgement explosions (under objects) - argon only (legacy
        // skins have no per-hit particle rings).
        if self.legacy.is_none() {
            for ev in &game.events {
                if self.pro_skin
                    && matches!(ev.result, osu_replay_judge::score::HitResult::Great | osu_replay_judge::score::HitResult::Perfect)
                {
                    continue;
                }
                draw_judgement_explosion(list, &self.mapper, ev, t);
            }
        }

        // 4. Hit objects, reverse start-time order (earlier on top).
        for idx in (0..game.objects.len()).rev() {
            match game.objects[idx].kind {
                ObjKind::Circle => self.draw_circle(assets, list, &game.objects[idx], t),
                ObjKind::Slider => {
                    self.draw_slider(assets, list, &game.objects[idx], tracking[idx], t, dt);
                }
                ObjKind::Spinner => {}
            }
        }

        // 5. Judgement text (legacy: skin burst sprites).
        for ev in &game.events {
            if self.pro_skin
                && matches!(ev.result, osu_replay_judge::score::HitResult::Great | osu_replay_judge::score::HitResult::Perfect)
            {
                continue;
            }
            draw_judgement_text(self.legacy.as_ref(), assets, list, &self.mapper, ev, t);
        }

        // 6. Approach circles.
        if std::env::var("NO_APPROACH").is_err() {
            for obj in &game.objects {
                // Spinners have no approach circle in lazer: neither
                // `DrawableSpinner` nor any Argon spinner component defines
                // one (only legacy skins draw `spinner-approachcircle`).
                if obj.kind == ObjKind::Spinner {
                    continue;
                }
                // HD hides approach circles everywhere except the first
                // adjustable object (`IncreaseFirstObjectVisibility`,
                // default on; spinners never qualify as the first object).
                if game.hidden && obj.index != game.hd_first_object {
                    continue;
                }
                draw_approach_circle(self.legacy.as_ref(), assets, list, &self.mapper, obj, t, game.hidden);
            }
        }

        // 7. Cursor + trail.
        self.draw_trail(list, assets);
        draw_cursor(self.legacy.as_ref(), assets, list, cursor_screen, self.cursor_expand as f32, self.cursor_size, self.mapper.virt, t);

        // 7.5 Storyboard above-layers (Foreground/Overlay composite):
        // over the playfield like osu!, under the HUD, undimmed (lazer's
        // DimLevel only affects the background stack).
        if self.storyboard_fg {
            let m = &self.mapper;
            list.image(
                assets.atlas,
                crate::draw::Region::StoryboardForeground,
                [m.screen_w * 0.5, m.screen_h * 0.5],
                [m.screen_w, m.screen_h],
                0.0,
                Colour::WHITE,
                Blend::Alpha,
            );
        }

        // 8. HUD.结算阶段隐藏(不随 gameplay 淡出):分数/血条等留在
        // 屏幕上会和结算页互相打架,进结算即消失。
        if !is_results {
            self.hud.draw(game, assets, list, &self.mapper, t);
        }

        // 9. Results screen: fades in once the gameplay has fully faded
        // out (sequential, not a cross-fade).
        list.global_alpha = prev_alpha;
        if results_alpha > 0.0 {
            let view = crate::results::ResultsView {
                game,
                classic_score: self.hud.is_classic_score(),
                has_bg: self.has_bg,
                has_avatar: self.has_avatar,
            };
            list.global_alpha = results_alpha;
            crate::results::draw(&view, assets, &self.mapper, list);
            list.global_alpha = prev_alpha;
        }

        self.last_t = t;
    }

    fn update_trail(&mut self, cursor: [f32; 2], t: f64) {
        // 时间不连续(实时预览的 seek 跳变,含回退):拖尾是光标运动的
        // 时间连续性产物,跳变时旧轨迹整体作废且不做插值。否则回退后
        // 旧点年龄为负、永不 retire 且 draw 端 clamp 成全亮;前进大跳
        // 会在相距很远的两位置间插出一条贯穿屏幕的长尾,越拖越多。
        let jumped = self
            .trail
            .last()
            .map_or(true, |p| (t - p.time).abs() > TRAIL_DURATION);
        if jumped {
            self.trail.clear();
        }
        // Part spacing follows the cursor size like the interpolated adds
        // of `CursorTrail.AddTrail` (interval ∝ `CursorScale`), with the
        // legacy `IntervalMultiplier = 1/max(size,1)` clamping it above 1
        // (bigger parts don't also densify).
        let spacing = match self.legacy.as_ref().map(|l| l.cursortrail).flatten() {
            Some(_) => TRAIL_SPACING * self.cursor_size / self.cursor_size.max(1.0),
            None => TRAIL_SPACING * self.cursor_size,
        };
        let last_pos = if jumped { None } else { self.trail.last().map(|p| p.pos) };
        if let Some(last) = last_pos {
            let dx = cursor[0] - last[0];
            let dy = cursor[1] - last[1];
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > spacing {
                let steps = ((dist / spacing) as usize).min(600);
                for s in 1..=steps {
                    let f = s as f32 / steps as f32;
                    self.trail.push(TrailPart {
                        pos: [last[0] + dx * f, last[1] + dy * f],
                        time: t,
                    });
                }
            } else if dist > 0.0 {
                self.trail.push(TrailPart { pos: cursor, time: t });
            }
        } else {
            self.trail.push(TrailPart { pos: cursor, time: t });
        }
        self.trail.retain(|p| (t - p.time).abs() < TRAIL_DURATION);
    }

    fn draw_trail(&mut self, list: &mut DrawList, assets: &Assets) {
        let expand = self.cursor_expand as f32;
        let t = self.last_t;

        // Legacy trail (`LegacyCursorTrail`): sprite trail; disjoint skins
        // (no cursormiddle) drop one part every 60ms with a 150ms linear
        // fade, continuous skins interpolate along the movement with an
        // additive 500ms fade.
        if let Some(lg) = &self.legacy
            && let Some(tex) = lg.cursortrail
        {
            let fade_duration = if lg.disjoint_trail { 150.0 } else { 500.0 };
            let mut last_time = f64::NEG_INFINITY;
            for p in &self.trail {
                let disjoint_part = lg.disjoint_trail && last_time >= p.time - 1e-9;
                last_time = p.time;
                if disjoint_part {
                    continue;
                }
                let age = (t - p.time).clamp(0.0, fade_duration) / fade_duration;
                let alpha = 1.0 - age as f32;
                if alpha <= 0.004 {
                    continue;
                }
                // `Texture.ScaleAdjust *= STABLE_MAGIC_SCALE_FACTOR` on
                // load: like the cursor, the 1.6 cancels against the
                // playfield scale, so the sprite shows at its display
                // size in WINDOW units. `TrailOrigin` follows
                // `CursorCentre` (centre vs top-left anchor). Part size
                // carries the user cursor scale (`CursorTrail`'s
                // `cursorScale` uniform, fed `ActiveCursor.CursorScale`).
                let w = tex.display_width() * self.mapper.virt * self.cursor_size;
                let h = tex.display_height() * self.mapper.virt * self.cursor_size;
                let at = if lg.cursor_centre {
                    p.pos
                } else {
                    [p.pos[0] + w * 0.5, p.pos[1] + h * 0.5]
                };
                list.image(
                    assets.atlas,
                    tex.region,
                    at,
                    [w, h],
                    0.0,
                    Colour::WHITE.opacity(alpha),
                    if lg.disjoint_trail { Blend::Alpha } else { Blend::Additive },
                );
            }
            return;
        }

        for p in &self.trail {
            let age = (t - p.time).clamp(0.0, TRAIL_DURATION) / TRAIL_DURATION;
            let alpha = (1.0 - age as f32).powf(TRAIL_FADE_EXPONENT) * TRAIL_ALPHA;
            if alpha > 0.004 {
                list.glow(p.pos, TRAIL_SIZE * expand * self.cursor_size, Colour::WHITE.opacity(alpha));
            }
        }
    }

    // -------------------------------------------------------------------
    // Circles
    // -------------------------------------------------------------------

    fn draw_circle(&mut self, assets: &Assets, list: &mut DrawList, obj: &ObjView, t: f64) {
        let appear = obj.start_time - obj.preempt;
        if t < appear {
            return;
        }

        let (judged, hit, ht) = match obj.body_judged {
            Some((time, result)) => (true, hit_result_ext::is_hit(result), time),
            None => (false, false, 0.0),
        };
        if judged && t > ht + 800.0 {
            // `DrawableHitCircle.UpdateHitStateTransforms`: Delay(800).FadeOut()
            // - the circle disappears instantly 800ms after its hit time.
            return;
        }

        // HD overrides the fade-in to 0.4 * preempt and fades the circle
        // out over the last stretch of the preempt (fully invisible 30% of
        // the preempt before the hit time); hit-state animations stack on
        // top as usual.
        let base_alpha = if self.hidden {
            value_at(t, appear, appear + hd_fade_in(obj), 0.0, 1.0, Easing::Linear)
        } else {
            value_at(t, appear, appear + obj.fade_in, 0.0, 1.0, Easing::Linear)
        };
        // `ArgonMainCirclePiece` hit state: the WHOLE piece (all fills,
        // number, flash and the border ring included) runs `this.FadeOut`
        // on top of the per-layer animations. The colour block (fills +
        // flash) always disappears well before the ring: fills hide over
        // 150ms, the flash pops in/out over 300ms, while the piece fade
        // runs 800 * 0.8 = 640ms OutQuad - the ring lingers longest.
        // On miss the circle fades over 100ms. Legacy pieces fade their
        // sprites over 240ms Out with a 1.4x pop instead.
        let mut overall = if judged && hit {
            if self.legacy.is_some() {
                value_at(t, ht, ht + 240.0, 1.0, 0.0, Easing::Linear)
            } else {
                value_at(t, ht, ht + 640.0, 1.0, 0.0, Easing::OutQuad)
            }
        } else if judged {
            value_at(t, ht, ht + 100.0, 1.0, 0.0, Easing::Linear)
        } else {
            1.0
        };
        if self.hidden {
            overall *= hd_fade_out(obj.preempt, obj.start_time, hd_fade_in(obj), t);
        }
        let alpha = base_alpha * overall;
        if alpha <= 0.003 {
            return;
        }

        draw_circle_piece(
            self.legacy.as_ref(),
            assets,
            list,
            &self.mapper,
            obj,
            obj.colour,
            alpha as f32,
            obj.number,
            true,
            judged,
            hit,
            ht,
            t,
        );
    }

    // -------------------------------------------------------------------
    // Sliders
    // -------------------------------------------------------------------

    fn draw_slider(
        &mut self,
        assets: &Assets,
        list: &mut DrawList,
        obj: &ObjView,
        tracking: bool,
        t: f64,
        dt: f64,
    ) {
        let appear = obj.start_time - obj.preempt;
        if t < appear {
            return;
        }
        let (body_judged, body_hit, bt) = match obj.body_judged {
            Some((time, result)) => (true, hit_result_ext::is_hit(result), time),
            None => (false, false, 0.0),
        };
        if body_judged && t > bt + 400.0 {
            return;
        }

        let head = obj.head_judged.map(|(time, r)| (time, hit_result_ext::is_hit(r)));
        let head_hit = head.map(|h| h.1).unwrap_or(false);

        let mut alpha = value_at(t, appear, appear + obj.fade_in, 0.0, 1.0, Easing::Linear);
        if body_judged {
            alpha *= value_at(t, bt, bt + 240.0, 1.0, 0.0, Easing::Linear);
        }
        if alpha <= 0.003 {
            return;
        }

        let m = &self.mapper;
        let s = obj.scale * m.pf;

        // --- Follow circle animation state ---------------------------------
        let anim = &mut self.slider_anims[obj.index];

        // Tick pulse: ticks hit while tracking.
        for n in &obj.nested {
            if n.kind == NestedKind::Tick {
                if let Some((jt, r)) = n.judged {
                    if hit_result_ext::is_hit(r) && self.last_t < jt && jt <= t {
                        if anim.follow_scale >= FOLLOW_AREA as f64 * 0.98 {
                            anim.tick_pulse =
                                Some((t, anim.follow_scale, FOLLOW_AREA as f64 * 1.08, Easing::OutQuint));
                        }
                        // `LegacyFollowCircle.OnSliderTick`: at full
                        // extension (>= 2x) pulse 2.2 -> 2 over 200ms.
                        if self.legacy.is_some() && anim.fc_scale >= 2.0 {
                            anim.fc_scale_anim = Some((t, t + 200.0, 2.2, 2.0, Easing::Linear));
                        }
                    }
                }
            }
        }

        // Tracking transitions.
        let slider_ended = t >= obj.end_time;
        if tracking && !anim.was_tracking {
            // OnSliderPress.
            if anim.follow_alpha.abs() < 1e-3 {
                anim.follow_scale = 1.0;
            }
            anim.follow_anim = Some((t, t + 300.0, anim.follow_scale, FOLLOW_AREA as f64, Easing::OutQuint));
            anim.follow_alpha_anim = Some((t, t + 300.0, anim.follow_alpha, 1.0, Easing::OutQuint));
        } else if !tracking && anim.was_tracking {
            if slider_ended {
                // OnSliderEnd.
                anim.follow_anim = Some((t, t + 300.0, anim.follow_scale, 1.0, Easing::OutQuint));
                anim.follow_alpha_anim = Some((t, t + 150.0, anim.follow_alpha, 0.0, Easing::OutQuint));
            } else {
                // OnSliderRelease.
                anim.follow_anim =
                    Some((t, t + 150.0, anim.follow_scale, FOLLOW_AREA as f64 * 1.2, Easing::OutQuint));
                anim.follow_alpha_anim = Some((t, t + 150.0, anim.follow_alpha, 0.0, Easing::OutQuint));
            }
        }

        // Legacy follow circle state (`LegacyFollowCircle`): press pops
        // to 2x over min(180, remaining) Out with the alpha ramping over
        // min(60, remaining); a mid-slider break flashes to 4x and dies
        // over 100ms; the natural end settles at 1.6x Out / 200ms In.
        if self.legacy.is_some() {
            let remaining = (obj.end_time - t).max(0.0);
            if tracking && !anim.was_tracking {
                // `OnSliderPress` starts with INSTANT `ScaleTo(1f)` /
                // `FadeTo(0)`: a re-track after a mid-slider break resets
                // the half-finished 4x break transform instead of
                // animating down from it (which showed the ring rushing
                // in oversized and shrinking to size).
                anim.fc_scale = 1.0;
                anim.fc_alpha = 0.0;
                anim.fc_tail_done = false;
                anim.fc_scale_anim = Some((t, t + remaining.min(180.0), 1.0, 2.0, Easing::Out));
                anim.fc_alpha_anim = Some((t, t + remaining.min(60.0), 0.0, 1.0, Easing::Linear));
            }
            // The exit fires at the slider end, and the slider's OWN result
            // picks the animation (`FollowCircle.updateStateTransforms` in
            // lazer uses the tail's ArmedState: Hit -> OnSliderEnd, Miss ->
            // OnSliderBreak). The classic timeline has no separate tail
            // judgement - the tail nested entry stays unjudged and the
            // whole-slider result lands on the body as IgnoreHit/IgnoreMiss
            // (completed/released-early) or Miss (dropped entirely) - so a
            // real Miss picks OnSliderBreak and anything else
            // OnSliderEnd (settle 1.6x Out / 200ms In vs flash 4x / 100ms).
            if slider_ended && !anim.fc_tail_done {
                anim.fc_tail_done = true;
                let body_miss =
                    matches!(obj.body_judged, Some((_, r)) if !hit_result_ext::is_hit(r));
                if body_miss {
                    anim.fc_scale_anim = Some((t, t + 100.0, anim.fc_scale, 4.0, Easing::Linear));
                    anim.fc_alpha_anim = Some((t, t + 100.0, anim.fc_alpha, 0.0, Easing::Linear));
                } else {
                    anim.fc_scale_anim = Some((t, t + 200.0, anim.fc_scale, 1.6, Easing::Out));
                    anim.fc_alpha_anim = Some((t, t + 200.0, anim.fc_alpha, 0.0, Easing::In));
                }
            }
            if let Some((a, b, from, to, e)) = anim.fc_scale_anim {
                anim.fc_scale = value_at(t, a, b, from, to, e);
            }
            if let Some((a, b, from, to, e)) = anim.fc_alpha_anim {
                anim.fc_alpha = value_at(t, a, b, from, to, e);
            }
        }
        anim.was_tracking = tracking;

        if let Some((a, b, from, to, e)) = anim.follow_anim {
            anim.follow_scale = value_at(t, a, b, from, to, e);
        }
        if let Some((a, b, from, to, e)) = anim.follow_alpha_anim {
            anim.follow_alpha = value_at(t, a, b, from, to, e);
        }
        // Tick pulse overrides briefly (40ms up then 200ms back).
        if let Some((start, from, to, e)) = anim.tick_pulse {
            if t < start + 40.0 {
                anim.follow_scale = value_at(t, start, start + 40.0, from, to, e);
            } else if t < start + 240.0 {
                anim.follow_scale =
                    value_at(t, start + 40.0, start + 240.0, to, FOLLOW_AREA as f64, Easing::OutQuint);
            } else {
                anim.tick_pulse = None;
                anim.follow_scale = FOLLOW_AREA as f64;
            }
        }

        let follow_scale_now = anim.follow_scale;
        let follow_alpha_now = anim.follow_alpha;
        let fc_scale_now = anim.fc_scale;
        let fc_alpha_now = anim.fc_alpha;
        let anim = &self.slider_anims[obj.index];
        let _ = anim;

        // --- Snaking range ---------------------------------------------------
        // `Slider.ProgressAt` = the ball's PATH progress (mirrored on odd
        // spans), which is what SnakingSliderBody uses - NOT the time
        // fraction.
        let completion = ((t - obj.start_time) / obj.duration.max(1e-9)).clamp(0.0, 1.0);
        let body_completion = if head_hit { completion } else { 0.0 };
        let raw = body_completion * obj.span_count as f64;
        let span = (raw as usize).min(obj.span_count.saturating_sub(1));
        // Fraction within the (clamped) span; at completion = 1 this is 1.0,
        // so the last odd span fully retracts instead of snapping back full.
        let frac = (raw - span as f64).clamp(0.0, 1.0);
        let path_progress = if span % 2 == 1 { 1.0 - frac } else { frac };

        let mut p0 = 0.0f64;
        let mut p1 = ((t - appear) / (obj.preempt / 3.0)).clamp(0.0, 1.0); // snaking in

        if span >= obj.span_count.saturating_sub(1) {
            if span % 2 == 1 {
                // Returning on the last span: the body retracts from the far
                // end behind the ball.
                p0 = 0.0;
                p1 = path_progress;
            } else {
                // Forward on the last span: the body retracts from the start
                // behind the ball.
                p0 = path_progress;
                p1 = 1.0;
            }
        }

        // --- Body -------------------------------------------------------------
        // `PlaySliderBody`: `PathRadius = OsuHitObject.OBJECT_RADIUS * scale`
        // for every skin (the `SliderPathRadius` legacy lookup only feeds
        // the vector slider ball). Argon narrows the visual body to its
        // OUTER_GRADIENT_SIZE design; legacy bodies are the full radius.
        let legacy = self.legacy.as_ref();
        let path_radius = match legacy {
            Some(_) => 64.0, // OsuHitObject.OBJECT_RADIUS
            None => OUTER_GRADIENT_SIZE / 2.0,
        } * obj.scale;
        let sub = obj.sub_path(p0, p1);
        if sub.len() >= 2 {
            let screen_pts: Vec<[f32; 2]> = sub.iter().map(|&p| m.pf(p)).collect();
            // ArgonSliderBody: the whole path fades with the slider; the
            // BORDER colour is the plain accent while only the body accent
            // gets BodyAlpha (0.92 pro / 0.98 normal, applied to
            // AccentColour.Darken(4) = the accent at 20% - a dark TINT,
            // not black).
            // LegacySliderBody: border = `SliderBorder` ?? white, body =
            // `SliderTrackOverride` ?? accent at 0.7 alpha (a constant
            // track alpha regardless of the source colour), border band
            // `border_portion` (0.1875) of the radius. (The stable
            // gradient's inner shadow core is approximated by the flat
            // two-band SDF body.)
            let mut fade = alpha as f32;
            // HD slider body: a LONG fade spanning from the default fade-in
            // end to the slider end (`Easing.Out` = OutQuad) - the track
            // gradually vanishes while the ball/follow circle stay visible.
            if self.hidden {
                fade *= value_at(
                    t,
                    appear + obj.fade_in,
                    obj.end_time,
                    1.0,
                    0.0,
                    Easing::OutQuad,
                ) as f32;
            }
            if body_judged && body_hit && head_hit && t > bt {
                fade *= value_at(t, bt, bt + 40.0, 1.0, 0.0, Easing::Linear) as f32;
            }
            let cap_r = path_radius * m.pf;
            let (cap_border, border_col, body_col, inner_col) = match legacy {
                Some(lg) => {
                    // `LegacyDrawableSliderPath.ColourAt`: from the edge
                    // inward - transparent-black rim over `[0, 0.078]`,
                    // the border colour over `(0.078, 0.1875]`, then the
                    // sRGB lerp `accent.Darken(0.1) -> lighten(accent,
                    // 0.5)` carrying the 0.7 track alpha. The composite
                    // shader evaluates the full gradient per fragment;
                    // `border` (the flat-mode band width) stays for the
                    // join discs only.
                    let accent = lg.slider_track_colour.unwrap_or(obj.colour).opacity(0.7 * fade);
                    (
                        (LEGACY_SLIDER_BORDER_PORTION - LEGACY_SLIDER_SHADOW_PORTION) * cap_r,
                        lg.slider_border_colour.opacity(fade),
                        accent.darken(0.1),
                        Some(accent.lighten(0.5)),
                    )
                }
                None => {
                    let body_alpha = if self.pro_skin { 0.92 } else { 0.98 };
                    (
                        GRADIENT_THICKNESS * obj.scale * m.pf,
                        obj.colour.opacity(fade),
                        obj.colour.darken(4.0).opacity(fade * body_alpha),
                        None,
                    )
                }
            };
            let mut segments = Vec::with_capacity(screen_pts.len() - 1);
            for i in 0..screen_pts.len() - 1 {
                segments.push((screen_pts[i], screen_pts[i + 1]));
            }
            list.bodies.push(crate::draw::BodyDraw {
                segments,
                radius: cap_r,
                border: cap_border,
                body: body_col,
                border_colour: border_col,
                inner_colour: inner_col,
            });
            // Layer anchor: in lazer the body is part of the DrawableSlider,
            // so an earlier slider's body covers later objects (and its own
            // head/ball draw over it).
            list.mark_body();
        }

        // --- Ticks ---------------------------------------------------------------
        for n in &obj.nested {
            if n.kind != NestedKind::Tick {
                continue;
            }
            draw_slider_tick(
                self.legacy.as_ref(),
                assets,
                list,
                m,
                obj,
                n.position,
                n.time,
                n.span_index,
                n.judged,
                t,
                self.hidden,
            );
        }

        // --- Repeat arrows ---------------------------------------------------------
        // `DrawableSliderRepeat.UpdateSnakingPosition`: the arrow rides the
        // snaked body tip (end repeats at SnakedEnd, head repeats at
        // SnakedStart) and aims along the current curve; hit repeats freeze
        // on the spot, missed ones keep following the retracting body.
        {
            let span_duration = obj.duration / obj.span_count.max(1) as f64;
            let anim = &mut self.slider_anims[obj.index];
            let mut ri = 0usize;
            for n in &obj.nested {
                if n.kind != NestedKind::Repeat {
                    continue;
                }
                let at_end = n.path_progress >= 0.999;
                let frozen = matches!(n.judged, Some((jt, r)) if jt <= t && hit_result_ext::is_hit(r));
                let (pos, aim) = repeat_anchor(obj, p0, p1, at_end, frozen);
                let anim_start = if ri == 0 {
                    obj.start_time - obj.preempt
                } else {
                    n.time - 2.0 * span_duration
                };
                if anim.repeat_rots.len() <= ri {
                    anim.repeat_rots.resize(ri + 1, (false, 0.0));
                }
                let st = &mut anim.repeat_rots[ri];
                if t >= anim_start && !frozen {
                    let mut a = aim as f64;
                    // Unwrap the aim towards the current rotation before easing.
                    while (a - st.1 as f64).abs() > 180.0 {
                        a += if a < st.1 as f64 { 360.0 } else { -360.0 };
                    }
                    st.1 = if st.0 {
                        value_at(
                            dt.clamp(0.0, 100.0),
                            0.0,
                            50.0,
                            st.1 as f64,
                            a,
                            Easing::OutQuint,
                        ) as f32
                    } else {
                        a as f32
                    };
                    st.0 = true;
                }
                draw_repeat_arrow(self.legacy.as_ref(), assets, list, m, obj, pos, n.time, n.judged, t, st.1);
                ri += 1;
            }
        }

        // --- Head circle ---------------------------------------------------------------
        let (h_judged, h_hit, h_time) = match head {
            Some((time, hit)) => (true, hit, time),
            None => (false, false, 0.0),
        };
        let head_alpha = if h_judged {
            if h_hit {
                // Same ArgonMainCirclePiece hit fade as circles: the whole
                // piece (border included) fades out over 800 * 0.8 = 640ms,
                // OutQuad - the colour block (fills/flash) is long gone by
                // then, so the ring lingers longest.
                value_at(t, h_time, h_time + 640.0, 1.0, 0.0, Easing::OutQuad)
            } else {
                value_at(t, h_time, h_time + 100.0, 1.0, 0.0, Easing::Linear)
            }
        } else {
            1.0
        };
        // HD: the head circle is a nested non-slider - its own fade-in is
        // 0.4 * preempt (faster than the body's default) and it rides the
        // circle fade-out window anchored at the head. The slider `alpha`
        // (default fade-in) keeps applying to the body/ball only.
        let head_base = if self.hidden {
            let head_fade_in = obj.preempt * HD_FADE_IN_MULTIPLIER;
            value_at(t, appear, appear + head_fade_in, 0.0, 1.0, Easing::Linear)
                * hd_fade_out(obj.preempt, obj.start_time, head_fade_in, t)
        } else {
            alpha
        };
        let combined = head_base * head_alpha;
        if combined > 0.003 {
            draw_circle_piece(
                self.legacy.as_ref(),
                assets,
                list,
                m,
                obj,
                obj.colour,
                combined as f32,
                obj.number,
                false,
                h_judged,
                h_hit,
                h_time,
                t,
            );
        }

        // --- Ball + follow circle --------------------------------------------------------
        if t >= obj.start_time && !(body_judged && t > bt + 240.0) {
            // ArgonSliderBall: FadeInFromZero(200, OutQuint) at the slider
            // start; at the end it intentionally piles an EXTRA
            // FadeOut(duration / 4 = 50ms, OutQuint) on top of the whole
            // slider's 240ms fade - the ball vanishes much faster than the
            // body ("intentionally pile on an extra FadeOut to make it
            // happen much faster").
            let mut ball_alpha =
                value_at(t, obj.start_time, obj.start_time + 200.0, 0.0, 1.0, Easing::OutQuint) * alpha;
            if body_judged {
                // The end fade: ArgonSliderBall/DefaultSliderBall pile an
                // extra FadeOut(duration/4 = 50ms, OutQuint); the legacy
                // ball has no such override - it just rides the slider's
                // own fade, and `LegacySliderBall.updateStateTransforms`
                // hides it INSTANTLY at the tail judgement.
                if legacy.is_some() {
                    ball_alpha = 0.0;
                } else {
                    ball_alpha *= value_at(t, bt, bt + 50.0, 1.0, 0.0, Easing::OutQuint);
                }
            }
            let ball_pos_screen = m.pf(obj.slider_ball_at(completion));
            let ball_r = OUTER_GRADIENT_SIZE * 0.5 * obj.scale * m.pf;

            if let Some(lg) = legacy {
                // LegacyFollowCircle: the sprite renders at half its
                // authored size inside the ball area (ctor `Scale *= 0.5`)
                // and animates its own scale/alpha.
                if fc_alpha_now > 0.003
                    && let Some(fc) = &lg.followcircle
                    && let Some(frame) = fc.frames.first().copied()
                {
                    let frame = if fc.frames.len() > 1 {
                        fc.frame_at(t - obj.start_time)
                    } else {
                        frame
                    };
                    let w = frame.display_width() * s * 0.5 * fc_scale_now as f32;
                    let h = frame.display_height() * s * 0.5 * fc_scale_now as f32;
                    list.image(
                        assets.atlas,
                        frame.region,
                        ball_pos_screen,
                        [w, h],
                        0.0,
                        Colour::WHITE.opacity(fc_alpha_now as f32),
                        Blend::Alpha,
                    );
                }
            } else if follow_alpha_now > 0.003 {
                // Follow circle (under the ball): border 4, additive.
                let fr = 64.0 * obj.scale * follow_scale_now as f32 * m.pf;
                let fcol = obj.colour;
                list.disc(
                    ball_pos_screen,
                    fr,
                    fcol.opacity(0.3 * follow_alpha_now as f32),
                    fcol.darken(0.5).opacity(0.3 * follow_alpha_now as f32),
                    Blend::Additive,
                );
                list.ring(
                    ball_pos_screen,
                    fr,
                    4.0 * s,
                    fcol.opacity(follow_alpha_now as f32),
                    fcol.darken(0.5).opacity(follow_alpha_now as f32),
                    Blend::Additive,
                );
            }

            if ball_alpha > 0.003 {
                let rot = ball_rotation(obj, completion);
                if let Some(lg) = legacy && let Some(ball) = &lg.sliderball {
                    // LegacySliderBall: `sliderb` frame sequence advanced
                    // by the slider velocity (`frameDelay = max(0.15 /
                    // velocity * 60fps, 60fps)`), tinted with the combo
                    // colour when `AllowSliderBallTint`; the `sliderb-nd`
                    // (darkened 5,5,5) and `sliderb-spec` (additive) layers
                    // counter-rotate to stay axis-aligned.
                    let span_duration = obj.duration / obj.span_count.max(1) as f64;
                    let velocity = if span_duration > 0.0 { obj.slider_distance / span_duration } else { 0.0 };
                    let frame_delay = if velocity > 0.0 {
                        (0.15 / velocity * SIXTY_FRAME_TIME).max(SIXTY_FRAME_TIME)
                    } else {
                        SIXTY_FRAME_TIME
                    };
                    let playhead = ((t - obj.start_time).max(0.0) / frame_delay) as usize;
                    let frame = ball.frames[playhead % ball.frames.len()];
                    let tint = if lg.sliderball_tint {
                        obj.colour
                    } else {
                        lg.sliderball_colour
                    };
                    let draw_layer = |list: &mut DrawList, tex: SkinTexture, colour: Colour, rotation: f32, blend: Blend| {
                        let w = tex.display_width() * s;
                        let h = tex.display_height() * s;
                        list.image(assets.atlas, tex.region, ball_pos_screen, [w, h], rotation, colour.opacity(ball_alpha as f32), blend);
                    };
                    if let Some(nd) = lg.sliderball_nd {
                        draw_layer(list, nd, Colour::rgba_bytes(5, 5, 5, 255), -rot, Blend::Alpha);
                    }
                    draw_layer(list, frame, tint, rot, Blend::Alpha);
                    if let Some(spec) = lg.sliderball_spec {
                        draw_layer(list, spec, Colour::WHITE, -rot, Blend::Additive);
                    }
                } else {
                    // ArgonSliderBall: circular container OUTER_GRADIENT_SIZE,
                    // white border of GRADIENT_THICKNESS, gradient fill
                    // accent -> accent.darken(0.5), and a FontAwesome
                    // AngleRight icon (Size 48, Scale (0.6, 0.8)) in white,
                    // rotated with the ball container to point along the
                    // direction of travel (`ball.Rotation` in
                    // DrawableSliderBall.UpdateProgress - no extra 180).
                    // The fa-angle-right glyph is 45-degree arms; the (0.6, 0.8)
                    // scale skews them to ~53 degrees from the axis (106 degree
                    // opening), ink ~11.3 x 24.2 in a 14.4 x 38.4 box, stroke
                    // ~5.3 (128-space units).
                    let (sr, cr) = rot.to_radians().sin_cos();
                    // Fill.
                    list.disc(
                        ball_pos_screen,
                        ball_r,
                        obj.colour.opacity(ball_alpha as f32),
                        obj.colour.darken(0.5).opacity(ball_alpha as f32),
                        Blend::Alpha,
                    );
                    // White border ring.
                    list.ring(
                        ball_pos_screen,
                        ball_r,
                        GRADIENT_THICKNESS * obj.scale * m.pf,
                        Colour::WHITE.opacity(ball_alpha as f32),
                        Colour::WHITE.opacity(ball_alpha as f32),
                        Blend::Alpha,
                    );
                    // Icon: two round-capped strokes forming the angle bracket.
                    // On slider end the icon also shrinks to 0.9x over 200ms
                    // OutQuint (`icon.ScaleTo(defaultIconScale * 0.9, ...)`).
                    let icon_scale = if body_judged {
                        value_at(t, bt, bt + 200.0, 1.0, 0.9, Easing::OutQuint) as f32
                    } else {
                        1.0
                    };
                    let unit = obj.scale * m.pf * icon_scale;
                    let thickness = 2.65 * unit;
                    let tip = [ball_pos_screen[0] + cr * 5.5 * unit, ball_pos_screen[1] + sr * 5.5 * unit];
                    let back = -5.0 * unit;
                    let open = 11.5 * unit;
                    let top = [ball_pos_screen[0] + cr * back + sr * -open, ball_pos_screen[1] + sr * back + cr * open];
                    let bottom = [ball_pos_screen[0] + cr * back + sr * open, ball_pos_screen[1] + sr * back - cr * open];
                    list.capsule(top, tip, thickness, Colour::WHITE.opacity(ball_alpha as f32), Blend::Alpha);
                    list.capsule(tip, bottom, thickness, Colour::WHITE.opacity(ball_alpha as f32), Blend::Alpha);
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Spinner
    // -------------------------------------------------------------------

    fn draw_spinner(
        &mut self,
        game: &GameData,
        assets: &Assets,
        list: &mut DrawList,
        idx: usize,
        t: f64,
        dt: f64,
        snap: &crate::game::FrameSnap,
    ) {
        let obj = &game.objects[idx];
        let appear = obj.start_time - obj.preempt;
        if t < appear {
            return;
        }
        let (judged, hit, jt) = match obj.body_judged {
            Some((time, r)) => (true, hit_result_ext::is_hit(r), time),
            None => (false, false, 0.0),
        };
        if judged && t > jt + 400.0 {
            return;
        }

        // Find this spinner's frame data in the (possibly synthesized)
        // snapshot.
        let frame = snap
            .spinners
            .iter()
            .find(|sp| sp.object_index == idx)
            .copied();
        let frame = match frame {
            Some(f) => f,
            None => return,
        };

        let anim = &mut self.spinner_anims[idx];
        let m = &self.mapper;
        let centre = m.pf([256.0, 192.0]);
        let unit = SPINNER_DISC * m.pf;
        let s = m.pf;

        // --- Damped states (ArgonSpinnerDisc / RotationTracker Update) ---
        let progress = if obj.spins_required <= 0.0 {
            1.0
        } else {
            ((frame.total_rotation / 360.0) as f64 / obj.spins_required).clamp(0.0, 1.0)
        };

        // `Result.TimeCompleted` latches the moment progress reaches 1;
        // the fill pulse fires once per NEW whole rotation afterwards.
        if progress >= 1.0 && anim.complete_at.is_none() {
            anim.complete_at = Some(t);
        }
        let complete = anim.complete_at.is_some_and(|ct| t >= ct);

        // Display rotation: `Interpolation.Damp(Rotation, currentRotation,
        // 0.99, |Time.Elapsed|)` - the framework Damp exponent is the
        // elapsed MILLISECONDS (0.99^16.67 ≈ 0.85 per 60Hz frame), so the
        // disc visibly keeps up with fast spins.
        anim.display_rotation = damp_frame(anim.display_rotation, frame.visual_rotation as f64, 0.99, dt);

        // The tracking interpolation (fill alpha + centre size) only
        // advances while incomplete; it freezes once complete.
        if !complete {
            let target_lerp = if frame.tracking { 1.0 } else { 0.0 };
            anim.tracking_lerp = damp_frame(anim.tracking_lerp, target_lerp, 0.985, dt);
        }

        if frame.tracking && anim.spm_started.is_none() {
            anim.spm_started = Some(t);
        }

        let target_fill = 0.1 + (0.98 - 0.1) * progress;
        anim.fill_scale = lerp(anim.fill_scale, target_fill as f32, (dt / 100.0).clamp(0.0, 1.0));

        let rotations = ((frame.total_rotation / 360.0) as f64).floor() as i64;
        if complete && rotations > anim.whole_rotations {
            anim.fill_pulse = Some(t);
        }
        anim.whole_rotations = rotations.max(anim.whole_rotations);

        // Ring/side values: DampContinuously, half-time 40ms.
        let ring_target = if complete { 0.50f64 } else { 0.31 };
        anim.ring_progress = damp_half(anim.ring_progress, ring_target, 40.0, dt);
        let ring_inner_target = if complete { 0.02 * 2.2 } else { 0.02 };
        anim.ring_inner = damp_half(anim.ring_inner, ring_inner_target, 40.0, dt);

        let side_alpha_target = if progress > 0.0 && progress < 1.0 { 1.0 } else { 0.0 };
        anim.side_alpha = damp_half(anim.side_alpha, side_alpha_target, 40.0, dt);
        let side_progress_target = if progress >= 1.0 { 0.0 } else { 0.15 * progress };
        anim.side_progress = damp_half(anim.side_progress, side_progress_target, 40.0, dt);

        // SPM (`SpinnerSpmCalculator`): records accumulate every frame while
        // t <= EndTime (score-side `Result.TotalRotation`, NOT the visual
        // rotation), over a 595ms window; the value FREEZES at EndTime so
        // it doesn't drop during the fade out.
        if t <= obj.end_time {
            anim.spm_history.push((t, frame.total_rotation));
            anim.spm_history.retain(|(ht, _)| t - ht <= 595.0);
            if let Some(first) = anim.spm_history.first() {
                let dtm = t - first.0;
                if dtm > 0.0 {
                    let drot = frame.total_rotation - first.1;
                    anim.spm = (drot as f64 / dtm) * 1000.0 * 60.0 / 360.0;
                }
            }
        }

        // Bonus ticks.
        let mut bonus = 0i64;
        let mut bonus_max = 0i64;
        for (oi, time, large) in &game.spinner_ticks {
            if *oi != idx {
                continue;
            }
            bonus_max += if *large { 50 } else { 10 };
            if *time <= t {
                bonus += if *large { 50 } else { 10 };
            }
        }
        if bonus != anim.bonus_score {
            let is_max = bonus_max > 0 && bonus >= bonus_max;
            anim.bonus_score = bonus;
            anim.bonus_flash = Some((t, is_max));
        }

        // Overall fade (`DrawableSpinner.UpdateHitStateTransforms`:
        // FadeOut(240) - linear). HD multiplies an EXTRA fade that only
        // starts AT the spinner's end time (the `Spinner` case anchors the
        // fade-out at `fadeOutStartTime + longFadeDuration == EndTime`),
        // over preempt * 0.3.
        let mut alpha = if judged {
            value_at(t, jt, jt + 240.0, 1.0, 0.0, Easing::Linear)
        } else {
            1.0
        };
        // Legacy spinners fade the whole sprite stack in over TimeFadeIn
        // (`LegacyOldStyleSpinner.UpdateStateTransforms`: FadeOut at
        // StartTime - preempt, then FadeInFromZero(TimeFadeIn) reaching
        // full alpha at StartTime). The argon spinner appears through its
        // disc/centre pop-in scale instead.
        if self.legacy.is_some() {
            alpha *= value_at(t, obj.start_time - obj.fade_in, obj.start_time, 0.0, 1.0, Easing::Linear);
        }
        if self.hidden {
            alpha *= value_at(
                t,
                obj.end_time,
                obj.end_time + obj.preempt * HD_FADE_OUT_DURATION_MULTIPLIER,
                1.0,
                0.0,
                Easing::Linear,
            );
        }
        if alpha <= 0.003 {
            return;
        }
        let a = alpha as f32;

        // Legacy spinner (`LegacySpinner` + `LegacyOldStyleSpinner` /
        // `LegacyNewStyleSpinner`): the skin's sprites laid out in
        // stable's 640x480 window space.
        if let Some(lg) = &self.legacy {
            let hit_time = if judged && hit { Some(jt) } else { None };
            draw_spinner_legacy(
                lg,
                assets,
                list,
                m,
                obj,
                a,
                t,
                progress,
                anim.complete_at,
                anim.display_rotation,
                anim.spm,
                anim.bonus_score,
                anim.bonus_flash,
                hit_time,
            );
            return;
        }

        // Pop-in scales (ArgonSpinnerDisc.updateStateTransforms): phase 1
        // over [0.5p, 0.75p] (centre -> 0.3, disc -> 0.2), a hold, then
        // phase 2 over [p, 1.5p] (nested BeginDelayedSequence(preempt/2))
        // to centre 0.8 / disc 1.0 - finishing half a preempt AFTER the
        // spinner starts.
        let t0 = appear;
        let p = obj.preempt;
        let mut centre_scale = 0.0f64;
        let mut disc_scale = 0.0f64;
        if t >= t0 + p * 0.5 {
            centre_scale = value_at(t, t0 + p * 0.5, t0 + p * 0.75, 0.0, 0.3, Easing::OutQuint);
            disc_scale = value_at(t, t0 + p * 0.5, t0 + p * 0.75, 0.0, 0.2, Easing::OutQuint);
        }
        if t >= t0 + p {
            centre_scale = value_at(t, t0 + p, t0 + p * 1.5, 0.3, 0.8, Easing::OutQuint);
            disc_scale = value_at(t, t0 + p, t0 + p * 1.5, 0.2, 1.0, Easing::OutQuint);
        }
        // Hit/miss end animations (at the judgement time):
        // hit -> disc 1.2x (320, Out) + the ticks spin an extra half turn;
        // miss -> disc 0.8x (320, In).
        let mut end_spin = 0.0f64;
        if judged && t >= jt {
            if hit {
                disc_scale = value_at(t, jt, jt + 320.0, disc_scale, 1.2, Easing::Out);
                end_spin = value_at(t, jt, jt + 320.0, 0.0, 180.0, Easing::Linear);
            } else {
                disc_scale = value_at(t, jt, jt + 320.0, disc_scale, 0.8, Easing::In);
            }
        }
        if disc_scale <= 0.001 {
            return;
        }

        // Ambient rotation (ticksContainer).
        let ambient = value_at(
            t,
            t0 + p / 2.0,
            t0 + p + obj.duration,
            0.0,
            25.0 * obj.duration / 2000.0,
            Easing::Linear,
        );

        // --- Draw: glow fill -----------------------------------------------------
        // Idle 0.2 <-> tracking 0.4 via the frozen tracking interpolation,
        // plus the complete pulse (0.6 over 60ms OutExpo, back over 250ms
        // OutQuint per new rotation).
        let mut fill_alpha = anim.tracking_lerp * (0.4 - 0.2) + 0.2;
        if let Some(pt) = anim.fill_pulse {
            let x = t - pt;
            fill_alpha += if x < 60.0 {
                value_at(x, 0.0, 60.0, 0.0, 0.2, Easing::OutExpo)
            } else {
                value_at(x, 60.0, 310.0, 0.2, 0.0, Easing::OutQuint)
            };
        }
        let fill_radius = (unit * 0.5 - 8.0 * s) * anim.fill_scale * disc_scale as f32;
        if fill_radius > 2.0 {
            list.glow(centre, fill_radius, Colour::from_hex(0xFC618F).opacity(fill_alpha as f32 * 0.45 * a));
            list.disc(
                centre,
                fill_radius,
                Colour::from_hex(0xFC618F).opacity(fill_alpha as f32 * 0.10 * a),
                Colour::from_hex(0xFC618F).opacity(fill_alpha as f32 * 0.10 * a),
                Blend::Additive,
            );
        }

        // Tick marks ring (rotates with the damped display rotation, inside
        // the ambient-rotating container; +180 on hit). Lazer places the
        // marks at relative (0.5 + sin(t)/2*0.75, 0.5 + cos(t)/2*0.75) and
        // a POSITIVE (clockwise) container rotation moves them to angle
        // (t - rot) - so the ring spins the same direction as the cursor.
        let tick_ring_rot = (ambient + end_spin + anim.display_rotation) as f32;
        let tick_radius = unit * 0.375 * disc_scale as f32;
        for i in 0..25u32 {
            let ang = (i as f32 / 25.0) * std::f32::consts::TAU;
            let rot = ang - tick_ring_rot.to_radians();
            let (sin_a, cos_a) = rot.sin_cos();
            let pos = [centre[0] + sin_a * tick_radius, centre[1] + cos_a * tick_radius];
            let mark_rot = -(i as f32 / 25.0) * 360.0 - 120.0 + tick_ring_rot;
            let half_l = 15.0 * s;
            let (sr, cr) = mark_rot.to_radians().sin_cos();
            let p0 = [pos[0] - cr * half_l, pos[1] - sr * half_l];
            let p1 = [pos[0] + cr * half_l, pos[1] + sr * half_l];
            list.capsule(p0, p1, 2.5 * s, Colour::WHITE.opacity(0.85 * a), Blend::Alpha);
        }

        // Ring arcs (top / bottom), scaling with the disc pop-in (they live
        // inside the disc container in lazer).
        let disc = disc_scale as f32;
        let ring_r = (unit * 0.5 - 8.0 * s) * disc;
        let ring_span = (anim.ring_progress * 360.0) as f32;
        let half = ring_span * 0.5;
        let ring_t = anim.ring_inner as f32 * unit * 0.5 * disc;
        list.arc(centre, ring_r, ring_t, -90.0 - half, -90.0 + half, Colour::WHITE.opacity(0.9 * a), Blend::Alpha);
        list.arc(centre, ring_r, ring_t, 90.0 - half, 90.0 + half, Colour::WHITE.opacity(0.9 * a), Blend::Alpha);

        // Side progress arcs (also inside the disc). The static background
        // switches off INSTANTLY when the spinner completes
        // (background.Alpha = progress >= 1 ? 0 : 1).
        let thickness = 0.12 * (unit * 0.5) * disc;
        let r = unit * 0.5 * disc - thickness * 0.5;
        let bg_half = 0.15 * 360.0 * 0.5;
        if progress < 1.0 {
            list.arc(centre, r, thickness, 180.0 - bg_half, 180.0 + bg_half, Colour::WHITE.opacity(0.25 * a), Blend::Alpha);
            list.arc(centre, r, thickness, -bg_half, bg_half, Colour::WHITE.opacity(0.25 * a), Blend::Alpha);
        }
        let side_span = (anim.side_progress * 360.0) as f32;
        if side_span > 0.5 && anim.side_alpha > 0.01 {
            let side_half = side_span * 0.5;
            let col = Colour::WHITE.opacity((anim.side_alpha * 0.9) as f32 * a);
            list.arc(centre, r, thickness, 180.0 - side_half, 180.0 + side_half, col, Blend::Alpha);
            list.arc(centre, r, thickness, -side_half, side_half, col, Blend::Alpha);
            let glow_col = Colour::rgba_bytes(171, 255, 255, 180).opacity((anim.side_alpha * 0.4) as f32 * a);
            list.arc(centre, r, thickness * 2.4, 180.0 - side_half, 180.0 + side_half, glow_col, Blend::Additive);
            list.arc(centre, r, thickness * 2.4, -side_half, side_half, glow_col, Blend::Additive);
        }

        // Centre rings (size damps 80 -> 40 with tracking).
        let centre_size = (anim.tracking_lerp * (40.0 - 80.0) + 80.0) as f32;
        let cs = centre_size * s * centre_scale as f32;
        if cs > 1.0 {
            list.ring(centre, cs * 0.4, 10.0 * s * centre_scale as f32, Colour::WHITE.opacity(a), Colour::WHITE.opacity(a), Blend::Alpha);
            list.ring(centre, cs * 0.5, 3.0 * s * centre_scale as f32, Colour::WHITE.opacity(a), Colour::WHITE.opacity(a), Blend::Alpha);
        }

        // SPM + bonus counters. The SPM fades in from the FIRST tracking
        // moment (`Result.TimeStarted`), not the spinner start.
        if let Some(started) = anim.spm_started {
            let spm_a = ((t - started) / obj.fade_in).clamp(0.0, 1.0) as f32 * a;
            let spm_pos = [centre[0], centre[1] + 60.0 * s];
            draw_ttf_text(list, assets.atlas, assets.semibold, false, &(anim.spm.trunc() as i64).to_string(), spm_pos, 28.0 * s, Colour::WHITE.opacity(spm_a), 0.0, Blend::Alpha);
            let label_pos = [spm_pos[0], spm_pos[1] + 30.0 * s];
            draw_ttf_text(list, assets.atlas, assets.semibold, false, "SPINS PER MINUTE", label_pos, 16.0 * s, Colour::WHITE.opacity(spm_a * 0.8), 2.0 * s, Blend::Alpha);

            // Bonus counter: normal pops 1.5 -> 1.0 and fades over 1500ms;
            // at MAX it pops to 2.8 and flashes pink, fading over 500ms.
            if let Some((bt, is_max)) = anim.bonus_flash {
                if anim.bonus_score > 0 && t < bt + 1500.0 {
                    let x = t - bt;
                    let (pop, ba) = if is_max {
                        (
                            value_at(x, 0.0, 1000.0, 1.5, 2.8, Easing::OutQuint) as f32,
                            value_at(x, 0.0, 500.0, 1.0, 0.0, Easing::Linear) as f32 * a,
                        )
                    } else {
                        (
                            value_at(x, 0.0, 1000.0, 1.5, 1.0, Easing::OutQuint) as f32,
                            value_at(x, 0.0, 1500.0, 1.0, 0.0, Easing::Linear) as f32 * a,
                        )
                    };
                    let text = if is_max { "MAX".to_string() } else { anim.bonus_score.to_string() };
                    let pos = [centre[0], centre[1] - 100.0 * s];
                    draw_ttf_text(list, assets.atlas, assets.bold, true, &text, pos, 28.0 * s * pop, Colour::from_hex(0xFC618F).opacity(ba), 0.0, Blend::Alpha);
                }
            }
        }
    }
}

/// Legacy skinned spinner (`LegacySpinner` base + `LegacyOldStyleSpinner`
/// / `LegacyNewStyleSpinner`): the skin's sprites laid out in stable's
/// 640x480 window space via [`Mapper::win`] (UNIFORM scale — the window
/// box stays 4:3 on screen, sprites never stretch). Draw order mirrors
/// the framework's depth sort: the style sprites first, then the
/// base-class overlay (rpm background, spm counter, spin, clear, bonus
/// counter — added inside a `Depth = float.MinValue` container) in front.
#[allow(clippy::too_many_arguments)]
fn draw_spinner_legacy(
    lg: &LegacyCache,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    alpha: f32,
    t: f64,
    progress: f64,
    complete_at: Option<f64>,
    display_rotation: f64,
    spm: f64,
    bonus_score: i64,
    bonus_flash: Option<(f64, bool)>,
    hit_time: Option<f64>,
) {
    let unit = m.win_unit();
    let centre = m.win([320.0, LEGACY_SPINNER_Y_CENTRE]);
    let scale = LEGACY_SPINNER_SCALE;

    // A `Sprite` with `Scale = new Vector2(SPRITE_SCALE)`: both axes take
    // the same window unit (the legacy spinner window is aspect-locked).
    let draw_sprite = |list: &mut DrawList, tex: SkinTexture, pos: [f32; 2], s: f32, rotation: f32, colour: Colour, blend: Blend| {
        let w = tex.display_width() * scale * unit * s;
        let h = tex.display_height() * scale * unit * s;
        list.image(assets.atlas, tex.region, pos, [w, h], rotation, colour, blend);
    };

    if lg.spinner_new_style {
        // --- LegacyNewStyleSpinner -----------------------------------------
        // scaleContainer wraps glow/bottom/top/middle2/middle:
        // `SPRITE_SCALE * (0.8 + ApplyEasing(Out, progress) * 0.2)`.
        let eased = 1.0 - (1.0 - progress as f32).powi(2);
        let stack_s = 0.8 + eased * 0.2;

        if let Some(glow) = lg.spinner_glow {
            // Additive, tinted (3, 151, 255); alpha tracks the raw
            // progress, flashes white per bonus tick hit, and fades out
            // over 300ms from the hit state.
            let mut glow_a = (progress as f32).clamp(0.0, 1.0);
            if let Some(ht) = hit_time {
                glow_a *= 1.0 - ((t - ht) / 300.0).clamp(0.0, 1.0) as f32;
            }
            let mut col = Colour::rgba_bytes(3, 151, 255, 255);
            if let Some((bt, _)) = bonus_flash {
                // `FlashColour(White, 200)`: snaps to white, back over 200ms.
                let k = 1.0 - ((t - bt) / 200.0).clamp(0.0, 1.0) as f32;
                col = Colour::lerp(col, Colour::WHITE, k);
            }
            draw_sprite(list, glow, centre, stack_s, 0.0, col.opacity(glow_a * alpha), Blend::Additive);
        }

        // Rotations: `discTop = Rotation * turnRatio` (half speed when
        // `spinner-middle2` exists), `discBottom = discTop / 3`,
        // `spinningMiddle` full speed; glow/middle stay static.
        let rot = display_rotation as f32;
        let turn_ratio = if lg.spinner_middle2.is_some() { 0.5 } else { 1.0 };
        if let Some(bottom) = lg.spinner_bottom {
            draw_sprite(list, bottom, centre, stack_s, rot * turn_ratio / 3.0, Colour::WHITE.opacity(alpha), Blend::Alpha);
        }
        if let Some(top) = lg.spinner_top {
            draw_sprite(list, top, centre, stack_s, rot * turn_ratio, Colour::WHITE.opacity(alpha), Blend::Alpha);
        }
        if let Some(middle2) = lg.spinner_middle2 {
            draw_sprite(list, middle2, centre, stack_s, rot, Colour::WHITE.opacity(alpha), Blend::Alpha);
        }
        if let Some(middle) = lg.spinner_middle {
            // `fixedMiddle.FadeColour(Red, Duration)` from StartTime.
            let red = value_at(t, obj.start_time, obj.start_time + obj.duration, 0.0, 1.0, Easing::Linear) as f32;
            let col = Colour::lerp(Colour::WHITE, Colour::rgba_bytes(255, 0, 0, 255), red);
            draw_sprite(list, middle, centre, stack_s, 0.0, col.opacity(alpha), Blend::Alpha);
        }
    } else {
        // --- LegacyOldStyleSpinner ----------------------------------------
        // (No glow: that sprite belongs to the new-style layout only.)
        if let Some(background) = lg.spinner_background {
            draw_sprite(list, background, centre, 1.0, 0.0, lg.spinner_background_colour.opacity(alpha), Blend::Alpha);
        }

        // Metre (`getMetreHeight` + the masking hack): 10 bars; the partial
        // bar blinks (progress capped at 99 so it keeps blinking at full).
        // The mask reveals texture rows [692 - h .. tex_h] — the metre
        // fills UPWARD from the bottom of the 692-unit bars region, and
        // any texture content below 692 units stays permanently visible.
        if let Some(metre) = lg.spinner_metre {
            const TOTAL_BARS: f64 = 10.0;
            const BARS_HEIGHT: f64 = 692.0; // texture units, `final_metre_height / SPRITE_SCALE`
            let mut p = progress * 100.0;
            if lg.spinner_blink {
                p = p.min(99.0);
            }
            let mut bars = (p as i64) / 10;
            if lg.spinner_blink && p > 0.0 {
                let fraction = (p as i64 % 10) as f64 / 10.0;
                // RNG.NextBool(fraction): deterministic per (time, bar).
                let hash = (t as i64 as u64).wrapping_mul(2654435761).wrapping_add(p as u64);
                if (hash % 1000) as f64 / 1000.0 < fraction {
                    bars += 1;
                }
            }
            let revealed = bars as f64 / TOTAL_BARS * BARS_HEIGHT; // texture units
            let tex_h = metre.display_height() as f64;
            let v0 = ((BARS_HEIGHT - revealed) / tex_h).clamp(0.0, 1.0) as f32;
            let vis_px = (tex_h - (BARS_HEIGHT - revealed).max(0.0)) as f32 * scale * unit;
            if vis_px > 0.5 {
                let w = metre.display_width() * scale * unit;
                // Sprite top pinned at SPINNER_TOP_OFFSET; the revealed
                // slice starts (692 - h) units down and runs to the
                // texture bottom.
                let top = m.win([0.0, LEGACY_SPINNER_TOP_OFFSET]);
                let slice_top = top[1] + (BARS_HEIGHT - revealed).max(0.0) as f32 * scale * unit;
                list.image_sub(
                    assets.atlas,
                    metre.region,
                    [top[0] + w * 0.5, slice_top + vis_px * 0.5],
                    [w, vis_px],
                    0.0,
                    Colour::WHITE.opacity(alpha),
                    Blend::Alpha,
                    0.0,
                    v0,
                    1.0,
                    1.0,
                );
            }
        }

        // The ONLY rotating element: `disc.Rotation = RotationTracker.Rotation`.
        // Drawn AFTER the metre (deviation from lazer's AddRangeInternal
        // order, which puts the disc under the metre): full-artwork metres
        // (BTMC & friends) would otherwise bury the spinning ring, which
        // stable always shows on top.
        if let Some(disc) = lg.spinner_circle {
            draw_sprite(list, disc, centre, 1.0, display_rotation as f32, Colour::WHITE.opacity(alpha), Blend::Alpha);
        }
    }

    // Approach circle (both styles): starts at 1.86x and ScaleTo's to 0.1x
    // linearly over the spinner duration.
    if let Some(approach) = lg.spinner_approachcircle {
        let s = value_at(t, obj.start_time, obj.start_time + obj.duration, 1.86, 0.1, Easing::Linear) as f32;
        if std::env::var("SPINNER_DEBUG").is_ok() {
            eprintln!(
                "SPINNER obj={} t={} start={} end={} dur={} s={:.3} alpha={:.2} tex={}x{} adjust={}",
                obj.index, t, obj.start_time, obj.end_time, obj.duration, s, alpha,
                approach.width, approach.height, approach.scale_adjust
            );
        }
        draw_sprite(list, approach, centre, s, 0.0, Colour::WHITE.opacity(alpha), Blend::Alpha);
    }

    // --- LegacySpinner base overlay (rendered in front) -------------------

    // spinner-rpm background + spm counter: hidden 50 units below their
    // resting spot, sliding up over the fade-in (`spm_hide_offset`).
    let spm_slide = value_at(t, obj.start_time - obj.fade_in, obj.start_time, 0.0, 1.0, Easing::Out) as f32;
    if let Some(rpm) = lg.spinner_rpm {
        let pos = m.win([320.0 - 87.0, 445.0 + 50.0 * (1.0 - spm_slide)]);
        let w = rpm.display_width() * scale * unit;
        let h = rpm.display_height() * scale * unit;
        list.image(assets.atlas, rpm.region, [pos[0] + w * 0.5, pos[1] + h * 0.5], [w, h], 0.0, Colour::WHITE.opacity(alpha), Blend::Alpha);
    }
    if let Some(digits) = &lg.score_digits {
        let spm_scale = scale * 0.9;
        let glyph_h = digits.digits.iter().flatten().next().map(|d| d.display_height()).unwrap_or(0.0);
        let pos = m.win([320.0 + 80.0, 448.0 + 50.0 * (1.0 - spm_slide)]);
        let centre_y = pos[1] + glyph_h * spm_scale * 0.5;
        let text = (spm.trunc() as i64).to_string();
        draw_legacy_number(list, assets.atlas, digits, &text, [pos[0], centre_y], unit * spm_scale, Colour::WHITE.opacity(alpha), Blend::Alpha, NumAlign::Right);
    }

    // "spin" sprite: fades in over the second half of the fade-in,
    // fades out over min(400, duration) before the judgement time.
    if let Some(spin) = lg.spinner_spin {
        let jt = obj.end_time;
        let spin_fade_out = 400.0f64.min(obj.duration);
        let spin_alpha = value_at(t, obj.start_time - obj.fade_in / 2.0, obj.start_time, 0.0, 1.0, Easing::Linear)
            * value_at(t, jt - spin_fade_out, jt, 1.0, 0.0, Easing::Linear);
        if spin_alpha > 0.003 && t < jt {
            let pos = m.win([320.0, LEGACY_SPINNER_TOP_OFFSET + 335.0]);
            draw_sprite(list, spin, pos, 1.0, 0.0, Colour::WHITE.opacity(spin_alpha as f32 * alpha), Blend::Alpha);
        }
    }

    // "clear" sprite on completion (`onCompletedChanged`): fade in 400
    // Out, scale 2x -> 0.8x (240 Out) -> 1x (160), gone 50ms before the
    // judgement.
    if let (Some(clear), Some(ct)) = (lg.spinner_clear, complete_at) {
        let jt = obj.end_time.max(ct);
        let x = t - ct;
        let clear_alpha = value_at(x, 0.0, 400.0, 0.0, 1.0, Easing::Out)
            * value_at(t, jt - 50.0, jt, 1.0, 0.0, Easing::Linear);
        if clear_alpha > 0.003 && t < jt {
            let s = if x < 240.0 {
                value_at(x, 0.0, 240.0, 2.0, 0.8, Easing::Out)
            } else {
                value_at(x, 240.0, 400.0, 0.8, 1.0, Easing::Linear)
            };
            let pos = m.win([320.0, LEGACY_SPINNER_TOP_OFFSET + 115.0]);
            draw_sprite(list, clear, pos, s as f32, 0.0, Colour::WHITE.opacity(clear_alpha as f32 * alpha), Blend::Alpha);
        }
    }

    // Bonus counter in score digits (`bonusCounter`): at max scale jumps
    // to 1.4 then eases to 1.8 (1000 Out) fading over 500ms; otherwise
    // SPRITE_SCALE*2 -> SPRITE_SCALE*1.28 over the 800ms fade (absolute
    // `ScaleTo`s — the max case has no 0.625 factor in lazer).
    if let (Some(digits), Some((bt, is_max))) = (&lg.score_digits, bonus_flash) {
        if bonus_score > 0 {
            let x = t - bt;
            let (s, ba) = if is_max {
                (
                    value_at(x, 0.0, 1000.0, 1.4, 1.8, Easing::Out) as f32,
                    value_at(x, 0.0, 500.0, 1.0, 0.0, Easing::Out) as f32,
                )
            } else {
                (
                    value_at(x, 0.0, 800.0, 2.0 * scale as f64, 1.28 * scale as f64, Easing::Out) as f32,
                    value_at(x, 0.0, 800.0, 1.0, 0.0, Easing::Out) as f32,
                )
            };
            if ba > 0.003 {
                let pos = m.win([320.0, LEGACY_SPINNER_TOP_OFFSET + 299.0]);
                let text = bonus_score.to_string();
                draw_legacy_number(list, assets.atlas, digits, &text, pos, unit * s, Colour::WHITE.opacity(ba * alpha), Blend::Alpha, NumAlign::Centre);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared element drawing (free functions)
// ---------------------------------------------------------------------------

/// The Argon main circle piece (hit circles and slider heads).
#[allow(clippy::too_many_arguments)]
fn draw_circle_piece(
    legacy: Option<&LegacyCache>,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    accent: Colour,
    alpha: f32,
    number: u32,
    with_outer_fill: bool,
    judged: bool,
    hit: bool,
    ht: f64,
    t: f64,
) {
    if let Some(lg) = legacy {
        // SkinnableDrawable fallback semantics: a legacy skin serves the
        // sprites OR the argon vector piece draws (`with_outer_fill` ==
        // false means this is a slider head, which prefers
        // `sliderstartcircle`).
        let pair = if with_outer_fill { lg.hitcircle } else { lg.sliderstartcircle.or(lg.hitcircle) };
        if let Some((circle, overlay)) = pair {
            draw_circle_piece_legacy(
                lg,
                assets,
                list,
                m,
                obj,
                accent,
                alpha,
                number,
                judged,
                hit,
                ht,
                t,
                circle,
                overlay,
            );
            return;
        }
    }
    draw_circle_piece_argon(assets, list, m, obj, accent, alpha, number, with_outer_fill, judged, hit, ht, t)
}

/// `LegacyMainCirclePiece`: tinted `hitcircle` sprite, `hitcircleoverlay`,
/// digit-sprite number (under the overlay when
/// `HitCircleOverlayAboveNumber`), and the hit transforms
/// (`updateStateTransforms`: 240ms Out fade + 1.4x scale; skins v2.0+
/// fade the number over 240/4, older skins pop it like the sprites).
#[allow(clippy::too_many_arguments)]
fn draw_circle_piece_legacy(
    lg: &LegacyCache,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    accent: Colour,
    alpha: f32,
    number: u32,
    judged: bool,
    hit: bool,
    ht: f64,
    t: f64,
    circle: SkinTexture,
    overlay: Option<SkinTexture>,
) {
    let centre = m.pf(obj.position);
    let s = obj.scale * m.pf;
    let x = t - ht;

    // Sprite transforms: 1 -> 1.4 scale over 240ms Out when hit; the
    // fade comes in through `alpha` (240ms linear on hit, 100ms miss).
    let sprite_scale = if judged && hit {
        value_at(x, 0.0, 240.0, 1.0, 1.4, Easing::Out) as f32
    } else {
        1.0
    };

    // `LegacyColourCompatibility.DisallowZeroAlpha` - near-black accents
    // are lifted so the tint never disappears.
    let tint = Colour::rgb(accent.r.max(0.05), accent.g.max(0.05), accent.b.max(0.05));

    let draw_sprite = |list: &mut DrawList, tex: SkinTexture, colour: Colour| {
        let w = tex.display_width() * s * sprite_scale;
        let h = tex.display_height() * s * sprite_scale;
        list.image(assets.atlas, tex.region, centre, [w, h], 0.0, colour.opacity(alpha), Blend::Alpha);
    };

    draw_sprite(list, circle, tint);

    // Number: digit sprites at their authored size, fading per the
    // skin's legacy version on hit.
    if let Some(digits) = &lg.hitcircle_digits && number > 0 {
        let number_alpha = if judged && hit {
            if lg.version > 1.0 {
                value_at(x, 0.0, 240.0 / 4.0, 1.0, 0.0, Easing::Linear) as f32
            } else {
                value_at(x, 0.0, 240.0, 1.0, 0.0, Easing::Linear) as f32
            }
        } else {
            1.0
        };
        if number_alpha > 0.003 {
            let text = number.to_string();
            draw_legacy_number(
                list,
                assets.atlas,
                digits,
                &text,
                centre,
                s * sprite_scale,
                Colour::WHITE.opacity(alpha * number_alpha),
                Blend::Alpha,
                NumAlign::Centre,
            );
        }
    }

    if let Some(overlay) = overlay {
        draw_sprite(list, overlay, Colour::WHITE);
    }
}

fn draw_circle_piece_argon(
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    accent: Colour,
    alpha: f32,
    number: u32,
    with_outer_fill: bool,
    judged: bool,
    hit: bool,
    ht: f64,
    t: f64,
) {
    let centre = m.pf(obj.position);
    let s = obj.scale * m.pf; // 128-space -> screen px
    let x = t - ht;

    let mut outer_fill_a = alpha;
    let mut inner_fill_a = alpha;
    let mut inner_grad_a = alpha;
    let mut outer_grad_a = alpha;
    let mut number_a = alpha;
    let mut outer_grad_size = OUTER_GRADIENT_SIZE;
    let mut border_size = 128.0f64;
    let mut flash_a = 0.0f64;
    // `outerGradient.FadeColour(Color4.White, 80)` mix factor (linear).
    let mut outer_grad_white = 0.0f32;

    if judged && hit {
        // Port of `ArgonMainCirclePiece.updateStateTransforms` (Hit).
        // Fill layers hide over 150ms OutQuint; the number over 75ms.
        outer_fill_a *= value_at(x, 0.0, 150.0, 1.0, 0.0, Easing::OutQuint) as f32;
        inner_fill_a = outer_fill_a;
        inner_grad_a = outer_fill_a;
        number_a *= value_at(x, 0.0, 75.0, 1.0, 0.0, Easing::Linear) as f32;
        let delayed = x - 150.0 / 12.0;
        // The outer gradient resizes on the delayed sequence (bomb-like
        // effect triggered by the border's shrink).
        outer_grad_size =
            OUTER_GRADIENT_SIZE * value_at(delayed, 0.0, 400.0, 1.0, 0.8, Easing::OutElasticHalf) as f32;
        // FadeColour(White, 80) then FadeOut(150): both LINEAR (the source
        // passes no easing), starting on the delayed sequence.
        outer_grad_white = value_at(delayed, 0.0, 80.0, 0.0, 1.0, Easing::Linear) as f32;
        outer_grad_a *= value_at(delayed, 80.0, 230.0, 1.0, 0.0, Easing::Linear) as f32;
        border_size = 128.0 * value_at(x, 0.0, 400.0, 1.0, 0.8, Easing::OutElasticHalf) + BORDER_THICKNESS as f64;
        // Flash: pops in over 150ms OutQuint then straight back out over
        // 150ms (`flash.FadeTo(1, 150, OutQuint).Then().FadeOut(150,
        // OutQuint)`) - the colour block is gone by 300ms, well before the
        // ring's 640ms fade.
        flash_a = value_at(x, 0.0, 150.0, 0.0, 1.0, Easing::OutQuint)
            * value_at(x, 150.0, 300.0, 1.0, 0.0, Easing::OutQuint);
    }

    if with_outer_fill && outer_fill_a > 0.003 {
        let r = (128.0 - 1.0) * 0.5;
        list.disc(centre, r * s, Colour::BLACK.opacity(outer_fill_a), Colour::BLACK.opacity(outer_fill_a), Blend::Alpha);
    }
    if outer_grad_a > 0.003 {
        // GradientVertical(accent, accent.Darken(0.1)) blended to pure white
        // over the first 80ms (FadeColour), gamma-correct like the framework.
        let top = Colour::lerp_linear(accent, Colour::WHITE, outer_grad_white);
        let bottom = Colour::lerp_linear(accent.darken(0.1), Colour::WHITE, outer_grad_white);
        list.disc(centre, outer_grad_size * 0.5 * s, top.opacity(outer_grad_a), bottom.opacity(outer_grad_a), Blend::Alpha);
    }
    if inner_grad_a > 0.003 {
        list.disc(centre, INNER_GRADIENT_SIZE * 0.5 * s, accent.darken(0.5).opacity(inner_grad_a), accent.darken(0.6).opacity(inner_grad_a), Blend::Alpha);
    }
    if inner_fill_a > 0.003 {
        list.disc(centre, INNER_FILL_SIZE * 0.5 * s, Colour::BLACK.opacity(inner_fill_a), Colour::BLACK.opacity(inner_fill_a), Blend::Alpha);
    }

    // Number (lazer child order: under the flash, over the fills).
    if number_a > 0.003 && number > 0 {
        let text = number.to_string();
        let pos = [centre[0], centre[1] - 2.0 * obj.scale * m.pf];
        draw_ttf_text(list, assets.atlas, assets.bold, true, &text, pos, 52.0 * obj.scale * m.pf, Colour::WHITE.opacity(number_a), 0.0, Blend::Alpha);
    }

    // Flash: the FlashPiece renders ONLY its EdgeEffect glow (Child.Alpha =
    // 0). With Hollow = false the glow FILLS the shape interior at full
    // brightness (masking shader: alpha 1 inside the r = 32 circle, then
    // ((32 + R - d) / R)^2 outward). The glow radius R is
    // OBJECT_RADIUS * 0.6 = 38.4. Additive, raw accent colour; only the
    // piece fade scales it. Gone by 300ms - faster than the ring.
    if flash_a > 0.003 {
        list.glow_fill(centre, 32.0 * s, 38.4 * s, accent.opacity(flash_a as f32 * alpha));
    }

    // Border ring (topmost layer, per lazer child order).
    let br = (border_size as f32) * 0.5;
    if judged && hit {
        // `border.TransformTo(BorderColour, GradientVertical(accent.Opacity(0.5),
        // accent.Opacity(0)), 800)`: per-corner linear-space colour
        // interpolation. The whole ring is then uniformly multiplied by the
        // piece fade (`this.FadeOut(800, OutQuad)`, hit-lighting branch).
        let f = value_at(x, 0.0, 800.0, 0.0, 1.0, Easing::Linear) as f32;
        let top = Colour::lerp_linear(Colour::WHITE, accent.opacity(0.5), f).opacity(alpha);
        let bottom = Colour::lerp_linear(Colour::WHITE, accent.opacity(0.0), f).opacity(alpha);
        list.ring(centre, br * s, BORDER_THICKNESS * s, top, bottom, Blend::Alpha);
    } else {
        list.ring(centre, br * s, BORDER_THICKNESS * s, Colour::WHITE.opacity(alpha), Colour::WHITE.opacity(alpha), Blend::Alpha);
    }
}

fn draw_slider_tick(
    legacy: Option<&LegacyCache>,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    position: [f32; 2],
    time: f64,
    span_index: usize,
    judged: Option<(f64, HitResult)>,
    t: f64,
    hidden: bool,
) {
    let span_duration = obj.duration / obj.span_count.max(1) as f64;
    let offset = if span_index > 0 { 200.0 } else { obj.preempt * 0.66 };
    let tick_preempt = span_duration / 2.0 + offset;
    let appear = time - tick_preempt.max(0.0);
    if t < appear {
        return;
    }

    let (j_time, j_hit) = match judged {
        Some((time, r)) => (Some(time), hit_result_ext::is_hit(r)),
        None => (None, false),
    };

    let mut alpha = value_at(t, appear, appear + 150.0, 0.0, 1.0, Easing::Linear);
    let mut scale = value_at(t, appear, appear + 600.0, 0.5, 1.0, Easing::OutElasticHalf);
    if let Some(jt) = j_time {
        if t > jt {
            alpha *= value_at(t, jt, jt + 150.0, 1.0, 0.0, Easing::OutQuint);
            if j_hit {
                scale = value_at(t, jt, jt + 150.0, 1.0, 1.5, Easing::Out);
            }
        }
    }
    // HD tick window (`SliderTick` case): linear fade ending exactly AT the
    // tick time, over `min(preempt - ANIM_DURATION, 1000)`.
    if hidden {
        let dur = (obj.preempt - HD_TICK_ANIM_DURATION).min(1000.0).max(0.0);
        alpha *= value_at(t, time - dur, time, 1.0, 0.0, Easing::Linear);
    }
    if alpha <= 0.003 {
        return;
    }

    let pos = m.pf(position);
    // Legacy: `sliderscorepoint` sprite at its authored size.
    if let Some(lg) = legacy
        && let Some(tex) = lg.scorepoint
    {
        let w = tex.display_width() * obj.scale * m.pf * scale as f32;
        let h = tex.display_height() * obj.scale * m.pf * scale as f32;
        list.image(assets.atlas, tex.region, pos, [w, h], 0.0, Colour::WHITE.opacity(alpha as f32), Blend::Alpha);
        return;
    }

    let size = 6.0 * obj.scale * m.pf * scale as f32;
    list.ring(pos, size, 3.0 * obj.scale * m.pf, obj.colour.opacity(alpha as f32), obj.colour.opacity(alpha as f32), Blend::Alpha);
}

/// Repeat-arrow fade: `ApplyRepeatFadeIn` in, then the hit/miss fade of
/// `DrawableSliderRepeat.UpdateHitStateTransforms` (Out / linear 300ms or
/// spanDuration, whichever is shorter).
fn repeat_fade(
    t: f64,
    fade_start: f64,
    fade_time: f64,
    judged: Option<(f64, HitResult)>,
    anim_duration: f64,
) -> f64 {
    let mut alpha = value_at(t, fade_start, fade_start + fade_time, 0.0, 1.0, Easing::Linear);
    if let Some((jt, r)) = judged {
        if t > jt {
            let e = if hit_result_ext::is_hit(r) { Easing::Out } else { Easing::Linear };
            alpha *= value_at(t, jt, jt + anim_duration, 1.0, 0.0, e);
        }
    }
    alpha
}

fn draw_repeat_arrow(
    legacy: Option<&LegacyCache>,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    position: [f32; 2],
    time: f64,
    judged: Option<(f64, HitResult)>,
    t: f64,
    rot: f32,
) {
    let (j_time, j_hit) = match judged {
        Some((time, r)) => (Some(time), hit_result_ext::is_hit(r)),
        None => (None, false),
    };
    let span_duration = obj.duration / obj.span_count.max(1) as f64;
    let anim_duration = 300.0f64.min(span_duration);
    if let Some(jt) = j_time {
        if t > jt + anim_duration {
            return;
        }
    }

    // `SliderEndCircle.ApplyDefaultsToSelf`:
    // - first repeat: TimePreempt = slider preempt + spanDuration, i.e. its
    //   lifetime starts WITH the slider; delayed preempt/3 for snaking-in.
    // - later repeats: TimePreempt = 2 * spanDuration (appear exactly after
    //   the previous circle on the same end is hit), TimeFadeIn = 0.
    let repeat_index = obj
        .nested
        .iter()
        .filter(|n| n.kind == NestedKind::Repeat)
        .position(|n| n.time == time)
        .unwrap_or(0);
    let (fade_start, fade_time) = if repeat_index == 0 {
        (obj.start_time - obj.preempt + obj.preempt / 3.0, 150.0f64)
    } else {
        (time - 2.0 * span_duration, 150.0f64.min(span_duration))
    };
    // `AnimationStartTime` = StartTime - TimePreempt (the repeat's lifetime
    // start): first repeat appears with the slider, later ones exactly two
    // spans before their own time. The pulse loop phases from this.
    let anim_start = if repeat_index == 0 {
        obj.start_time - obj.preempt
    } else {
        time - 2.0 * span_duration
    };
    if t < anim_start {
        return;
    }

    let alpha = repeat_fade(t, fade_start, fade_time, judged, anim_duration);
    if alpha <= 0.003 {
        return;
    }

    let pos = m.pf(position);
    let s = obj.scale * m.pf;

    // `LegacyReverseArrow`: the skin's `reversearrow` sprite replaces the
    // whole argon assembly. v1.0 skins wobble the rotation (+-5.625 deg
    // over the 300ms loop) and pulse the scale linearly; newer skins only
    // pulse the scale (Out). On hit the arrow pops 1 -> 1.4 over
    // min(300, spanDuration) Out.
    if let Some(lg) = legacy
        && let Some(tex) = lg.reversearrow
    {
        let loop_t = (t - anim_start).max(0.0) % 300.0;
        let old_skin = lg.version <= 1.0;
        let (mut scale, wobble) = if old_skin {
            (
                value_at(loop_t, 0.0, 300.0, 1.3, 1.0, Easing::Linear),
                value_at(loop_t, 0.0, 300.0, 5.625, -5.625, Easing::Linear) as f32,
            )
        } else {
            (value_at(loop_t, 0.0, 300.0, 1.3, 1.0, Easing::Out), 0.0)
        };
        if let Some(jt) = j_time {
            if j_hit && t >= jt {
                scale = value_at(t, jt, jt + anim_duration, 1.0, 1.4, Easing::Out);
            }
        }
        let w = tex.display_width() * s * scale as f32;
        let h = tex.display_height() * s * scale as f32;
        list.image(assets.atlas, tex.region, pos, [w, h], rot + wobble, Colour::WHITE.opacity(alpha as f32), Blend::Alpha);
        return;
    }

    let mut scale = 1.0f64;
    if let Some(jt) = j_time {
        if j_hit && t >= jt {
            scale = value_at(t, jt, jt + anim_duration, 1.0, 1.5, Easing::Out);
        }
    }
    // The 300ms loop pulses the whole `main` container (pill + chevrons)
    // and slides the edge piece, phased from the repeat's lifetime start.
    // On hit, `ArgonReverseArrow.Update` returns early: `main.Scale` and
    // `side.X` freeze at the values they had at the hit instant (only the
    // outer 1 -> 1.5 scale keeps going); a miss keeps pulsing while fading.
    let anim_time = if j_hit {
        j_time.map(|jt| jt.min(t) - anim_start).unwrap_or(t - anim_start)
    } else {
        t - anim_start
    };
    let pulse = pulse_at(anim_time);

    // Edge piece: wraps around the slider body end (unrotated, like the
    // skin's `side` sprite), size OUTER_GRADIENT_SIZE.
    {
        let size = OUTER_GRADIENT_SIZE * obj.scale * m.pf * scale as f32;
        // The whole reverse-arrow assembly (edge piece included) rotates
        // with the end direction; the edge piece wraps the semicircle cap.
        let rot_deg = rot;
        let (srr, crr) = rot_deg.to_radians().sin_cos();
        // `side` pulse slide (ArgonReverseArrow.Update): local X (the aim
        // direction) slides 0 -> -12 over 35ms (Out), back over 250ms (Out),
        // cycling every 300ms - i.e. OUT of the body, opposite the aim.
        let side_x = side_slide_at(anim_time);
        let shift = side_x as f32 * obj.scale * m.pf * scale as f32;
        list.image(
            assets.atlas,
            crate::draw::Region::RepeatEdge,
            [pos[0] + crr * shift, pos[1] + srr * shift],
            [size, size],
            rot_deg,
            Colour::WHITE.opacity(alpha as f32),
            Blend::Alpha,
        );
    }

    // White pill (40x20), rotated to the arrow direction, scaled by pulse.
    let (sr, cr) = rot.to_radians().sin_cos();
    let pill_w = 40.0 * s * pulse as f32 * scale as f32;
    let pill_h = 20.0 * s * pulse as f32 * scale as f32;
    let half_l = pill_w * 0.5 - pill_h * 0.5;
    list.capsule(
        [pos[0] - cr * half_l, pos[1] - sr * half_l],
        [pos[0] + cr * half_l, pos[1] + sr * half_l],
        pill_h * 0.5,
        Colour::WHITE.opacity(alpha as f32),
        Blend::Alpha,
    );

    // Dark double chevron (FontAwesome AngleDoubleRight, sprite Size 16):
    // the icon colour is `accentColour.Darken(4)` - a very dark TINT of
    // the slider's combo colour, not flat black. Metrics measured from the
    // framework's actual FontAwesome5 atlas (BMFont): the double glyph's
    // ink is 74x60 while ChevronRight's is 49x81, so each chevron in the
    // pair is 60/81 the single glyph's height (ink height ~9.94 at the
    // Size-16 box) and the two centres sit ~6.2 units apart - they nearly
    // touch edge to edge, never a wide gap.
    let unit = s * pulse as f32 * scale as f32;
    let chev_size = 11.85 * unit;
    let chev_thickness = 11.85 * 0.094 * unit;
    let chev_col = obj.colour.darken(4.0).opacity(alpha as f32);
    let off = 3.1 * unit;
    let p1 = [pos[0] - cr * off, pos[1] - sr * off];
    let p2 = [pos[0] + cr * off, pos[1] + sr * off];
    draw_chevron(list, p1, rot, chev_size, chev_thickness, chev_col, chev_col, Blend::Alpha);
    draw_chevron(list, p2, rot, chev_size, chev_thickness, chev_col, chev_col, Blend::Alpha);
}

/// Screen-space body geometry for a slider at time `t`: (sub-path points,
/// outer radius, border thickness). None when fully retracted.
pub fn slider_body_geometry(
    m: &Mapper,
    obj: &ObjView,
    t: f64,
    head_hit: bool,
) -> Option<(Vec<[f32; 2]>, f32, f32, f32)> {
    let appear = obj.start_time - obj.preempt;
    if t < appear {
        return None;
    }
    let alpha = ((t - appear) / obj.fade_in).clamp(0.0, 1.0) as f32;
    let completion = ((t - obj.start_time) / obj.duration.max(1e-9)).clamp(0.0, 1.0);
    let body_completion = if head_hit { completion } else { 0.0 };
    let raw = body_completion * obj.span_count as f64;
    let span = (raw as usize).min(obj.span_count.saturating_sub(1));
    let frac = (raw - span as f64).clamp(0.0, 1.0);
    let path_progress = if span % 2 == 1 { 1.0 - frac } else { frac };

    let mut p0 = 0.0f64;
    let mut p1 = ((t - appear) / (obj.preempt / 3.0)).clamp(0.0, 1.0);
    if span >= obj.span_count.saturating_sub(1) {
        if span % 2 == 1 {
            p0 = 0.0;
            p1 = path_progress;
        } else {
            p0 = path_progress;
            p1 = 1.0;
        }
    }
    let sub = obj.sub_path(p0, p1);
    if sub.len() < 2 {
        return None;
    }
    let len_px: f32 = sub
        .windows(2)
        .map(|w| {
            let dx = w[1][0] - w[0][0];
            let dy = w[1][1] - w[0][1];
            (dx * dx + dy * dy).sqrt()
        })
        .sum();
    if len_px < 4.0 {
        return None;
    }
    let screen_pts: Vec<[f32; 2]> = sub.iter().map(|&p| m.pf(p)).collect();
    let cap_r = (OUTER_GRADIENT_SIZE / 2.0) * obj.scale * m.pf;
    let cap_border = GRADIENT_THICKNESS * obj.scale * m.pf;
    Some((screen_pts, cap_r, cap_border, alpha))
}

fn pulse_at(loop_time: f64) -> f64 {
    let total = 300.0;
    let cur = if loop_time < 0.0 { 0.0 } else { loop_time % total };
    if cur < 35.0 {
        value_at(cur, 0.0, 35.0, 1.0, 1.3, Easing::Out)
    } else {
        value_at(cur, 35.0, 285.0, 1.3, 1.0, Easing::Out)
    }
}

/// `side.X` slide in `ArgonReverseArrow.Update`: local X (the aim direction)
/// slides 0 -> -12 over 35ms (Out), back over 250ms (Out), cycling every
/// 300ms - i.e. OUT of the body, opposite the aim.
fn side_slide_at(loop_time: f64) -> f64 {
    let total = 300.0;
    let cur = if loop_time < 0.0 { 0.0 } else { loop_time % total };
    if cur < 35.0 {
        value_at(cur, 0.0, 35.0, 0.0, -12.0, Easing::Out)
    } else {
        value_at(cur, 35.0, 285.0, -12.0, 0.0, Easing::Out)
    }
}

/// Repeat-arrow anchor for one frame, porting
/// `DrawableSliderRepeat.UpdateSnakingPosition`: the arrow rides the snaked
/// body tip (end repeats at the snake end, head repeats at its start) and
/// aims at the nearest curve point distinct from that tip (framework
/// `Precision.AlmostEquals`, 1e-3 per axis). Hit repeats freeze in place -
/// and since every repeat is judged while the body is at full extent, the
/// frozen anchor is the fully-extended path end. Returns (position, aim
/// rotation in degrees).
fn repeat_anchor(obj: &ObjView, p0: f64, p1: f64, at_end: bool, frozen: bool) -> ([f32; 2], f32) {
    if !frozen {
        let sub = obj.sub_path(p0, p1);
        if !sub.is_empty() {
            let tip = if at_end { sub[sub.len() - 1] } else { sub[0] };
            // Walk inward from the tip along the current curve.
            let len = sub.len() as isize;
            let mut i = if at_end { len - 2 } else { 1 };
            while i >= 0 && i < len {
                let p = sub[i as usize];
                if (p[0] - tip[0]).abs() > 1e-3 || (p[1] - tip[1]).abs() > 1e-3 {
                    let rot = (p[1] - tip[1]).atan2(p[0] - tip[0]).to_degrees();
                    return (tip, rot);
                }
                i += if at_end { -1 } else { 1 };
            }
            // Degenerate tail: every point coincides with the tip; osu aims
            // at a zero vector, i.e. rotation 0.
            return (tip, 0.0);
        }
    }
    // Frozen (hit) repeat, or no drawable body: the full-path end.
    let n = obj.slider_points.len();
    if n < 2 {
        return (obj.position, 0.0);
    }
    let (a, b) = if at_end {
        (obj.slider_points[n - 1], obj.slider_points[n - 2])
    } else {
        (obj.slider_points[0], obj.slider_points[1])
    };
    (
        [obj.position[0] + a[0], obj.position[1] + a[1]],
        (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L-shaped path: head (0,0) -> corner (100,0) -> end (100,100), slider
    /// positioned at (100,100) in playfield coords.
    fn test_slider(spans: usize) -> ObjView {
        ObjView {
            index: 0,
            kind: ObjKind::Slider,
            start_time: 1000.0,
            end_time: 1000.0 + spans as f64 * 250.0,
            position: [100.0, 100.0],
            end_position: [200.0, 200.0],
            radius: 36.0,
            scale: 1.0,
            preempt: 450.0,
            fade_in: 400.0,
            new_combo: true,
            colour: Colour::WHITE,
            combo_colour_index: 0,
            number: 1,
            slider_points: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]],
            slider_distance: 200.0,
            span_count: spans,
            duration: spans as f64 * 250.0,
            nested: Vec::new(),
            spins_required: 0.0,
            head_judged: None,
            body_judged: None,
        }
    }

    /// Hidden mod windows, locked against `OsuModHidden`:
    /// fade-in 0.4 * preempt (non-sliders; sliders keep the default),
    /// fade-out linear over the last `0.3 * preempt` starting
    /// `fade_in` after the lifetime start.
    #[test]
    fn hd_windows_match_osu_mod_hidden() {
        // Circle, preempt 1000, HD fade-in 400.
        let mut circle = test_slider(1);
        circle.kind = ObjKind::Circle;
        circle.preempt = 1000.0;
        assert!((hd_fade_in(&circle) - 400.0).abs() < 1e-9);

        // Slider keeps the default fade-in.
        let slider = test_slider(1);
        assert!((hd_fade_in(&slider) - 400.0).abs() < 1e-9);
        let mut short_preempt_slider = test_slider(1);
        short_preempt_slider.preempt = 450.0;
        short_preempt_slider.fade_in = 311.0;
        assert!((hd_fade_in(&short_preempt_slider) - 311.0).abs() < 1e-9);

        // Circle fade-out window [start - 1000 + 400, + 300] = [9400, 9700].
        let fo = |t| hd_fade_out(1000.0, 10000.0, 400.0, t);
        assert!((fo(9200.0) - 1.0).abs() < 1e-9);
        assert!((fo(9400.0) - 1.0).abs() < 1e-9);
        assert!((fo(9550.0) - 0.5).abs() < 1e-9);
        assert!((fo(9700.0) - 0.0).abs() < 1e-9);
        assert!((fo(9900.0) - 0.0).abs() < 1e-9);
    }

    /// While snaking in, the end arrow rides the tip and aims back along the
    /// snaked body (`UpdateSnakingPosition` searches inward from the tip).
    #[test]
    fn end_arrow_rides_snake_tip() {
        let obj = test_slider(2);
        // Half-snaked: tip at the corner (100,0)+offset; the body behind it
        // runs toward the head (-x), so the arrow aims 180 degrees.
        let (pos, rot) = repeat_anchor(&obj, 0.0, 0.5, true, false);
        assert!((pos[0] - 200.0).abs() < 0.6 && (pos[1] - 100.0).abs() < 0.6, "pos {:?}", pos);
        assert!((rot - 180.0).abs() < 0.1, "rot {}", rot);
        // Fully snaked: tip at the end, aiming back up along -y.
        let (pos, rot) = repeat_anchor(&obj, 0.0, 1.0, true, false);
        assert!((pos[0] - 200.0).abs() < 0.6 && (pos[1] - 200.0).abs() < 0.6, "pos {:?}", pos);
        assert!((rot + 90.0).abs() < 0.1, "rot {}", rot);
    }

    /// A head repeat aims into the path from wherever the snaked body starts;
    /// during snaking-out that start retracts away from the head.
    #[test]
    fn head_arrow_follows_snaked_start() {
        let obj = test_slider(3);
        // Fully snaked: arrow at the head aiming +x along the body.
        let (pos, rot) = repeat_anchor(&obj, 0.0, 1.0, false, false);
        assert!((pos[0] - 100.0).abs() < 0.6 && (pos[1] - 100.0).abs() < 0.6, "pos {:?}", pos);
        assert!(rot.abs() < 0.1, "rot {}", rot);
        // Snaking out from the head: start retracted to the corner (100,0),
        // body continues toward (100,100) -> aims +90.
        let (pos, rot) = repeat_anchor(&obj, 0.5, 1.0, false, false);
        assert!((pos[0] - 200.0).abs() < 0.6 && (pos[1] - 100.0).abs() < 0.6, "pos {:?}", pos);
        assert!((rot - 90.0).abs() < 0.1, "rot {}", rot);
    }

    /// Hit repeats freeze on the fully-extended path end regardless of the
    /// current snaked range.
    #[test]
    fn hit_arrow_freezes_at_full_end() {
        let obj = test_slider(2);
        // Frozen head repeat: stays at the head aiming into the path.
        let (pos, rot) = repeat_anchor(&obj, 0.5, 1.0, false, true);
        assert!((pos[0] - 100.0).abs() < 0.6 && (pos[1] - 100.0).abs() < 0.6, "pos {:?}", pos);
        assert!(rot.abs() < 0.1, "rot {}", rot);
        // Frozen end repeat: stays at the full end.
        let (pos, _) = repeat_anchor(&obj, 0.0, 0.5, true, true);
        assert!((pos[0] - 200.0).abs() < 0.6 && (pos[1] - 200.0).abs() < 0.6, "pos {:?}", pos);
    }

    /// Degenerate fully-retracted body (p0 == p1): the arrow stays on the
    /// collapsed tip with rotation 0 (osu aims at a zero vector).
    #[test]
    fn degenerate_body_collapses_to_tip() {
        let obj = test_slider(2);
        let (pos, rot) = repeat_anchor(&obj, 1.0, 1.0, true, false);
        assert!((pos[0] - 200.0).abs() < 0.6 && (pos[1] - 200.0).abs() < 0.6, "pos {:?}", pos);
        assert!(rot == 0.0, "rot {}", rot);
    }
}

/// Slider ball rotation: `DrawableSliderBall.UpdateProgress` -
/// `-90 - atan2(diff.x, diff.y)` where diff is the position BEHIND the ball
/// minus the position AHEAD (both clamped so they never coincide at the
/// path ends: behind at `min(1 - check, progress)`, ahead at `min(1, p+check)`).
pub fn ball_rotation(obj: &ObjView, progress: f64) -> f32 {
    let check = 0.1 / obj.slider_distance.max(1.0);
    let p0 = obj.slider_ball_at((progress).min(1.0 - check).clamp(0.0, 1.0));
    let p1 = obj.slider_ball_at((progress + check).min(1.0));
    let diff = [p0[0] - p1[0], p0[1] - p1[1]];
    if (diff[0] * diff[0] + diff[1] * diff[1]).sqrt() < 0.01 {
        return 0.0;
    }
    (-90.0 - diff[0].atan2(diff[1]).to_degrees()) as f32
}

impl SceneState {
    /// Dumps the scene geometry at time `t` as JSON for the verification
    /// harness (slider body polylines, approach rings, slider balls).
    pub fn probe_dump(&self, game: &GameData, t: f64, path: &str) {
        let m = &self.mapper;
        let mut out = String::from("{");

        out.push_str("\"sliders\":[");
        let mut first = true;
        for obj in &game.objects {
            if obj.kind != ObjKind::Slider {
                continue;
            }
            let head_hit = obj.head_judged.map(|(jt, r)| jt <= t && hit_result_ext::is_hit(r)).unwrap_or(false);
            if let Some((pts, cap_r, cap_border, alpha)) = slider_body_geometry(m, obj, t, head_hit) {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!("{{\"obj\":{},\"r\":{:.1},\"border\":{:.1},\"alpha\":{:.2},\"points\":[",
                    obj.index, cap_r, cap_border, alpha
                ));
                for (i, p) in pts.iter().enumerate() {
                    if i > 0 { out.push(','); }
                    out.push_str(&format!("[{:.1},{:.1}]", p[0], p[1]));
                }
                out.push_str("]}");
            }
        }
        out.push_str("],");

        out.push_str("\"repeats\":[");
        first = true;
        for obj in &game.objects {
            if obj.kind != ObjKind::Slider {
                continue;
            }
            // Snaked range identical to draw_slider.
            let appear = obj.start_time - obj.preempt;
            let head_hit = obj
                .head_judged
                .map(|(jt, r)| jt <= t && hit_result_ext::is_hit(r))
                .unwrap_or(false);
            let completion = ((t - obj.start_time) / obj.duration.max(1e-9)).clamp(0.0, 1.0);
            let body_completion = if head_hit { completion } else { 0.0 };
            let raw = body_completion * obj.span_count as f64;
            let span = (raw as usize).min(obj.span_count.saturating_sub(1));
            let frac = (raw - span as f64).clamp(0.0, 1.0);
            let path_progress = if span % 2 == 1 { 1.0 - frac } else { frac };
            let mut p0 = 0.0f64;
            let mut p1 = ((t - appear) / (obj.preempt / 3.0)).clamp(0.0, 1.0);
            if span >= obj.span_count.saturating_sub(1) {
                if span % 2 == 1 {
                    p0 = 0.0;
                    p1 = path_progress;
                } else {
                    p0 = path_progress;
                    p1 = 1.0;
                }
            }
            let span_duration = obj.duration / obj.span_count.max(1) as f64;
            let mut ri = 0usize;
            for n in &obj.nested {
                if n.kind != NestedKind::Repeat {
                    continue;
                }
                let at_end = n.path_progress >= 0.999;
                let frozen = matches!(n.judged, Some((jt, r)) if jt <= t && hit_result_ext::is_hit(r));
                let (pos, aim) = repeat_anchor(obj, p0, p1, at_end, frozen);
                let rot = self
                    .slider_anims
                    .get(obj.index)
                    .and_then(|a| a.repeat_rots.get(ri))
                    .map(|&(_, r)| r)
                    .unwrap_or(aim);
                // Same fade as draw_repeat_arrow (0 while pre-lifetime).
                let anim_start = if ri == 0 {
                    obj.start_time - obj.preempt
                } else {
                    n.time - 2.0 * span_duration
                };
                let (fade_start, fade_time) = if ri == 0 {
                    (obj.start_time - obj.preempt + obj.preempt / 3.0, 150.0f64)
                } else {
                    (n.time - 2.0 * span_duration, 150.0f64.min(span_duration))
                };
                let a = if t < anim_start {
                    0.0
                } else {
                    repeat_fade(t, fade_start, fade_time, n.judged, 300.0f64.min(span_duration))
                };
                let sp = m.pf(pos);
                let rad = rot.to_radians();
                let tip = [sp[0] + rad.cos() * 10.0, sp[1] + rad.sin() * 10.0];
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!(
                    "{{\"obj\":{},\"pos\":[{:.1},{:.1}],\"rot\":{:.1},\"a\":{:.2},\"tip\":[{:.1},{:.1}]}}",
                    obj.index, sp[0], sp[1], rot, a, tip[0], tip[1]
                ));
                ri += 1;
            }
        }
        out.push_str("],");

        out.push_str("\"balls\":[");
        first = true;
        for obj in &game.objects {
            if obj.kind != ObjKind::Slider || t < obj.start_time || t > obj.end_time {
                continue;
            }
            let completion = ((t - obj.start_time) / obj.duration.max(1e-9)).clamp(0.0, 1.0);
            let bp = obj.slider_ball_at(completion);
            let sp = m.pf(bp);
            if !first { out.push(','); }
            first = false;
            out.push_str("{\"obj\":");
            out.push_str(&obj.index.to_string());
            out.push_str(",\"pos\":[");
            out.push_str(&format!("{:.1},{:.1}", sp[0], sp[1]));
            out.push_str("],\"rot\":");
            out.push_str(&format!("{:.1}", ball_rotation(obj, completion)));
            out.push_str("}");
        }
        out.push_str("],");

        out.push_str("\"approach\":[");
        first = true;
        for obj in &game.objects {
            let appear = obj.start_time - obj.preempt;
            if t < appear || t > obj.start_time {
                continue;
            }
            let judged = match obj.kind {
                ObjKind::Circle => obj.body_judged.map(|(jt, _)| jt <= t).unwrap_or(false),
                ObjKind::Slider => obj.head_judged.map(|(jt, _)| jt <= t).unwrap_or(false),
                ObjKind::Spinner => continue,
            };
            if judged {
                continue;
            }
            let scale = value_at(t, appear, obj.start_time, 4.0, 1.0, Easing::Linear) as f32;
            let centre = m.pf(obj.position);
            let size = 128.0 * (128.0 / 118.0) * obj.scale * scale * m.pf;
            if !first { out.push(','); }
            first = false;
            out.push_str("{\"obj\":");
            out.push_str(&obj.index.to_string());
            out.push_str(",\"centre\":[");
            out.push_str(&format!("{:.1},{:.1}", centre[0], centre[1]));
            out.push_str("],\"ring_r\":");
            out.push_str(&format!("{:.1}", size * 0.5 * (122.0 / 128.0)));
            out.push_str("}");
        }
        out.push_str("]}
");
        std::fs::write(path, out).expect("write probe json");
    }
}

/// A single ">" chevron centred at `pos`, pointing along `rotation_deg`.
/// Opening ~60 degrees (FontAwesome-style), thin strokes, round caps.
/// `top`/`bottom` colour the two sides across the chevron's local y axis
/// (GradientVertical on the rotated drawable).
pub fn draw_chevron(
    list: &mut DrawList,
    pos: [f32; 2],
    rotation_deg: f32,
    size: f32,
    thickness: f32,
    top: Colour,
    bottom: Colour,
    blend: Blend,
) {
    // FontAwesome `ChevronRight` metrics (viewBox 320x512, path-derived):
    // glyph height 429.4/512 of the sprite box, axial depth 0.605 of the
    // height (arms at atan(0.5/0.605) = 39.6 deg from the axis - the outer
    // edges are exactly 45 deg), stroke 48/512 of the box, rounded tip.
    let (sr, cr) = rotation_deg.to_radians().sin_cos();
    let h = size * (429.4 / 512.0);
    let axial = h * 0.3025; // half of the 0.605H depth
    let half_h = h * 0.5;
    let tip = [pos[0] + cr * axial, pos[1] + sr * axial];
    let top_end = [pos[0] - cr * axial - sr * half_h, pos[1] - sr * axial + cr * half_h];
    let bottom_end = [pos[0] - cr * axial + sr * half_h, pos[1] - sr * axial - cr * half_h];
    list.capsule_gradient(top_end, tip, thickness, top, bottom, blend);
    list.capsule_gradient(tip, bottom_end, thickness, top, bottom, blend);
}

/// osu-framework `Interpolation.Damp(current, target, base, exponent)` as
/// used by the spinner (`Damp(a, b, 0.99, |Time.Elapsed|)`): the exponent
/// is the elapsed MILLISECONDS, so each 60Hz frame closes ~15% of the gap
/// (0.99^16.67 = 0.846) - NOT per-frame.
fn damp_frame(current: f64, target: f64, smoothing: f64, dt_ms: f64) -> f64 {
    target + (current - target) * smoothing.powf(dt_ms)
}

/// osu-framework `Interpolation.DampContinuously`: exponential approach
/// with `half_time_ms` being the time to close half the remaining gap
/// (the ring/side values use a 40ms half-time).
fn damp_half(current: f64, target: f64, half_time_ms: f64, dt_ms: f64) -> f64 {
    target + (current - target) * 0.5f64.powf(dt_ms / half_time_ms)
}

// ---------------------------------------------------------------------------
// Follow points
// ---------------------------------------------------------------------------

fn draw_follow_points(
    legacy: Option<&LegacyCache>,
    game: &GameData,
    m: &Mapper,
    list: &mut DrawList,
    atlas: &Atlas,
    t: f64,
) {
    let objs = &game.objects;
    for w in objs.windows(2) {
        let (start, end) = (&w[0], &w[1]);
        // FollowPointLifetimeEntry.refreshLifetimes: the connection is dead
        // when either object is a spinner OR the end object starts a new
        // combo - no follow points lead into a new combo.
        if start.kind == ObjKind::Spinner
            || end.kind == ObjKind::Spinner
            || end.new_combo
        {
            continue;
        }

        let start_pos = start.end_position;
        let end_pos = end.position;
        let dv = [end_pos[0] - start_pos[0], end_pos[1] - start_pos[1]];
        let distance = (dv[0] * dv[0] + dv[1] * dv[1]).sqrt();
        if distance < FOLLOW_POINT_SPACING * 2.0 {
            continue;
        }
        let rotation = dv[1].atan2(dv[0]).to_degrees();
        let start_time = start.end_time;
        let duration = end.start_time - start_time;
        let preempt = FOLLOW_POINT_PREEMPT * (start.preempt / 450.0).min(1.0);

        let mut d = FOLLOW_POINT_SPACING * 1.5;
        while d < distance - FOLLOW_POINT_SPACING {
            let fraction = d / distance;
            let fade_out = start_time + fraction as f64 * duration;
            let fade_in = fade_out - preempt;

            if t >= fade_in && t <= fade_out + end.fade_in {
                let alpha = value_at(t, fade_in, fade_in + end.fade_in, 0.0, 1.0, Easing::Linear)
                    * value_at(t, fade_out, fade_out + end.fade_in, 1.0, 0.0, Easing::Linear);
                if alpha > 0.003 {
                    let move_t = value_at(t, fade_in, fade_in + end.fade_in, 0.0, 1.0, Easing::Out);
                    let f = fraction as f64 - 0.1 + 0.1 * move_t;
                    let pos = [start_pos[0] + dv[0] * f as f32, start_pos[1] + dv[1] * f as f32];
                    let scale = value_at(t, fade_in, fade_in + end.fade_in, 1.5, 1.0, Easing::Out);
                    let sp = m.pf(pos);

                    // Legacy: the skin's `followpoint` animation, phased
                    // from the follow point's own fade-in
                    // (`FollowPoint.AnimationStartTime = fadeInTime`),
                    // untinted, rotated along the connection.
                    if let Some(lg) = legacy
                        && let Some(anim) = &lg.followpoint
                    {
                        let frame = anim.frame_at(t - fade_in);
                        let w = frame.display_width() * m.pf * end.scale * scale as f32;
                        let h = frame.display_height() * m.pf * end.scale * scale as f32;
                        list.image(atlas, frame.region, sp, [w, h], rotation, Colour::WHITE.opacity(alpha as f32), Blend::Alpha);
                        d += FOLLOW_POINT_SPACING;
                        continue;
                    }

                    let size = 8.0 * m.pf * end.scale * scale as f32;
                    // fa-chevron-right stroke ~47.7/512 of the glyph height.
                    let thickness = size * 0.094;
                    // ArgonFollowPoint: GradientVertical FC618F -> BB1A41 on
                    // the rotated drawable; the shadow chevron's explicit
                    // Gray(0.2) MULTIPLIES into the inherited gradient
                    // (ColourInfo.ApplyChild) = 20% of each channel, which
                    // is exactly `darken(4.0)` (1/(1+4)). Additive blending.
                    let top = Colour::from_hex(0xFC618F).opacity(alpha as f32);
                    let bottom = Colour::from_hex(0xBB1A41).opacity(alpha as f32);
                    let shadow_top = top.darken(4.0);
                    let shadow_bottom = bottom.darken(4.0);
                    let (sr, cr) = rotation.to_radians().sin_cos();
                    let off = size * 0.5;
                    let p2 = [sp[0] + cr * off, sp[1] + sr * off];
                    // Dim pink back chevron + bright front chevron.
                    draw_chevron(list, sp, rotation, size, thickness, shadow_top, shadow_bottom, Blend::Additive);
                    draw_chevron(list, p2, rotation, size, thickness, top, bottom, Blend::Additive);
                }
            }

            d += FOLLOW_POINT_SPACING;
        }
    }
}

// ---------------------------------------------------------------------------
// Judgements
// ---------------------------------------------------------------------------

fn draw_judgement_explosion(list: &mut DrawList, m: &Mapper, ev: &EventView, t: f64) {
    if ev.display != JudgementDisplay::Text || !hit_result_ext::is_hit(ev.result) {
        return;
    }
    let x = t - ev.time;
    if x < 0.0 || x > 1000.0 {
        return;
    }

    let (count_small, count_large, travel) = match ev.result {
        HitResult::Meh => (3usize, 0usize, 52.0f32 * 0.3),
        HitResult::Ok | HitResult::Good => (4, 0, 52.0 * 0.6),
        HitResult::Great | HitResult::Perfect => (4, 4, 52.0),
        _ => return,
    };

    // Deterministic pseudo-random directions per event.
    let mut rng = (ev.time * 1000.0) as u64
        ^ ((ev.position[0] as i64 as u64) << 16)
        ^ ((ev.position[1] as i64 as u64) << 32)
        | 1;
    let mut next_unit = |rng: &mut u64| -> f32 {
        *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*rng >> 33) as f64 / u64::from(u32::MAX >> 1) as f64) as f32
    };

    let colour = colour_for_result(ev.result);
    let fade = value_at(x, 0.0, 1000.0, 1.0, 0.0, Easing::OutQuint) as f32;
    let centre = m.pf(ev.position);
    let unit = m.pf * ev.scale;
    let move_t = value_at(x, 0.0, 600.0, 0.3, 1.0, Easing::OutQuint) as f32;

    for i in 0..count_small + count_large {
        let large = i >= count_small;
        let dir = next_unit(&mut rng) * 360.0f32;
        let dist = (0.5 + next_unit(&mut rng) * 0.5) * travel;
        let d = dist * move_t;
        let rad = dir.to_radians();
        let pos = [centre[0] + rad.cos() * d * unit, centre[1] + rad.sin() * d * unit];
        let r = if large { 7.0 } else { 4.5 };
        list.ring(pos, r * unit, 4.0 * unit, colour.opacity(fade), colour.opacity(fade), Blend::Additive);
    }
}

fn draw_judgement_text(
    legacy: Option<&LegacyCache>,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    ev: &EventView,
    t: f64,
) {
    let x = t - ev.time;
    if x < 0.0 {
        return;
    }

    // Legacy judgement bursts (`LegacyJudgementPieceOld`): fade in over
    // 120ms, fade out from 500ms over 600ms. Multi-frame animations play
    // WITHOUT the scale transforms; single sprites pop: hits 0.6 -> 1.1
    // (0.8x120ms) -> hold -> 0.9 -> 0.95 -> 1.0, misses 1.6 -> 1 over
    // 100ms In with the v2.0+ rise/fall and a random +-8.6 deg tilt.
    // Missed ticks (LargeTickMiss/IgnoreMiss) get the reduced animation.
    if let Some(lg) = legacy
        && let Some(idx) = judgement_index(ev.result)
        && let Some(anim) = &lg.judgement[idx]
    {
        let frame = anim.frame_at(x.max(0.0));
        let fade_in_length = 120.0f64;
        let fade_out_delay = 500.0f64;
        let fade_out_length = 600.0f64;

        let is_miss = !hit_result_ext::is_hit(ev.result);
        let is_missed_tick = is_miss && ev.result != HitResult::Miss;

        let (alpha, fade_end) = if is_missed_tick {
            (value_at(x, 0.0, fade_in_length, 0.0, 1.0, Easing::Linear)
                * value_at(x, fade_out_delay / 2.0, fade_out_delay / 2.0 + fade_out_length, 1.0, 0.0, Easing::Linear),
             fade_out_delay / 2.0 + fade_out_length)
        } else {
            (value_at(x, 0.0, fade_in_length, 0.0, 1.0, Easing::Linear)
                * value_at(x, fade_out_delay, fade_out_delay + fade_out_length, 1.0, 0.0, Easing::Linear),
             fade_out_delay + fade_out_length)
        };
        if alpha <= 0.003 {
            return;
        }
        if x > fade_end {
            return;
        }

        let centre = m.pf(ev.position);
        let unit = m.pf * ev.scale;

        // Single-frame sprites take the scale/rotation transforms;
        // animations only fade.
        let multi_frame = anim.frames.len() > 1;
        let (mut scale, mut offset_y, mut rotation) = (1.0f64, 0.0f64, 0.0f32);
        if !multi_frame {
            if is_miss {
                if is_missed_tick {
                    scale = value_at(x, 0.0, 100.0, 1.2, 1.0, Easing::In);
                } else {
                    scale = value_at(x, 0.0, 100.0, 1.6, 1.0, Easing::In);
                    if lg.version > 1.0 {
                        // Rise from -5 by +80 over the fade-out tail.
                        offset_y = -5.0
                            + value_at(x, 0.0, fade_out_delay + fade_out_length, 0.0, 80.0, Easing::In);
                    }
                    // Deterministic per-event tilt (RNG.NextSingle(-8.6, 8.6)).
                    let mut rng = (ev.time as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
                    rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    let tilt = ((rng >> 33) as f64 / u64::from(u32::MAX >> 1) as f64) as f32 * 17.2 - 8.6;
                    rotation = tilt * value_at(x, 0.0, fade_in_length, 0.0, 1.0, Easing::Linear) as f32
                        + tilt * value_at(x, fade_in_length, fade_out_delay + fade_out_length, 0.0, 1.0, Easing::In) as f32;
                }
            } else {
                // 0.6 -> 1.1 over 96ms, hold to 120ms, 0.9 over 24ms,
                // jump to 0.95, then 1.0 over 24ms.
                scale = if x < 96.0 {
                    value_at(x, 0.0, 96.0, 0.6, 1.1, Easing::Linear)
                } else if x < 120.0 {
                    value_at(x, 96.0, 120.0, 1.1, 0.9, Easing::Linear)
                } else if x < 144.0 {
                    value_at(x, 120.0, 144.0, 0.95, 1.0, Easing::Linear)
                } else {
                    1.0
                };
            }
        }

        let pos = [centre[0], centre[1] + offset_y as f32 * unit];
        let w = frame.display_width() * unit * scale as f32;
        let h = frame.display_height() * unit * scale as f32;
        list.image(assets.atlas, frame.region, pos, [w, h], rotation, Colour::WHITE.opacity(alpha as f32), Blend::Alpha);
        return;
    }

    match ev.display {
        JudgementDisplay::Text => {
            if x > 1800.0 {
                return;
            }
            let colour = colour_for_result(ev.result);
            let centre = m.pf(ev.position);
            let unit = m.pf * ev.scale;

            let is_miss = !hit_result_ext::is_hit(ev.result);
            let (alpha, scale, offset_y) = if is_miss {
                (
                    value_at(x, 0.0, 800.0, 1.0, 0.0, Easing::Linear),
                    value_at(x, 0.0, 100.0, 1.6, 1.0, Easing::In),
                    value_at(x, 0.0, 800.0, 0.0, 100.0, Easing::InQuint),
                )
            } else {
                (
                    value_at(x, 0.0, 800.0, 1.0, 0.0, Easing::Linear),
                    value_at(x, 0.0, 1800.0, 1.0, 1.2, Easing::OutQuint),
                    0.0,
                )
            };
            if alpha <= 0.003 {
                return;
            }

            let pos = [centre[0], centre[1] + offset_y as f32 * unit];
            let text = judgement_word(ev.result);
            draw_ttf_text(
                list,
                assets.atlas,
                assets.bold,
                true,
                text,
                pos,
                20.0 * unit * scale as f32,
                colour.opacity(alpha as f32),
                5.0 * unit,
                Blend::Additive,
            );
        }
        JudgementDisplay::TickMiss => {
            if x > 600.0 {
                return;
            }
            let colour = colour_for_result(ev.result);
            let centre = m.pf(ev.position);
            let unit = m.pf * ev.scale;
            let alpha = value_at(x, 0.0, 600.0, 1.0, 0.0, Easing::Linear);
            let scale = value_at(x, 0.0, 150.0, 1.4, 1.0, Easing::Out);
            if alpha <= 0.003 {
                return;
            }
            list.disc(centre, 6.0 * unit * scale as f32, colour.opacity(alpha as f32), colour.opacity(alpha as f32), Blend::Additive);
        }
        JudgementDisplay::None => {}
    }
}

// ---------------------------------------------------------------------------
// Approach circles
// ---------------------------------------------------------------------------

/// `hd`: HD is active and this is the first adjustable object - the only
/// approach circle kept. Its fade window uses the HD-adjusted TimeFadeIn.
fn draw_approach_circle(
    legacy: Option<&LegacyCache>,
    assets: &Assets,
    list: &mut DrawList,
    m: &Mapper,
    obj: &ObjView,
    t: f64,
    hd: bool,
) {
    let appear = obj.start_time - obj.preempt;
    if t < appear || t > obj.start_time {
        return;
    }
    // Judged only counts once the judgement TIME has passed (the fields are
    // filled statically from the whole timeline).
    let judged = match obj.kind {
        ObjKind::Circle => obj.body_judged.map(|(jt, _)| jt <= t).unwrap_or(false),
        ObjKind::Slider => obj.head_judged.map(|(jt, _)| jt <= t).unwrap_or(false),
        ObjKind::Spinner => false,
    };
    if judged {
        return;
    }

    let fade_in_dur = if hd {
        // `Math.Min(TimeFadeIn * 2, TimePreempt)` with the HD-adjusted
        // TimeFadeIn (preempt * 0.4) = 0.8 * preempt.
        (obj.preempt * HD_FADE_IN_MULTIPLIER * 2.0).min(obj.preempt)
    } else {
        (obj.fade_in * 2.0).min(obj.preempt)
    };
    let alpha = value_at(t, appear, appear + fade_in_dur, 0.0, 0.9, Easing::Linear)
        * value_at(t, obj.start_time - 50.0, obj.start_time, 1.0, 0.0, Easing::Linear);
    if alpha <= 0.003 {
        return;
    }

    // Official approachcircle texture: 128 box * (128/118), scaled 4 -> 1
    // over the preempt, tinted with the combo colour (DisallowZeroAlpha).
    let scale = value_at(t, appear, obj.start_time, 4.0, 1.0, Easing::Linear) as f32;
    let centre = m.pf(obj.position);
    let c = obj.colour;
    let col = Colour::rgb(c.r.max(0.05), c.g.max(0.05), c.b.max(0.05)).opacity(alpha as f32);

    // Legacy: the skin's `approachcircle` sprite at its authored size
    // (`LegacyApproachCircle`, same tint and 4x -> 1x scaling).
    if let Some(lg) = legacy
        && let Some(tex) = lg.approachcircle
    {
        let w = tex.display_width() * obj.scale * m.pf * scale;
        let h = tex.display_height() * obj.scale * m.pf * scale;
        if std::env::var("APPROACH_DEBUG").is_ok() {
            eprintln!(
                "APPROACH obj={} t={} tex={}x{} adjust={} display={:.0} objscale={:.2} pf={:.2} scale={:.2} -> w={:.0} alpha={:.2}",
                obj.index, t, tex.width, tex.height, tex.scale_adjust, tex.display_width(), obj.scale, m.pf, scale, w, alpha
            );
        }
        list.image(assets.atlas, tex.region, centre, [w, h], 0.0, col, Blend::Alpha);
        return;
    }

    // Fallback ring (skin does not provide `approachcircle`). Size it
    // INK-aware: lazer's `DefaultApproachCircle` expands the classic ring
    // (ink 118 of a 128 canvas) by 128/118 so its ink lands on the full
    // 128 clickable circle - but the embedded ring's ink runs to its
    // canvas edge, so applying that expansion blindly overshoots. Target
    // the ink directly instead: stable's default ring sinks onto the
    // circle (ink 118 over the 128 box - a 119px skin circle matches it
    // exactly), lazer's lands on the full 128 for argon's full-size
    // circles.
    let rect = assets.atlas.region_rect(crate::draw::Region::ApproachCircle);
    let ink = assets.atlas.ink(crate::draw::Region::ApproachCircle);
    let ink_ratio = ((ink[2] - ink[0]) / (rect.x1 - rect.x0)).clamp(0.01, 1.0);
    let target_ink = if legacy.is_some() { 118.0 } else { 128.0 };
    let size = target_ink / ink_ratio * obj.scale * scale * m.pf;
    if std::env::var("APPROACH_DEBUG").is_ok() {
        eprintln!(
            "APPROACH-fallback obj={} t={} appear={} start={} preempt={} fade_in={} ink_ratio={:.3} target={} scale={:.2} size={:.0} alpha={:.2} pos={:?}",
            obj.index, t, appear, obj.start_time, obj.preempt, obj.fade_in, ink_ratio, target_ink, scale, size, alpha, obj.position
        );
    }
    list.image(
        assets.atlas,
        crate::draw::Region::ApproachCircle,
        centre,
        [size, size],
        0.0,
        col,
        Blend::Alpha,
    );
}

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

fn draw_cursor(
    legacy: Option<&LegacyCache>,
    assets: &Assets,
    list: &mut DrawList,
    pos: [f32; 2],
    scale: f32,
    user_size: f32,
    virt: f32,
    t: f64,
) {
    // `LegacyCursor`: the `cursor` sprite (the only expanding layer) with
    // `cursormiddle` on top; press pops to 1.3x (100ms Out),
    // `CursorRotate` spins the sprite a revolution per 10s. Sizing per
    // `NonPlayfieldSprite`: the texture's `ScaleAdjust` gains the 1.6
    // stable factor, which CANCELS the playfield's own 1.6 scale - the
    // net on-screen size is the texture's display size in WINDOW units
    // (`display * virt`), not a 50-unit box stretch (the 50 box is just
    // the receptor container; sprites auto-size). `user_size` is the
    // `cursorScaleContainer` Scale (`GameplayCursorSize`), multiplying
    // the whole cursor either skin path.
    if let Some(lg) = legacy
        && let Some(cursor) = lg.cursor
    {
        let rotation = if lg.cursor_rotate {
            ((t % LEGACY_CURSOR_REVOLUTION) / LEGACY_CURSOR_REVOLUTION * 360.0) as f32
        } else {
            0.0
        };
        let draw = |list: &mut DrawList, tex: SkinTexture, expand: f32, rotation: f32| {
            let w = tex.display_width() * virt * expand * user_size;
            let h = tex.display_height() * virt * expand * user_size;
            // `CursorCentre: 0` anchors the sprite's top-left at the
            // cursor position instead of its centre.
            let at = if lg.cursor_centre {
                pos
            } else {
                [pos[0] + w * 0.5, pos[1] + h * 0.5]
            };
            list.image(assets.atlas, tex.region, at, [w, h], rotation, Colour::WHITE, Blend::Alpha);
        };
        draw(list, cursor, scale, rotation);
        if let Some(middle) = lg.cursormiddle {
            draw(list, middle, 1.0, 0.0);
        }
        return;
    }

    // The argon cursor lives INSIDE the playfield (`Playfield` base adds
    // `Cursor` internally), so `OsuCursor.SIZE = 28` is in PLAYFIELD
    // units: on-screen diameter = 28 * pf (= H/480 via the 0.8 x 4:3
    // adjustment chain), scaling with the render resolution. The ring
    // thicknesses and glow radius are local sizes too and scale the same
    // way. Container Scale (user size) multiplies them all.
    let pf = virt * PLAYFIELD_SCALE;
    let r = CURSOR_SIZE * 0.5 * pf * scale * user_size;

    let top = Colour::from_hex(0xFC618F);
    let bottom = Colour::from_hex(0xBB1A41);
    list.ring(pos, r, 6.0 * pf * scale * user_size, top, bottom, Blend::Alpha);

    let fill = top.darken(0.6).opacity(0.4);
    list.disc(pos, r, fill, fill, Blend::Alpha);

    list.ring(pos, r, 2.0 * pf * scale * user_size, Colour::WHITE.opacity(0.8), Colour::WHITE.opacity(0.8), Blend::Alpha);

    list.glow(pos, 20.0 * pf * scale * user_size, Colour::rgba_bytes(171, 255, 255, 100));
    list.disc(pos, r * 0.2, Colour::WHITE, Colour::WHITE, Blend::Alpha);
}
