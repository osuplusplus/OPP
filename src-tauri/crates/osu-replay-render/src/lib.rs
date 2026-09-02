//! Library surface of the replay renderer, used by the CLI (`main.rs`) and
//! by external embedders (e.g. the OPP live preview) that render frames on
//! demand into a window or read pixels back.

pub mod autoplay;
pub mod draw;
pub mod game;
pub mod hitsound;
pub mod hud;
pub mod pp;
pub mod render;
pub mod results;
pub mod scene;
/// Beatmap storyboard layer (`.osu` Events + shared `.osb`): rendered by
/// the osu-storyboard-render library into two offscreen composites
/// (below/above the playfield) and GPU-copied into atlas slots each frame.
pub mod storyboard;
/// osu!(lazer) skinning abstraction port: user skin directories
/// (`--skin <dir>`) with the built-in argon skin as fallback.
pub mod skin;
/// 原生窗口直渲(窗口 surface,跨平台:Windows Win32 / Linux Xlib)。
/// 宿主以 raw window handle 传入自己的窗口;句柄类型经
/// [`raw_window_handle`] 再导出,宿主无需直接依赖该 crate。
pub mod surface;

pub use raw_window_handle;

use draw::{Atlas, Image, Region, TtfFont};
use skin::SkinTexture;

const TORUS_BOLD_FONT: &[u8] = include_bytes!("../assets/fonts/Torus-Bold.otf");
const TORUS_SEMI_BOLD_FONT: &[u8] = include_bytes!("../assets/fonts/Torus-SemiBold.otf");
/// Torus Light: the expanded panel's score counter (`TotalScoreCounter`,
/// Torus 60 Light fixedWidth).
const TORUS_LIGHT_FONT: &[u8] = include_bytes!("../assets/fonts/Torus-Light.otf");
/// Torus Regular: the judgement/statistic counter values
/// (`StatisticCounter`, Torus 20 fixedWidth).
const TORUS_REGULAR_FONT: &[u8] = include_bytes!("../assets/fonts/Torus-Regular.otf");
/// Venera: the rank letter's typeface (`RankText`, OsuFont.Numeric Bold;
/// the official ppy distribution is Venera 500 from osu-web).
const VENERA_FONT: &[u8] = include_bytes!("../assets/fonts/Venera-500.otf");

/// Glyph-region weights (`Region::Glyph::weight`).
pub const WEIGHT_SEMIBOLD: u8 = 0;
pub const WEIGHT_BOLD: u8 = 1;
pub const WEIGHT_LIGHT: u8 = 2;
pub const WEIGHT_VENERA: u8 = 3;
pub const WEIGHT_REGULAR: u8 = 4;

/// The four text fonts packed into the atlas (Torus family + Venera for
/// the rank letter).
#[derive(Clone)]
pub struct Fonts {
    /// Torus Bold (emphasis, avatar placeholder).
    pub bold: TtfFont,
    /// Torus SemiBold (labels, counters).
    pub semibold: TtfFont,
    /// Torus Light: the score counter (`TotalScoreCounter`, Torus 60 Light).
    pub light: TtfFont,
    /// Venera: the rank letter (`RankText`, OsuFont.Numeric).
    pub venera: TtfFont,
    /// Torus Regular: the judgement/statistic counter values.
    pub regular: TtfFont,
}
const CURSOR_TRAIL_PNG: &[u8] = include_bytes!("../assets/cursor/cursortrail.png");

/// Built-in lazer mod icons (`osu-resources` `Textures/Icons/Mods`,
/// MIT-licensed). Index order matches `Region::ModIcon`.
pub const MOD_ICON_NAMES: [&str; 14] = [
    "easy",
    "no-fail",
    "hidden",
    "hard-rock",
    "sudden-death",
    "perfect",
    "double-time",
    "nightcore",
    "half-time",
    "flashlight",
    "spun-out",
    "classic",
    "score-v2",
    "touch-device",
];
const MOD_ICON_PNGS: [&[u8]; 14] = [
    include_bytes!("../assets/modicons/mod-easy.png"),
    include_bytes!("../assets/modicons/mod-no-fail.png"),
    include_bytes!("../assets/modicons/mod-hidden.png"),
    include_bytes!("../assets/modicons/mod-hard-rock.png"),
    include_bytes!("../assets/modicons/mod-sudden-death.png"),
    include_bytes!("../assets/modicons/mod-perfect.png"),
    include_bytes!("../assets/modicons/mod-double-time.png"),
    include_bytes!("../assets/modicons/mod-nightcore.png"),
    include_bytes!("../assets/modicons/mod-half-time.png"),
    include_bytes!("../assets/modicons/mod-flashlight.png"),
    include_bytes!("../assets/modicons/mod-spun-out.png"),
    include_bytes!("../assets/modicons/mod-classic.png"),
    include_bytes!("../assets/modicons/mod-score-v2.png"),
    include_bytes!("../assets/modicons/mod-touch-device.png"),
];
const MOD_ICON_BG_PNG: &[u8] = include_bytes!("../assets/modicons/mod-icon.png");
const REPEAT_EDGE_PNG: &[u8] = include_bytes!("../assets/cursor/repeat-edge-piece.png");
const APPROACH_CIRCLE_PNG: &[u8] = include_bytes!("../assets/cursor/approachcircle.png");

