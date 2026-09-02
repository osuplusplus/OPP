//! Draw primitives, colours, easings, fonts and the texture atlas.
//! Everything is CPU-side scene description; `render.rs` turns a `DrawList`
//! into an offscreen wgpu frame.

use std::collections::HashMap;

use ab_glyph::{Font as _, ScaleFont as _};

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[allow(dead_code)]
impl Colour {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Colour {
        Colour { r, g, b, a: 1.0 }
    }
    pub const WHITE: Colour = Colour { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Colour = Colour { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

    pub fn from_hex(hex: u32) -> Colour {
        Colour {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub fn from_bytes(rgb: [u8; 3]) -> Colour {
        Colour {
            r: rgb[0] as f32 / 255.0,
            g: rgb[1] as f32 / 255.0,
            b: rgb[2] as f32 / 255.0,
            a: 1.0,
        }
    }

    pub fn rgba_bytes(r: u8, g: u8, b: u8, a: u8) -> Colour {
        Colour {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// osu-framework `Color4Extensions.Darken(amount)`:
    /// `Multiply(colour, 1 / (1 + amount))` - a multiplicative darkening
    /// that KEEPS the hue (Darken(4) = 20% of the colour, a dark tint -
    /// NOT the old osuTK `1 - amount` semantics which clamp to black).
    pub fn darken(self, amount: f32) -> Colour {
        let f = 1.0 / (1.0 + amount);
        Colour { r: self.r * f, g: self.g * f, b: self.b * f, a: self.a }
    }

    /// `LegacyDrawableSliderPath.lighten`: "lightens in a way more
    /// friendly to dark or strong colours" - `amount *= 0.5`, then each
    /// channel is `min(1, c * (1 + 0.5 * amount) + amount)` in sRGB
    /// (additive lift, keeps alpha).
    pub fn lighten(self, amount: f32) -> Colour {
        let amount = amount * 0.5;
        Colour {
            r: (self.r * (1.0 + 0.5 * amount) + amount).min(1.0),
            g: (self.g * (1.0 + 0.5 * amount) + amount).min(1.0),
            b: (self.b * (1.0 + 0.5 * amount) + amount).min(1.0),
            a: self.a,
        }
    }

    pub fn opacity(self, a: f32) -> Colour {
        Colour { a: self.a * a, ..self }
    }

    pub fn lerp(a: Colour, b: Colour, t: f32) -> Colour {
        Colour {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }

    /// Gamma-correct interpolation in linear RGB space (framework
    /// `Interpolation.ValueAt(Color4, Color4)`: sRGB -> linear -> lerp ->
    /// sRGB). The alpha channel lerps linearly.
    pub fn lerp_linear(a: Colour, b: Colour, t: f32) -> Colour {
        fn to_lin(c: f32) -> f32 {
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        fn to_srgb(c: f32) -> f32 {
            if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
        }
        Colour {
            r: to_srgb(to_lin(a.r) + (to_lin(b.r) - to_lin(a.r)) * t),
            g: to_srgb(to_lin(a.g) + (to_lin(b.g) - to_lin(a.g)) * t),
            b: to_srgb(to_lin(a.b) + (to_lin(b.b) - to_lin(a.b)) * t),
            a: a.a + (b.a - a.a) * t,
        }
    }

    pub fn with_alpha(self, a: f32) -> Colour {
        Colour { a, ..self }
    }
}

// ---------------------------------------------------------------------------
// Easings (osu-framework DefaultEasingFunction ports)
// ---------------------------------------------------------------------------

const ELASTIC_CONST: f64 = 2.0 * std::f64::consts::PI / 0.3;
const ELASTIC_CONST2: f64 = 0.3 / 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    In,
    Out,
    InQuad,
    OutQuad,
    InCubic,
    OutCubic,
    InQuart,
    OutQuart,
    InQuint,
    OutQuint,
    InSine,
    OutSine,
    InExpo,
    OutExpo,
    OutElasticHalf,
    OutElasticQuarter,
    OutBack,
    OutPow10,
}

impl Easing {
    pub fn apply(self, time: f64) -> f64 {
        let time = time.clamp(0.0, 1.0);
        match self {
            Easing::Linear => time,
            Easing::In | Easing::InQuad => time * time,
            Easing::Out | Easing::OutQuad => time * (2.0 - time),
            Easing::InCubic => time * time * time,
            Easing::OutCubic => {
                let t = time - 1.0;
                t * t * t + 1.0
            }
            Easing::InQuart => time * time * time * time,
            Easing::OutQuart => {
                let t = time - 1.0;
                1.0 - t * t * t * t
            }
            Easing::InQuint => time * time * time * time * time,
            Easing::OutQuint => {
                let t = time - 1.0;
                t * t * t * t * t + 1.0
            }
            Easing::InSine => 1.0 - (time * std::f64::consts::PI * 0.5).cos(),
            Easing::OutSine => (time * std::f64::consts::PI * 0.5).sin(),
            Easing::InExpo => 2.0f64.powf(10.0 * (time - 1.0)),
            Easing::OutExpo => 1.0 - 2.0f64.powf(-10.0 * time),
            Easing::OutElasticHalf => {
                let offset = 2.0f64.powf(-10.0) * ((0.5 - ELASTIC_CONST2) * ELASTIC_CONST).sin();
                2.0f64.powf(-10.0 * time)
                    * ((0.5 * time - ELASTIC_CONST2) * ELASTIC_CONST).sin()
                    + 1.0
                    - offset * time
            }
            Easing::OutElasticQuarter => {
                let offset = 2.0f64.powf(-10.0) * ((0.25 - ELASTIC_CONST2) * ELASTIC_CONST).sin();
                2.0f64.powf(-10.0 * time)
                    * ((0.25 * time - ELASTIC_CONST2) * ELASTIC_CONST).sin()
                    + 1.0
                    - offset * time
            }
            Easing::OutBack => {
                const BACK: f64 = 1.70158;
                let t = time - 1.0;
                t * t * ((BACK + 1.0) * t + BACK) + 1.0
            }
            Easing::OutPow10 => {
                let t = time - 1.0;
                t * t.powf(10.0) + 1.0
            }
        }
    }
}

/// Value of a transform over time: `from` at `start`, `to` at `end`.
pub fn value_at(t: f64, start: f64, end: f64, from: f64, to: f64, easing: Easing) -> f64 {
    if end <= start {
        return if t >= end { to } else { from };
    }
    let p = ((t - start) / (end - start)).clamp(0.0, 1.0);
    from + (to - from) * easing.apply(p)
}

/// Linear interpolation helper for f32.
pub fn lerp(a: f32, b: f32, t: f64) -> f32 {
    (a + (b - a) * t as f32) as f32
}

/// osu-framework `Interpolation.Damp` (exponential smoothing toward target).
pub fn damp(current: f64, target: f64, smoothing: f64, dt: f64) -> f64 {
    // Damp(value, target, smoothing, elapsedTime) with smoothing per second.
    target + (current - target) * (-smoothing * dt / 1000.0).exp()
}

// ---------------------------------------------------------------------------
// Vertices / draw list
// ---------------------------------------------------------------------------

pub const MODE_TEXTURED: f32 = 0.0;
pub const MODE_RING: f32 = 1.0;
pub const MODE_DISC: f32 = 2.0;
pub const MODE_GLOW: f32 = 3.0;
pub const MODE_STROKE: f32 = 4.0;
pub const MODE_CAPSULE: f32 = 5.0;
pub const MODE_FLAT: f32 = 6.0;
pub const MODE_ARC: f32 = 7.0;
pub const MODE_CAPDISC: f32 = 8.0;
pub const MODE_GLOWRING: f32 = 9.0;
/// Rounded rectangle: aux.y = corner radius, colour2.xy = half extents
/// (local space is centred). Corner colours give the vertical gradient.
pub const MODE_ROUNDED: f32 = 11.0;
pub const MODE_GLOWFILL: f32 = 10.0;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub local: [f32; 2],
    pub color: [f32; 4],
    pub color2: [f32; 4],
    pub uv: [f32; 4],
    pub aux: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Blend {
    #[default]
    Alpha,
    Additive,
}

/// A slider body to render via the distance-field prepass: the union of
/// capsule segments, coloured in a single composite sample (lazer's
/// `PathDrawNode` approach - no joins, no seams, no alpha compositing).
pub struct BodyDraw {
    /// Segment endpoints in screen pixels.
    pub segments: Vec<([f32; 2], [f32; 2])>,
    pub radius: f32,
    pub border: f32,
    pub body: Colour,
    pub border_colour: Colour,
    /// Legacy skin gradient (`LegacyDrawableSliderPath.ColourAt`): the
    /// inner end of the radial body gradient (`lighten(accent, 0.5)`),
    /// with `body` as the outer end (`accent.Darken(0.1)`). None renders
    /// the flat two-band body.
    pub inner_colour: Option<Colour>,
}

#[allow(dead_code)]
#[derive(Default)]
pub struct DrawList {
    /// Global alpha multiplier applied to every pushed vertex (scene
    /// transitions: the results screen cross-fades over the final
    /// gameplay frame).
    pub global_alpha: f32,
    pub bodies: Vec<BodyDraw>,
    /// Draw-order anchors for bodies: (index-stream position, body index).
    /// Recorded where the body is pushed during scene construction so the
    /// renderer can interleave each body's composite pass between the scene
    /// runs (lazer layers whole sliders by start time: an earlier slider's
    /// body covers later objects).
    pub body_marks: Vec<(u32, usize)>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Runs of indices with the same blend mode, in draw order.
    pub runs: Vec<(Blend, u32, u32)>, // (blend, index offset, count)
    cur_blend: Blend,
    run_start: u32,
    run_len: u32,
}

impl DrawList {
    pub fn new() -> DrawList {
        DrawList {
            global_alpha: 1.0,
            bodies: Vec::new(),
            body_marks: Vec::new(),
            vertices: Vec::with_capacity(1 << 16),
            indices: Vec::with_capacity(1 << 17),
            runs: Vec::new(),
            cur_blend: Blend::Alpha,
            run_start: 0,
            run_len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.bodies.clear();
        self.body_marks.clear();
        self.vertices.clear();
        self.indices.clear();
        self.runs.clear();
        self.run_start = 0;
        self.run_len = 0;
    }

    /// Marks the just-pushed body (`bodies.len() - 1`) at the current point
    /// of the index stream.
    pub fn mark_body(&mut self) {
        let pos = self.run_start + self.run_len;
        self.body_marks.push((pos, self.bodies.len() - 1));
    }

    pub fn set_blend(&mut self, blend: Blend) {
        if blend != self.cur_blend {
            self.flush_run();
            self.cur_blend = blend;
        }
    }

    fn flush_run(&mut self) {
        if self.run_len > 0 {
            self.runs.push((self.cur_blend, self.run_start, self.run_len));
            self.run_start += self.run_len;
            self.run_len = 0;
        }
    }

    pub fn finish(&mut self) {
        self.flush_run();
    }

    fn quad(&mut self, mut v: [Vertex; 4]) {
        let a = self.global_alpha;
        if a < 1.0 {
            for vtx in &mut v {
                vtx.color[3] *= a;
                vtx.color2[3] *= a;
            }
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&v);
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        self.run_len += 6;
    }

    fn tri(&mut self, mut v: [Vertex; 3]) {
        let a = self.global_alpha;
        if a < 1.0 {
            for vtx in &mut v {
                vtx.color[3] *= a;
                vtx.color2[3] *= a;
            }
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&v);
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
        self.run_len += 3;
    }

    // -- textured quad (atlas region) ------------------------------------

    pub fn image(
        &mut self,
        atlas: &Atlas,
        region: Region,
        center: [f32; 2],
        size: [f32; 2],
        rotation_deg: f32,
        colour: Colour,
        blend: Blend,
    ) {
        self.image_sub(atlas, region, center, size, rotation_deg, colour, blend, 0.0, 0.0, 1.0, 1.0);
    }

    /// Textured quad restricted to a sub-rectangle of the region (u/v in
    /// 0..1 of the region, v0 top). Used for masked legacy sprites (the
    /// spinner metre reveals top-down).
    pub fn image_sub(
        &mut self,
        atlas: &Atlas,
        region: Region,
        center: [f32; 2],
        size: [f32; 2],
        rotation_deg: f32,
        colour: Colour,
        blend: Blend,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
    ) {
        let rect = atlas.region_rect(region);
        let aw = atlas.width as f32;
        let ah = atlas.height as f32;
        let rw = rect.x1 - rect.x0;
        let rh = rect.y1 - rect.y0;
        let rect = Rect {
            x0: (rect.x0 + rw * u0) / aw,
            y0: (rect.y0 + rh * v0) / ah,
            x1: (rect.x0 + rw * u1) / aw,
            y1: (rect.y0 + rh * v1) / ah,
        };
        let (s, c) = rotation_deg.to_radians().sin_cos();
        let rot = |p: [f32; 2]| -> [f32; 2] {
            [center[0] + p[0] * c - p[1] * s, center[1] + p[0] * s + p[1] * c]
        };
        let hx = size[0] * 0.5;
        let hy = size[1] * 0.5;
        let corners = [
            ([-hx, -hy], [rect.x0, rect.y0]),
            ([hx, -hy], [rect.x1, rect.y0]),
            ([hx, hy], [rect.x1, rect.y1]),
            ([-hx, hy], [rect.x0, rect.y1]),
        ];
        let mk = |(local, uv): ([f32; 2], [f32; 2])| Vertex {
            pos: rot(local),
            local: [0.0; 2],
            color: [colour.r, colour.g, colour.b, colour.a],
            color2: [0.0; 4],
            uv: [uv[0], uv[1], 0.0, 0.0],
            aux: [MODE_TEXTURED, 0.0, 0.0, 0.0],
        };
        let v: [Vertex; 4] = std::array::from_fn(|i| mk(corners[i]));
        self.set_blend(blend);
        self.quad(v);
    }

    // -- SDF shapes -------------------------------------------------------

    /// Annulus (ring) with a vertical colour gradient.
    pub fn ring(
        &mut self,
        center: [f32; 2],
        outer_radius: f32,
        thickness: f32,
        top: Colour,
        bottom: Colour,
        blend: Blend,
    ) {
        let r = outer_radius + 1.5;
        let corner = |p: [f32; 2], col: Colour| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [col.r, col.g, col.b, col.a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_RING, outer_radius, thickness, 0.0],
        };
        self.set_blend(blend);
        self.quad([
            corner([-r, -r], top),
            corner([r, -r], top),
            corner([r, r], bottom),
            corner([-r, r], bottom),
        ]);
    }

    /// Rounded rectangle centred at `center`, vertical colour gradient
    /// (framework `Container` + `CornerRadius` masking).
    pub fn rounded_rect(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        radius: f32,
        top: Colour,
        bottom: Colour,
        blend: Blend,
    ) {
        let hw = size[0] * 0.5;
        let hh = size[1] * 0.5;
        // Pad the quad past the corners so AA has room on every edge.
        let pad = radius.max(1.0) + 1.5;
        let corner = |p: [f32; 2], col: Colour| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [col.r, col.g, col.b, col.a],
            color2: [hw, hh, 0.0, 0.0],
            uv: [0.0; 4],
            aux: [MODE_ROUNDED, radius, 0.0, 0.0],
        };
        self.set_blend(blend);
        self.quad([
            corner([-hw - pad, -hh - pad], top),
            corner([hw + pad, -hh - pad], top),
            corner([hw + pad, hh + pad], bottom),
            corner([-hw - pad, hh + pad], bottom),
        ]);
    }

    /// Filled disc with vertical gradient.
    pub fn disc(&mut self, center: [f32; 2], radius: f32, top: Colour, bottom: Colour, blend: Blend) {
        let r = radius + 1.5;
        let corner = |p: [f32; 2], col: Colour| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [col.r, col.g, col.b, col.a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_DISC, radius, 0.0, 0.0],
        };
        self.set_blend(blend);
        self.quad([
            corner([-r, -r], top),
            corner([r, -r], top),
            corner([r, r], bottom),
            corner([-r, r], bottom),
        ]);
    }

    /// Additive radial glow (approximation of osu! edge-effect glow).
    pub fn glow(&mut self, center: [f32; 2], radius: f32, colour: Colour) {
        let corner = |p: [f32; 2]| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [colour.r, colour.g, colour.b, colour.a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_GLOW, radius, 0.0, 0.0],
        };
        self.set_blend(Blend::Additive);
        self.quad([
            corner([-radius, -radius]),
            corner([radius, -radius]),
            corner([radius, radius]),
            corner([-radius, radius]),
        ]);
    }

    /// Additive ring-shaped glow: brightness peaks at `ring_radius` and
    /// falls off over `extent` in both directions (like a framework
    /// EdgeEffect glow around a circle's edge - the centre stays dark).
    #[allow(dead_code)]
    pub fn glow_ring(&mut self, center: [f32; 2], ring_radius: f32, extent: f32, colour: Colour) {
        let r = ring_radius + extent + 1.5;
        let corner = |p: [f32; 2]| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [colour.r, colour.g, colour.b, colour.a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_GLOWRING, ring_radius, extent, 0.0],
        };
        self.set_blend(Blend::Additive);
        self.quad([
            corner([-r, -r]),
            corner([r, -r]),
            corner([r, r]),
            corner([-r, r]),
        ]);
    }

    /// Framework EdgeEffect glow with `Hollow = false` (e.g. lazer's
    /// `FlashPiece`): full brightness inside `inner_radius`, then a
    /// quadratic falloff `((inner + extent - d) / extent)^2` outward to
    /// `inner_radius + extent`. Drawn additively. This mirrors the masking
    /// shader with `BlendRange = extent` and `AlphaExponent = 2`: the glow
    /// FILLS the shape's interior rather than rendering as a hollow ring.
    pub fn glow_fill(&mut self, center: [f32; 2], inner_radius: f32, extent: f32, colour: Colour) {
        let r = inner_radius + extent + 1.5;
        let corner = |p: [f32; 2]| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [colour.r, colour.g, colour.b, colour.a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_GLOWFILL, inner_radius, extent, 0.0],
        };
        self.set_blend(Blend::Additive);
        self.quad([
            corner([-r, -r]),
            corner([r, -r]),
            corner([r, r]),
            corner([-r, r]),
        ]);
    }

    /// Capsule (thick line with round caps) between two points.
    pub fn capsule(&mut self, p0: [f32; 2], p1: [f32; 2], radius: f32, colour: Colour, blend: Blend) {
        self.capsule_gradient(p0, p1, radius, colour, colour, blend);
    }

    /// Capsule with a colour gradient across its local y axis (perpendicular
    /// to the segment), like a GradientVertical fill on a rotated drawable
    /// (lazer's ArgonFollowPoint chevrons).
    pub fn capsule_gradient(
        &mut self,
        p0: [f32; 2],
        p1: [f32; 2],
        radius: f32,
        top: Colour,
        bottom: Colour,
        blend: Blend,
    ) {
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-4 {
            self.disc(p0, radius, top, bottom, blend);
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        let half = len * 0.5;
        let pad = radius + 1.5;
        // Local space: x along the segment, y across. The corner transform
        // maps local.x to (c, s) - that must be the SEGMENT direction
        // (ux, uy), with (-s, c) = (-uy, ux) as the perpendicular. Binding
        // s to -uy here would mirror the capsule about its own horizontal
        // midline (directions render flipped).
        let (s, c) = (uy, ux);
        let center = [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5];
        let corner = |lx: f32, ly: f32, col: Colour| Vertex {
            pos: [center[0] + lx * c - ly * s, center[1] + lx * s + ly * c],
            local: [lx, ly],
            color: [col.r, col.g, col.b, col.a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_CAPSULE, half, radius, 0.0],
        };
        self.set_blend(blend);
        self.quad([
            corner(-half - pad, -pad, top),
            corner(half + pad, -pad, top),
            corner(half + pad, pad, bottom),
            corner(-half - pad, pad, bottom),
        ]);
    }

    /// Arc of a ring between two angles (degrees, screen space, 0 = +x, y down).
    /// `thickness` is a band CENTRED on `radius` (the fragment test is
    /// `|d - radius| <= thickness / 2`), so the quad must extend to
    /// `radius + thickness / 2` - not just the radius - or the outer half
    /// of the band gets clipped away. Square caps.
    pub fn arc(
        &mut self,
        center: [f32; 2],
        radius: f32,
        thickness: f32,
        a0: f32,
        a1: f32,
        colour: Colour,
        blend: Blend,
    ) {
        let r_out = radius + thickness * 0.5 + 1.5;
        let mk = |p: [f32; 2]| Vertex {
            pos: [center[0] + p[0], center[1] + p[1]],
            local: p,
            color: [colour.r, colour.g, colour.b, colour.a],
            color2: [a1.to_radians(), 0.0, 0.0, 0.0],
            uv: [0.0; 4],
            aux: [MODE_ARC, radius, thickness, a0.to_radians()],
        };
        self.set_blend(blend);
        self.quad([
            mk([-r_out, -r_out]),
            mk([r_out, -r_out]),
            mk([r_out, r_out]),
            mk([-r_out, r_out]),
        ]);
    }

    /// Flat-colour triangle fan (screen-space points).
    pub fn polygon(&mut self, points: &[[f32; 2]], colour: Colour, blend: Blend) {
        self.set_blend(blend);
        for i in 1..points.len() - 1 {
            let mk = |p: [f32; 2]| Vertex {
                pos: p,
                local: [0.0; 2],
                color: [colour.r, colour.g, colour.b, colour.a],
                color2: [0.0; 4],
                uv: [0.0; 4],
                aux: [MODE_FLAT, 0.0, 0.0, 0.0],
            };
            self.tri([mk(points[0]), mk(points[i]), mk(points[i + 1])]);
        }
    }

    /// Radial cap disc: body colour inside, border band of `border` px at
    /// the rim (slider end caps).
    pub fn cap_disc(
        &mut self,
        centre: [f32; 2],
        radius: f32,
        border: f32,
        body: Colour,
        border_col: Colour,
        blend: Blend,
    ) {
        let r = radius + 1.5;
        let corner = |p: [f32; 2]| Vertex {
            pos: [centre[0] + p[0], centre[1] + p[1]],
            local: p,
            color: [body.r, body.g, body.b, body.a],
            color2: [border_col.r, border_col.g, border_col.b, border_col.a],
            uv: [0.0; 4],
            aux: [MODE_CAPDISC, radius, border, 0.0],
        };
        self.set_blend(blend);
        self.quad([
            corner([-r, -r]),
            corner([r, -r]),
            corner([r, r]),
            corner([-r, r]),
        ]);
    }

    /// Quad with per-vertex colours (flat coverage; relies on MSAA for edges).
    pub fn quad_gradient(&mut self, pts: &[[f32; 2]; 4], cols: [Colour; 4], blend: Blend) {
        let mk = |i: usize| Vertex {
            pos: pts[i],
            local: [0.0; 2],
            color: [cols[i].r, cols[i].g, cols[i].b, cols[i].a],
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [MODE_FLAT, 0.0, 0.0, 0.0],
        };
        self.set_blend(blend);
        self.quad(std::array::from_fn(mk));
    }

    /// Thick polyline with a border band (slider body), stroked as ONE
    /// continuous strip with proper miter joins (offset scaled by
    /// 1/cos(half-angle), clamped). Consecutive quads share their mitre
    /// edges exactly: no interior overlaps, so translucent fades never
    /// double-composite (no ghosting), and the width stays correct around
    /// corners.
    pub fn stroke_band(
        &mut self,
        points: &[[f32; 2]],
        half_width: f32,
        portion: f32,
        body: Colour,
        border: Colour,
        alpha: f32,
    ) {
        let n = points.len();
        if n < 2 {
            return;
        }
        self.set_blend(Blend::Alpha);

        let cbody = [body.r, body.g, body.b, body.a * alpha];
        let cborder = [border.r, border.g, border.b, border.a * alpha];

        // Per-vertex left/right offsets (mitre).
        let mut left: Vec<[f32; 2]> = Vec::with_capacity(n);
        let mut right: Vec<[f32; 2]> = Vec::with_capacity(n);
        for i in 0..n {
            let p = points[i];
            let (d1, d2) = if i == 0 {
                (None, seg_dir(points[0], points[1]))
            } else if i == n - 1 {
                (seg_dir(points[n - 2], points[n - 1]), None)
            } else {
                (seg_dir(points[i - 1], points[i]), seg_dir(points[i], points[i + 1]))
            };
            let (mx, my, scale) = match (d1, d2) {
                (Some(a), Some(b)) => {
                    let n1 = [-a.1, a.0];
                    let n2 = [-b.1, b.0];
                    let mut mx = n1[0] + n2[0];
                    let mut my = n1[1] + n2[1];
                    let l = (mx * mx + my * my).sqrt();
                    if l < 1e-5 {
                        // Near-reversal: fall back to the segment normal.
                        (n1[0], n1[1], 1.0)
                    } else {
                        mx /= l;
                        my /= l;
                        let cos_half = (mx * n1[0] + my * n1[1]).abs().max(0.35);
                        (mx, my, 1.0 / cos_half)
                    }
                }
                (Some(a), None) => (-a.1, a.0, 1.0),
                (None, Some(b)) => (-b.1, b.0, 1.0),
                (None, None) => (0.0, 0.0, 1.0),
            };
            let off = half_width * scale;
            left.push([p[0] + mx * off, p[1] + my * off]);
            right.push([p[0] - mx * off, p[1] - my * off]);
        }

        for i in 0..n - 1 {
            let base = self.vertices.len() as u32;
            let mut push = |pos: [f32; 2], t: f32| {
                self.vertices.push(Vertex {
                    pos,
                    local: [0.0; 2],
                    color: cbody,
                    color2: cborder,
                    uv: [0.0; 4],
                    aux: [MODE_STROKE, t, portion, 0.0],
                });
            };
            push(right[i], -1.0);
            push(left[i], 1.0);
            push(left[i + 1], 1.0);
            push(right[i + 1], -1.0);
            self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            self.run_len += 6;
        }

        // Round joins at sharp corners: the clamped miter leaves a wedge
        // gap on reversals, bridge it with a join disc (border rim + body).
        let border_w = half_width * portion;
        for i in 1..n - 1 {
            let (Some(d1), Some(d2)) = (seg_dir(points[i - 1], points[i]), seg_dir(points[i], points[i + 1])) else {
                continue;
            };
            let turn = d1.0 * d2.0 + d1.1 * d2.1; // cos(turn angle)
            if turn < 0.55 {
                let body_c = Colour { r: cbody[0], g: cbody[1], b: cbody[2], a: cbody[3] };
                let border_c = Colour { r: cborder[0], g: cborder[1], b: cborder[2], a: cborder[3] };
                self.cap_disc(points[i], half_width, border_w, body_c, border_c, Blend::Alpha);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TTF/OTF fonts (Torus): rasterised into the atlas at startup
// ---------------------------------------------------------------------------

/// Em size classes rasterised per glyph; small text gets its own bitmaps so
/// thin strokes survive downsampling.
pub const TTF_CLASSES: [u32; 3] = [24, 48, 96];

const TTF_CHARS: &[char] = &[
    'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P','Q','R','S','T','U','V','W','X','Y','Z',
    'a','b','c','d','e','f','g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v','w','x','y','z',
    '0','1','2','3','4','5','6','7','8','9','.',',','%','x','+','-','!','/',':',
    ' ', '(', ')', '\'', '&',
];

#[derive(Clone, Copy)]
pub struct TtfGlyph {
    pub rect: Rect,
    /// Ink box size in raster px.
    pub w: f32,
    pub h: f32,
    /// Offset from pen origin to ink top-left (raster px, y down from the
    /// glyph's baseline box).
    pub xoff: f32,
    pub yoff: f32,
    /// Horizontal advance (raster px at this em).
    pub advance: f32,
}

#[derive(Clone)]
pub struct TtfFont {
    /// (char, em) -> glyph.
    pub glyphs: HashMap<(char, u32), TtfGlyph>,
    /// (ascent, descent) at em 100.
    pub metrics: (f32, f32),
}

impl TtfFont {
    pub fn rasterize(bytes: &[u8], weight: u8) -> (TtfFont, Vec<(Region, Image)>) {
        let font = ab_glyph::FontArc::try_from_vec(bytes.to_vec()).expect("load ttf");

        let mut glyphs = HashMap::new();
        let mut images = Vec::new();
        let m100 = font.as_scaled(100.0f32);
        let metrics = (m100.ascent(), m100.descent());

        for &em in &TTF_CLASSES {
            let scale = ab_glyph::PxScale::from(em as f32);
            let scaled = font.as_scaled(scale);
            for &c in TTF_CHARS {
                let gid = match font.glyph_id(c) {
                    g if g.0 != 0 => g,
                    _ => continue,
                };
                let glyph = gid.with_scale_and_position(scale, ab_glyph::Point { x: 0.0, y: 0.0 });
                let advance = scaled.h_advance(gid);
                let Some(outlined) = font.outline_glyph(glyph) else {
                    // Whitespace: no outline to raster, but the advance
                    // must survive so words keep their spacing.
                    glyphs.insert(
                        (c, em),
                        TtfGlyph {
                            rect: Rect { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 },
                            w: 0.0,
                            h: 0.0,
                            xoff: 0.0,
                            yoff: 0.0,
                            advance,
                        },
                    );
                    continue;
                };

                // NOTE: ab_glyph bounds are y-up around the baseline, so
                // min.y is NEGATIVE for glyphs above it. Casts to u32 must
                // happen only after the (positive) extents are computed.
                let bounds = outlined.px_bounds();
                let w = (bounds.max.x - bounds.min.x).ceil().max(1.0) as u32;
                let h = (bounds.max.y - bounds.min.y).ceil().max(1.0) as u32;
                if w > 512 || h > 512 {
                    continue;
                }

                let mut coverage = vec![0f32; (w * h) as usize];
                outlined.draw(|x, y, v| {
                    let idx = y as usize * w as usize + x as usize;
                    if idx < coverage.len() {
                        coverage[idx] = v;
                    }
                });

                let mut rgba = Vec::with_capacity(coverage.len() * 4);
                for v in coverage {
                    let a = v.clamp(0.0, 1.0);
                    rgba.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
                }

                glyphs.insert(
                    (c, em),
                    TtfGlyph {
                        rect: Rect { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 },
                        w: w as f32,
                        h: h as f32,
                        xoff: bounds.min.x as f32,
                        yoff: bounds.min.y as f32,
                        advance: scaled.h_advance(gid),
                    },
                );
                images.push((
                    Region::Glyph { weight, c, em },
                    Image { width: w, height: h, rgba },
                ));
            }
        }

        (TtfFont { glyphs, metrics }, images)
    }

    /// Patches glyph uv rects after the atlas is built.
    pub fn patch_rects(&mut self, atlas: &Atlas, weight: u8) {
        for ((c, em), g) in self.glyphs.iter_mut() {
            let region = Region::Glyph { weight, c: *c, em: *em };
            if let Some(r) = atlas.rects.get(&region) {
                g.rect = *r;
            }
        }
    }

    pub fn class_for(size_px: f32) -> u32 {
        // Pick the smallest class that is >= the requested size, else the
        // largest (downscale beats heavy upscale beyond 2x).
        if size_px <= 24.0 {
            24
        } else if size_px <= 48.0 {
            48
        } else {
            96
        }
    }
}

/// Draws a string with a TTF font, centred at `center`.
/// Measures a string with the same glyph classes `draw_ttf_text` rasterises
/// at: `(advance width, ink top, ink bottom)` in scaled pixels. The ink
/// extents are relative to the baseline, top negative.
pub fn ttf_measure(font: &TtfFont, text: &str, size_px: f32, spacing: f32) -> (f32, f32, f32) {
    let em = TtfFont::class_for(size_px);
    let scale = size_px / em as f32;
    let mut width = 0.0f32;
    let mut top = f32::MAX;
    let mut bottom = f32::MIN;
    let mut n = 0;
    for c in text.chars() {
        if let Some(g) = font.glyphs.get(&(c, em)) {
            width += g.advance * scale + spacing;
            top = top.min(g.yoff);
            bottom = bottom.max(g.yoff + g.h);
            n += 1;
        }
    }
    if n > 0 {
        width -= spacing;
    }
    if top == f32::MAX {
        let (_, descent100) = font.metrics;
        (width, 0.0, descent100 * em as f32 / 100.0 * scale)
    } else {
        (width, top * scale, bottom * scale)
    }
}

pub fn draw_ttf_text(
    list: &mut DrawList,
    atlas: &Atlas,
    font: &TtfFont,
    _bold: bool,
    text: &str,
    center: [f32; 2],
    size_px: f32,
    colour: Colour,
    spacing: f32,
    blend: Blend,
) {
    let em = TtfFont::class_for(size_px);
    let scale = size_px / em as f32;
    let (ascent100, descent100) = font.metrics;
    let ascent = ascent100 * em as f32 / 100.0;
    let descent = descent100 * em as f32 / 100.0;

    let key = |c: char| (c, em);
    let mut width = 0.0f32;
    let mut chars: Vec<char> = Vec::new();
    for c in text.chars() {
        chars.push(c);
        if let Some(g) = font.glyphs.get(&key(c)) {
            width += g.advance * scale + spacing;
        }
    }
    if !chars.is_empty() {
        width -= spacing;
    }

    // Centre the string's ink band (union of glyph ink boxes) on center.y.
    let mut top = f32::MAX;
    let mut bottom = f32::MIN;
    for c in &chars {
        if let Some(g) = font.glyphs.get(&key(*c)) {
            top = top.min(g.yoff);
            bottom = bottom.max(g.yoff + g.h);
        }
    }
    let baseline = if top == f32::MAX {
        center[1] + (ascent - descent) * 0.5 * scale
    } else {
        center[1] - (top + bottom) * 0.5 * scale
    };
    let mut pen_x = center[0] - width * 0.5;

    for c in chars {
        let g = match font.glyphs.get(&key(c)) {
            Some(g) => g,
            None => continue,
        };
        if g.w <= 0.0 || g.h <= 0.0 {
            // Whitespace: advance only, nothing to draw.
            pen_x += g.advance * scale + spacing;
            continue;
        }
        let aw = atlas.width as f32;
        let ah = atlas.height as f32;
        let gx0 = g.rect.x0 / aw;
        let gy0 = g.rect.y0 / ah;
        let gx1 = g.rect.x1 / aw;
        let gy1 = g.rect.y1 / ah;
        let x = pen_x + g.xoff * scale;
        let y = baseline + g.yoff * scale;
        let w = g.w * scale;
        let h = g.h * scale;

        let corner = |px: f32, py: f32, u: f32, v: f32| Vertex {
            pos: [px, py],
            local: [0.0; 2],
            color: [colour.r, colour.g, colour.b, colour.a],
            color2: [0.0; 4],
            uv: [u, v, 0.0, 0.0],
            aux: [MODE_TEXTURED, 0.0, 0.0, 0.0],
        };
        list.set_blend(blend);
        list.quad([
            corner(x, y, gx0, gy0),
            corner(x + w, y, gx1, gy0),
            corner(x + w, y + h, gx1, gy1),
            corner(x, y + h, gx0, gy1),
        ]);

        pen_x += g.advance * scale + spacing;
    }
}

// ---------------------------------------------------------------------------
// Texture atlas
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Region {
    /// TTF glyph rasterised at a specific em size class.
    Glyph { weight: u8, c: char, em: u32 },
    CounterDigit(u8), // b'0'..=b'9'
    CounterDot,
    CounterPercent,
    CounterX,
    CounterWireframes,
    CursorTrail,
    RepeatEdge,
    ApproachCircle,
    /// Full-screen beatmap background (`--bg`).
    Background,
    /// The beatmap background pre-blurred for the results screen
    /// (lazer `ResultsScreen` blurs the background with
    /// `BACKGROUND_BLUR` sigma and fades it to gray).
    BackgroundBlurred,
    /// Storyboard below-layer composite (Background/Fail/Pass sprites,
    /// `--storyboard`): refreshed GPU-side every frame, never read back.
    Storyboard,
    /// Storyboard above-layer composite (Foreground/Overlay sprites),
    /// drawn over the playfield like osu!; packed only when the map's
    /// storyboard uses those layers.
    StoryboardForeground,
    /// The results-screen avatar (`--avatar <image>` / config `avatar`):
    /// cover-cropped to a square with pre-masked rounded corners.
    Avatar,
    /// A user-skin texture (`--skin <dir>`): index into the skin's
    /// texture table, assigned when the atlas is built.
    Skin(u32),
    /// A built-in lazer mod icon (`assets/modicons/mod-*.png`): index
    /// into the MOD_ICON_NAMES table.
    ModIcon(u16),
    /// The hexagonal ModIcon background (`mod-icon.png`).
    ModIconBg,
}

pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    rects: HashMap<Region, Rect>,
    /// Ink bounding boxes (in region-local pixels) for texture-based glyph
    /// runs (argon counter digits): (x0, y0, x1, y1).
    inks: HashMap<Region, [f32; 4]>,
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

struct ShelfPacker {
    width: u32,
    y: u32,
    shelf_height: u32,
    x: u32,
}

impl ShelfPacker {
    fn new(width: u32) -> ShelfPacker {
        ShelfPacker { width, y: 0, shelf_height: 0, x: 0 }
    }
    fn alloc(&mut self, w: u32, h: u32) -> (u32, u32) {
        let pad = 2;
        if self.x + w + pad > self.width {
            self.y += self.shelf_height + pad;
            self.x = 0;
            self.shelf_height = 0;
        }
        let (px, py) = (self.x, self.y);
        self.x += w + pad;
        self.shelf_height = self.shelf_height.max(h);
        (px, py)
    }
}


#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Alpha-coverage bounding box of an image in local pixels.
fn seg_dir(a: [f32; 2], b: [f32; 2]) -> Option<(f32, f32)> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let l = (dx * dx + dy * dy).sqrt();
    if l < 1e-5 { None } else { Some((dx / l, dy / l)) }
}

fn ink_bbox(img: &Image) -> (f32, f32, f32, f32) {
    let (mut x0, mut y0) = (f32::MAX, f32::MAX);
    let (mut x1, mut y1) = (f32::MIN, f32::MIN);
    for y in 0..img.height {
        for x in 0..img.width {
            let a = img.rgba[((y * img.width + x) * 4 + 3) as usize];
            if a > 16 {
                x0 = x0.min(x as f32);
                y0 = y0.min(y as f32);
                x1 = x1.max(x as f32 + 1.0);
                y1 = y1.max(y as f32 + 1.0);
            }
        }
    }
    if x0 == f32::MAX {
        (0.0, 0.0, img.width as f32, img.height as f32)
    } else {
        (x0, y0, x1, y1)
    }
}

impl Atlas {
    /// Pack into at most `max_dim` on both axes: try widths 4096 → up to
    /// `max_dim` (doubling), taking the first whose packed height fits.
    /// Returns `None` when even the widest allowed layout overflows — the
    /// caller decides whether to downscale and retry (see `build_atlas`).
    pub fn try_build(images: &[(Region, Image)], max_dim: u32) -> Option<Atlas> {
        let mut width = 4096u32.min(max_dim);
        loop {
            let atlas = pack(images, width);
            if atlas.height <= max_dim {
                return Some(atlas);
            }
            if width >= max_dim {
                return None;
            }
            width = (width * 2).min(max_dim);
        }
    }

    pub fn build(images: &[(Region, Image)]) -> Atlas {
        pack(images, 4096)
    }
}

fn pack(images: &[(Region, Image)], width: u32) -> Atlas {
    {
        let mut packer = ShelfPacker::new(width);
        let mut rects = HashMap::new();

        let mut height = 0u32;
        let mut placed: Vec<(Region, u32, u32)> = Vec::new();
        for (region, img) in images {
            let (x, y) = packer.alloc(img.width, img.height);
            placed.push((*region, x, y));
            rects.insert(
                *region,
                Rect {
                    x0: x as f32,
                    y0: y as f32,
                    x1: (x + img.width) as f32,
                    y1: (y + img.height) as f32,
                },
            );
            height = height.max(y + img.height);
        }
        let height = height.next_power_of_two().max(16);

        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for ((_, img), (_, px, py)) in images.iter().zip(placed.iter()) {
            for row in 0..img.height {
                let src = ((row * img.width * 4) as usize, ((row * img.width + img.width) * 4) as usize);
                let dst = (((py + row) * width + px) as usize * 4)
                    ..(((py + row) * width + px + img.width) as usize * 4);
                rgba[dst].copy_from_slice(&img.rgba[src.0..src.1]);
            }
        }

        // Compute ink bboxes from alpha coverage.
        let mut inks = HashMap::new();
        for (region, img) in images {
            let (x0, y0, x1, y1) = ink_bbox(img);
            inks.insert(*region, [x0, y0, x1, y1]);
        }

        Atlas { width, height, rgba, rects, inks }
    }
}

impl Atlas {
    pub fn region_rect(&self, region: Region) -> Rect {
        *self.rects.get(&region).expect("atlas region")
    }

    pub fn ink(&self, region: Region) -> [f32; 4] {
        self.inks.get(&region).copied().unwrap_or([0.0, 0.0, 0.0, 0.0])
    }
}



#[cfg(test)]
mod atlas_tests {
    use super::*;

    fn solid(region: Region, w: u32, h: u32) -> (Region, Image) {
        (region, Image { width: w, height: h, rgba: vec![255; (w * h * 4) as usize] })
    }

    /// The width ladder: content that overflows 8192 in height at 4096
    /// wide must re-pack wider instead of failing.
    #[test]
    fn ladder_widens_instead_of_overflowing() {
        // Three 4000x4000 slabs: at 4096 wide they stack 12000 tall
        // (> 8192); at 8192 wide two fit per shelf -> <= 8192 tall.
        let images = vec![
            solid(Region::Skin(0), 4000, 4000),
            solid(Region::Skin(1), 4000, 4000),
            solid(Region::Skin(2), 4000, 4000),
        ];
        let atlas = Atlas::try_build(&images, 8192).expect("fits when widened");
        assert_eq!(atlas.width, 8192);
        assert!(atlas.height <= 8192, "height {}", atlas.height);
    }

    /// Content that cannot fit even at max width reports None (the
    /// caller downscale loop's trigger).
    #[test]
    fn reports_overflow_at_max_width() {
        let images = vec![solid(Region::Skin(0), 9000, 9000)];
        assert!(Atlas::try_build(&images, 8192).is_none());
    }
}
