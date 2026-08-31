//! Legacy slider-body gradient verification: renders a single horizontal
//! body through the distance-field pipeline and checks the three
//! `LegacyDrawableSliderPath.ColourAt` bands across a cross-section -
//! transparent-black rim, border band, then the sRGB lerp
//! `accent.Darken(0.1) -> lighten(accent, 0.5)` at 0.7 track alpha.

use osu_replay_render::draw::{Atlas, BodyDraw, Colour, DrawList};
use osu_replay_render::render::Renderer;

#[test]
fn legacy_slider_body_gradient_bands() {
    let w = 320u32;
    let h = 160u32;
    let atlas = Atlas::build(&[]);
    let mut renderer = Renderer::new(w, h, &atlas);

    let accent = Colour::rgb(1.0, 0.0, 0.0).opacity(0.7);
    let mut list = DrawList::new();
    list.bodies.push(BodyDraw {
        segments: vec![([60.0, 80.0], [260.0, 80.0])],
        radius: 60.0,
        border: (0.1875 - 0.078125) * 60.0,
        body: accent.darken(0.1),
        border_colour: Colour::WHITE,
        inner_colour: Some(accent.lighten(0.5)),
    });
    // Body composites are interleaved into the draw order through marks;
    // unmarked bodies never draw.
    list.mark_body();
    list.finish();

    let buf = renderer.render(&list, [0.0, 0.0, 0.0, 1.0]);
    let stride = renderer.padded_row as usize;
    let px = |x: u32, y: u32| -> (f32, f32, f32) {
        let i = y as usize * stride + (x * 4) as usize;
        // Readback is BGRA.
        (buf[i + 2] as f32 / 255.0, buf[i + 1] as f32 / 255.0, buf[i] as f32 / 255.0)
    };

    let centre = 80.0f32;
    let r = 60.0f32;
    let sample = |dy: f32| px(160, (centre + dy) as u32);

    // Border band: position in (0.078, 0.1875] -> dy in
    // [r*(1-0.1875), r*(1-0.078)) = [48.75, 55.3): white.
    let (br, bg, bb) = sample(-51.0);
    assert!((br - 1.0).abs() < 0.02 && (bg - 1.0).abs() < 0.02 && (bb - 1.0).abs() < 0.02,
        "border band should be white, got ({br:.2},{bg:.2},{bb:.2})");

    // Body outer edge (just inside the border, position slightly past
    // 0.1875): close to accent.Darken(0.1) = (0.909, 0, 0) at 0.7 alpha
    // over black = (0.636, 0, 0). Allow the first lerp step.
    let (or, og, ob) = sample(-46.0);
    assert!(og < 0.06 && ob < 0.06, "body outer edge should have ~no green/blue, got ({or:.2},{og:.2},{ob:.2})");
    assert!((or - 0.636).abs() < 0.1, "body outer red ~0.64, got {or:.2}");

    // Centre: lighten(accent, 0.5) = (1, 0.25, 0.25) at 0.7 alpha over
    // black = (0.7, 0.175, 0.175) - the bright inner core (the
    // "highlight").
    let (cr, cg, cb) = sample(0.0);
    assert!((cr - 0.7).abs() < 0.06, "centre red ~0.70, got {cr:.2}");
    assert!((cg - 0.175).abs() < 0.05, "centre green ~0.175, got {cg:.2}");
    assert!((cb - 0.175).abs() < 0.05, "centre blue ~0.175, got {cb:.2}");

    // Monotonic brightening toward the centre along the body gradient
    // (green channel rises from the outer edge inward).
    let mut last_g = -1.0;
    for step in 0..=10 {
        let dy = -46.0 + step as f32 * 4.6;
        let (_, g, _) = sample(-dy.abs());
        if g + 0.005 < last_g {
            panic!("gradient should brighten toward the centre: g went {last_g:.3} -> {g:.3}");
        }
        last_g = g;
    }
}