const COUNTER_DIGITS: [&[u8]; 10] = [
    include_bytes!("../assets/counter/argon-counter-0.png"),
    include_bytes!("../assets/counter/argon-counter-1.png"),
    include_bytes!("../assets/counter/argon-counter-2.png"),
    include_bytes!("../assets/counter/argon-counter-3.png"),
    include_bytes!("../assets/counter/argon-counter-4.png"),
    include_bytes!("../assets/counter/argon-counter-5.png"),
    include_bytes!("../assets/counter/argon-counter-6.png"),
    include_bytes!("../assets/counter/argon-counter-7.png"),
    include_bytes!("../assets/counter/argon-counter-8.png"),
    include_bytes!("../assets/counter/argon-counter-9.png"),
];
const COUNTER_DOT_PNG: &[u8] = include_bytes!("../assets/counter/argon-counter-dot.png");

/// Lazer `ResultsScreen.BACKGROUND_BLUR` (blur sigma) expressed in
/// virtual 1024x768 canvas units: 10 screen px at 1080p is
/// `10 * 768 / 1080` units (the canvas scale is height-bound at 16:9).
const RESULTS_BG_BLUR_UNITS: f32 = 10.0 * 768.0 / 1080.0;

/// Separable 3-pass box blur — the standard fast gaussian approximation.
/// `sigma` is in image pixels; the buffer is RGBA8.
fn blur_image(img: &Image, sigma: f32) -> Image {
    let mut out = img.rgba.clone();
    if sigma <= 0.05 || img.width < 2 || img.height < 2 {
        return Image { width: img.width, height: img.height, rgba: out };
    }
    // Box half-width whose triple pass matches the gaussian: one box pass
    // over [-r, r] has variance r(r+2)/6, three passes r(r+2)/2.
    let r = (((2.0 * sigma * sigma + 1.0).sqrt()) as usize).max(1);
    let (w, h) = (img.width as usize, img.height as usize);
    let mut tmp = vec![0u8; out.len()];
    for _ in 0..3 {
        box_blur_axis(&out, &mut tmp, w, h, r, true);
        box_blur_axis(&tmp, &mut out, w, h, r, false);
    }
    Image { width: img.width, height: img.height, rgba: out }
}

/// One box-blur pass along x (`horizontal`) or y on RGBA8, edge-clamped
/// (sliding window sum, O(1) per pixel).
fn box_blur_axis(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let div = (2 * r + 1) as u32;
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    for line in 0..lines {
        let base = if horizontal { line * w * 4 } else { line * 4 };
        let stride = if horizontal { 4 } else { w * 4 };
        for c in 0..4 {
            let at = |i: usize| src[base + i * stride + c] as u32;
            let mut sum: u32 = (0..=r).map(|i| at(i.min(len - 1))).sum();
            sum += at(0) * r as u32;
            for i in 0..len {
                dst[base + i * stride + c] = (sum / div) as u8;
                let add = at((i + r + 1).min(len - 1));
                let sub = at(i.saturating_sub(r));
                sum = sum + add - sub;
            }
        }
    }
}

