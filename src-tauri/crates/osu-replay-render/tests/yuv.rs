//! End-to-end GPU YUV conversion test: renders flat-colour quads through
//! the offscreen renderer, converts on the GPU, and checks the packed
//! NV12/I420 output against BT.601 limited-range reference values.

use osu_replay_render::draw::{Atlas, Blend, DrawList, Vertex};
use osu_replay_render::render::Renderer;

const W: usize = 1280;
const H: usize = 720;

/// Full-screen flat quad (mode 6 = the shader's fallback flat colour).
fn flat_frame(list: &mut DrawList, color: [f32; 4]) {
    let base = list.vertices.len() as u32;
    for corner in [
        [0.0f32, 0.0],
        [W as f32, 0.0],
        [W as f32, H as f32],
        [0.0, H as f32],
    ] {
        list.vertices.push(Vertex {
            pos: corner,
            local: [0.0; 2],
            color,
            color2: [0.0; 4],
            uv: [0.0; 4],
            aux: [6.0, 0.0, 0.0, 0.0],
        });
    }
    list.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    list.runs.push((Blend::Alpha, base, 6));
}

/// NV12 sample: (Y, U, V) at pixel (x, y).
fn nv12_at(frame: &[u8], x: usize, y: usize) -> (u8, u8, u8) {
    let yy = frame[y * W + x];
    let c = W * H + (y / 2) * W + (x / 2) * 2;
    (yy, frame[c], frame[c + 1])
}

/// I420 sample: (Y, U, V) at pixel (x, y).
fn i420_at(frame: &[u8], x: usize, y: usize) -> (u8, u8, u8) {
    let yy = frame[y * W + x];
    let cw = W / 2;
    let u = frame[W * H + (y / 2) * cw + x / 2];
    let v = frame[W * H + cw * (H / 2) + (y / 2) * cw + x / 2];
    (yy, u, v)
}

fn render_color(renderer: &mut Renderer, rgb: [f32; 3], interleaved: bool) -> Vec<u8> {
    let mut list = DrawList::new();
    flat_frame(&mut list, [rgb[0], rgb[1], rgb[2], 1.0]);
    list.finish();
    renderer.render_deferred_yuv(&list, [0.0, 0.0, 0.0, 1.0], interleaved);
    let mut out = Vec::new();
    renderer.read_oldest_yuv_into(&mut out);
    assert_eq!(out.len(), W * H * 3 / 2);
    out
}

fn check(name: &str, got: (u8, u8, u8), want: (i32, i32, i32)) {
    let ok = (got.0 as i32 - want.0).abs() <= 2
        && (got.1 as i32 - want.1).abs() <= 2
        && (got.2 as i32 - want.2).abs() <= 2;
    assert!(ok, "{name}: got (Y={} U={} V={}), want ~({} {} {})", got.0, got.1, got.2, want.0, want.1, want.2);
}

#[test]
fn gpu_yuv_nv12_colors() {
    let atlas = Atlas::build(&[]);
    let mut renderer = Renderer::new(W as u32, H as u32, &atlas);
    assert!(renderer.yuv_ready());
    // (name, rgb, expected BT.601 limited-range YCbCr)
    for (name, rgb, want) in [
        ("white", [1.0f32, 1.0, 1.0], (235, 128, 128)),
        ("black", [0.0, 0.0, 0.0], (16, 128, 128)),
        ("red", [1.0, 0.0, 0.0], (81, 90, 240)),
        ("green", [0.0, 1.0, 0.0], (145, 54, 34)),
        ("blue", [0.0, 0.0, 1.0], (41, 240, 110)),
    ] {
        let frame = render_color(&mut renderer, rgb, true);
        // Centre and corners-inside: identical flat colour everywhere.
        for (x, y) in [(W / 2, H / 2), (4, 4), (W - 5, H - 5), (W - 5, 4), (4, H - 5)] {
            check(&format!("nv12 {name} at ({x},{y})"), nv12_at(&frame, x, y), want);
        }
        // Luma must be uniform across a whole word boundary (4 px groups).
        let row = &frame[(H / 2) * W..(H / 2) * W + W];
        assert!(row.iter().all(|&v| (v as i32 - want.0).abs() <= 2), "nv12 {name}: luma row not uniform");
    }
}

#[test]
fn gpu_yuv_i420_colors() {
    let atlas = Atlas::build(&[]);
    let mut renderer = Renderer::new(W as u32, H as u32, &atlas);
    for (name, rgb, want) in [
        ("white", [1.0f32, 1.0, 1.0], (235, 128, 128)),
        ("red", [1.0, 0.0, 0.0], (81, 90, 240)),
        ("blue", [0.0, 0.0, 1.0], (41, 240, 110)),
    ] {
        let frame = render_color(&mut renderer, rgb, false);
        for (x, y) in [(W / 2, H / 2), (4, 4), (W - 5, H - 5)] {
            check(&format!("i420 {name} at ({x},{y})"), i420_at(&frame, x, y), want);
        }
    }
}