/// Prepares a custom results-screen avatar: cover-crop to a square and
/// pre-mask the rounded corners (radius = `corner_ratio` of the side;
/// the renderer draws it over the placeholder box whose 80-unit square
/// uses `CORNER_RADIUS` 20, hence 0.25).
pub fn rounded_avatar(img: &Image, corner_ratio: f32) -> Image {
    // Cover-crop to a square (centre).
    let side = img.width.min(img.height) as i64;
    let x0 = (img.width as i64 - side) / 2;
    let y0 = (img.height as i64 - side) / 2;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    for row in 0..side {
        let src = ((y0 + row) * img.width as i64 + x0) as usize * 4;
        let dst = (row * side) as usize * 4;
        rgba[dst..dst + side as usize * 4].copy_from_slice(&img.rgba[src..src + side as usize * 4]);
    }
    // Rounded-corner alpha mask (per-pixel distance to the corner arc).
    let r = (side as f32 * corner_ratio).min(side as f32 * 0.5);
    for row in 0..side {
        for col in 0..side {
            let (x, y) = (col as f32 + 0.5, row as f32 + 0.5);
            // The nearest corner-arc centre: the point clamped into
            // [r, side - r] on both axes.
            let (cx, cy) = (x.min(side as f32 - r).max(r), y.min(side as f32 - r).max(r));
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            if d > r {
                let a = ((r + 1.0 - d).clamp(0.0, 1.0) * 255.0) as u32;
                let i = (row * side + col) as usize * 4 + 3;
                rgba[i] = (rgba[i] as u32 * a / 255) as u8;
            }
        }
    }
    Image { width: side as u32, height: side as u32, rgba }
}
const COUNTER_PERCENT_PNG: &[u8] = include_bytes!("../assets/counter/argon-counter-percentage.png");
const COUNTER_X_PNG: &[u8] = include_bytes!("../assets/counter/argon-counter-x.png");
const COUNTER_WIREFRAMES_PNG: &[u8] = include_bytes!("../assets/counter/argon-counter-wireframes.png");

/// Atlas slots reserved for the storyboard composites (`--storyboard`):
/// two full-frame regions the storyboard layer renders into GPU-side
/// every frame (below = Background/Fail/Pass, above = Foreground/Overlay
/// when present). Sized to the output resolution; larger renders sample
/// the slots upscaled rather than growing the atlas.
#[derive(Clone, Copy, Debug)]
pub struct StoryboardSlots {
    pub width: u32,
    pub height: u32,
    /// Reserve the above-playfield slot too (the map's storyboard has
    /// Foreground/Overlay elements).
    pub foreground: bool,
}

/// Builds the atlas (fonts, counter digits, cursor pieces, skin
/// textures, optional background) plus the two font wrappers needed by
/// the scene builder. Skin textures are packed as `Region::Skin(i)`
/// entries and their handles handed back to the skin so its
/// `get_texture` serves atlas regions.
/// BREAKING (0.7.0): returns `(Atlas, Fonts)` — the extra Torus Light /
/// Venera fonts back the results screen's score counter and rank letter.
/// `avatar_image` (`--avatar` / config `avatar`) is packed as
/// `Region::Avatar`: cover-cropped square, pre-masked rounded corners.
/// `storyboard` (`--storyboard`) reserves the full-frame storyboard
/// composite slots (`Region::Storyboard` [+ `StoryboardForeground`]).
pub fn build_atlas(
    bg_image: Option<Image>,
    avatar_image: Option<Image>,
    skin: &mut dyn skin::SkinTextureSource,
    max_dim: u32,
    storyboard: Option<StoryboardSlots>,
) -> (Atlas, Fonts) {
    let (mut bold, mut bold_images) = TtfFont::rasterize(TORUS_BOLD_FONT, WEIGHT_BOLD);
    let (mut semibold, mut semibold_images) = TtfFont::rasterize(TORUS_SEMI_BOLD_FONT, WEIGHT_SEMIBOLD);
    let (mut light, mut light_images) = TtfFont::rasterize(TORUS_LIGHT_FONT, WEIGHT_LIGHT);
    let (mut venera, mut venera_images) = TtfFont::rasterize(VENERA_FONT, WEIGHT_VENERA);
    let (mut regular, mut regular_images) = TtfFont::rasterize(TORUS_REGULAR_FONT, WEIGHT_REGULAR);

    let mut images: Vec<(Region, Image)> = Vec::new();
    if let Some(img) = bg_image {
        // Results-screen copy: lazer's `ResultsScreen` blurs the beatmap
        // background with `BACKGROUND_BLUR = 10` (the framework blur
        // SIGMA in screen px; 10 px at 1080p = 7.11 virtual units) and
        // fades it to `Gray(0.5)`. The blur is baked here in the image's
        // own pixels so the drawn sigma is that many virtual units.
        let sigma = RESULTS_BG_BLUR_UNITS * img.width as f32 / 1365.3333;
        let blurred = blur_image(&img, sigma);
        images.push((Region::Background, img));
        images.push((Region::BackgroundBlurred, blurred));
    }
    if let Some(avatar) = avatar_image {
        images.push((Region::Avatar, rounded_avatar(&avatar, 0.25)));
    }
    // Storyboard composite slots: transparent placeholders — the layer
    // overwrites them GPU-side every frame (copy_texture_to_texture).
    if let Some(slots) = storyboard {
        let blank = |w, h| Image { width: w, height: h, rgba: vec![0u8; (w * h * 4) as usize] };
        images.push((Region::Storyboard, blank(slots.width, slots.height)));
        if slots.foreground {
            images.push((Region::StoryboardForeground, blank(slots.width, slots.height)));
        }
    }
    images.append(&mut bold_images);
    images.append(&mut semibold_images);
    images.append(&mut light_images);
    images.append(&mut venera_images);
    images.append(&mut regular_images);

    for (d, png) in COUNTER_DIGITS.iter().enumerate() {
        let (w, h, rgba) = decode_png_bytes(png).expect("embedded png");
        images.push((Region::CounterDigit(b'0' + d as u8), Image { width: w, height: h, rgba }));
    }
    for (png, region) in [
        (COUNTER_DOT_PNG, Region::CounterDot),
        (COUNTER_PERCENT_PNG, Region::CounterPercent),
        (COUNTER_X_PNG, Region::CounterX),
        (COUNTER_WIREFRAMES_PNG, Region::CounterWireframes),
    ] {
        let (w, h, rgba) = decode_png_bytes(png).expect("embedded png");
        images.push((region, Image { width: w, height: h, rgba }));
    }
    {
        let (w, h, rgba) = decode_png_bytes(CURSOR_TRAIL_PNG).expect("embedded png");
        images.push((Region::CursorTrail, Image { width: w, height: h, rgba }));
    }
    {
        let (w, h, rgba) = decode_png_bytes(REPEAT_EDGE_PNG).expect("embedded png");
        images.push((Region::RepeatEdge, Image { width: w, height: h, rgba }));
    }
    {
        let (w, h, rgba) = decode_png_bytes(APPROACH_CIRCLE_PNG).expect("embedded png");
        images.push((Region::ApproachCircle, Image { width: w, height: h, rgba }));
    }
    for (i, png) in MOD_ICON_PNGS.iter().enumerate() {
        let (w, h, rgba) = decode_png_bytes(png).expect("embedded mod icon png");
        images.push((Region::ModIcon(i as u16), Image { width: w, height: h, rgba }));
    }
    {
        let (w, h, rgba) = decode_png_bytes(MOD_ICON_BG_PNG).expect("embedded mod icon bg png");
        images.push((Region::ModIconBg, Image { width: w, height: h, rgba }));
    }

    // Skin textures (`--skin <dir>` / built-in argon sprites): decode,
    // pack, then hand the atlas handles back to the skin.
    // Skin textures (`--skin <dir>` / built-in argon sprites): decode,
    // pack, then hand the atlas handles back to the skin. Packing is
    // capped at `max_dim` per axis (the GPU texture limit of the target
    // device): the packer first widens 4096 -> max_dim, and a skin that
    // still overflows is uniformly downscaled (lazer's
    // `MaxDimensionLimitedTextureLoaderStore` semantics: the display size
    // shrinks with the pixels) until it fits.
    let skin_images = skin.texture_images();
    let mut scale = 1.0f32;
    let (atlas, skin_regions) = loop {
        let mut images = images.clone();
        let mut skin_regions: Vec<(String, SkinTexture)> = Vec::with_capacity(skin_images.len());
        for (i, (name, img)) in skin_images.iter().enumerate() {
            let region = Region::Skin(i as u32);
            let scaled = if scale >= 1.0 { img.clone() } else { skin::legacy::downscale(img, scale) };
            images.push((region, scaled.clone()));
            skin_regions.push((
                name.clone(),
                SkinTexture { region, width: scaled.width, height: scaled.height, scale_adjust: 1.0 },
            ));
        }
        match Atlas::try_build(&images, max_dim) {
            Some(atlas) => break (atlas, skin_regions),
            None => {
                let new_scale = scale * 0.9;
                eprintln!(
                    "atlas: {}x{} overflow at max_dim {max_dim}, downscaling skin textures to {:.0}%",
                    max_dim, max_dim, new_scale * 100.0
                );
                scale = new_scale;
            }
        }
    };
    skin.assign_regions(&skin_regions);
    if std::env::var("ATLAS_DEBUG").is_ok() {
        for r in [Region::CounterDigit(b'5'), Region::Glyph { weight: WEIGHT_BOLD, c: 'G', em: 24 }, Region::Glyph { weight: WEIGHT_BOLD, c: 'G', em: 96 }, Region::Glyph { weight: WEIGHT_SEMIBOLD, c: '5', em: 48 }, Region::CounterWireframes] {
            let rect = atlas.region_rect(r);
            let ink = atlas.ink(r);
            eprintln!("ATLAS {:?}: rect=({:.0},{:.0},{:.0},{:.0}) ink=({:.0},{:.0},{:.0},{:.0})", r, rect.x0, rect.y0, rect.x1, rect.y1, ink[0], ink[1], ink[2], ink[3]);
        }
    }
    bold.patch_rects(&atlas, WEIGHT_BOLD);
    semibold.patch_rects(&atlas, WEIGHT_SEMIBOLD);
    light.patch_rects(&atlas, WEIGHT_LIGHT);
    venera.patch_rects(&atlas, WEIGHT_VENERA);
    regular.patch_rects(&atlas, WEIGHT_REGULAR);
    if std::env::var("ATLAS_DUMP").is_ok() {
        let file = std::fs::File::create("atlas_dump.png").unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), atlas.width, atlas.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&atlas.rgba).unwrap();
        eprintln!("atlas dumped: {}x{}", atlas.width, atlas.height);
    }
    (atlas, Fonts { bold, semibold, light, venera, regular })
}

/// Decodes a PNG or JPEG file into an atlas image (RGBA).
///
/// Malformed or placeholder-empty files (some skins ship 0-byte pngs to
/// disable an element) surface as `Err` instead of panicking - the
/// framework's `TextureLoaderStore.Get` swallows decode exceptions and
/// reports the texture as missing, so callers skip them the same way.
/// ImageSharp detects the format from the file content rather than the
/// extension, so a png-extension miss retries as jpeg.
pub fn decode_image_file(path: &std::path::Path) -> Result<Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let lower = path.to_string_lossy().to_lowercase();
    if !lower.ends_with(".png") {
        if let Ok(img) = decode_jpeg_bytes(&bytes) {
            return Ok(img);
        }
    }
    match decode_png_bytes(&bytes) {
        Ok((w, h, rgba)) => Ok(Image { width: w, height: h, rgba }),
        Err(png_err) => decode_jpeg_bytes(&bytes)
            .map_err(|jpeg_err| format!("{}: png: {}; jpeg: {}", path.display(), png_err, jpeg_err)),
    }
}

fn decode_jpeg_bytes(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("jpeg missing info")?;
    let (w, h) = (info.width as u32, info.height as u32);
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => {
            let mut v = Vec::with_capacity((w * h * 4) as usize);
            for px in &pixels {
                v.extend_from_slice(&[*px, *px, *px, 255]);
            }
            v
        }
        jpeg_decoder::PixelFormat::RGB24 => {
            let mut v = Vec::with_capacity((w * h * 4) as usize);
            for px in pixels.chunks_exact(3) {
                v.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            v
        }
        other => return Err(format!("unsupported jpeg pixel format {:?}", other)),
    };
    Ok(Image { width: w, height: h, rgba })
}

pub fn decode_png_bytes(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // EXPAND: paletted (Indexed) skins decode to RGB, their tRNS chunks
    // to a real alpha channel - old skins ship both.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    // 16-bit PNGs (Photoshop-exported skins ship them, e.g. whole
    // scoreentry sets) decode to 2-byte samples; EXPAND does not reduce
    // bit depth. Take the high byte of each sample and expand to 8-bit
    // RGBA, exactly like the 8-bit paths below.
    let sixteen = info.bit_depth == png::BitDepth::Sixteen;
    match info.color_type {
        png::ColorType::Rgba if !sixteen => Ok((w, h, buf)),
        png::ColorType::Rgba => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(8) {
                rgba.extend_from_slice(&[px[0], px[2], px[4], px[6]]);
            }
            Ok((w, h, rgba))
        }
        png::ColorType::GrayscaleAlpha if !sixteen => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            Ok((w, h, rgba))
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(4) {
                rgba.extend_from_slice(&[px[0], px[0], px[2], px[2]]);
            }
            Ok((w, h, rgba))
        }
        png::ColorType::Grayscale if !sixteen => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in &buf {
                rgba.extend_from_slice(&[*px, *px, *px, 255]);
            }
            Ok((w, h, rgba))
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], 255]);
            }
            Ok((w, h, rgba))
        }
        png::ColorType::Rgb if !sixteen => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            Ok((w, h, rgba))
        }
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(6) {
                rgba.extend_from_slice(&[px[0], px[2], px[4], 255]);
            }
            Ok((w, h, rgba))
        }
        other => Err(format!(
            "unsupported png colour type {:?} depth {:?}",
            other, info.bit_depth
        )),
    }
}
