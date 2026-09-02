//! Offscreen wgpu renderer: uploads the atlas, renders a `DrawList` to a
//! texture with MSAA, reads the frame back as BGRA bytes.

use crate::draw::{Atlas, Blend, DrawList, Vertex};
use std::num::NonZeroU64;

const SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) color2: vec4<f32>,
    @location(4) uv: vec4<f32>,
    @location(5) aux: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) color2: vec4<f32>,
    @location(3) uv: vec4<f32>,
    @location(4) aux: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(in.pos.x / screen.size.x * 2.0 - 1.0, 1.0 - in.pos.y / screen.size.y * 2.0);
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = in.local;
    out.color = in.color;
    out.color2 = in.color2;
    out.uv = in.uv;
    out.aux = in.aux;
    return out;
}

fn aa(sdf: f32) -> f32 {
    return clamp(sdf / 0.75 + 0.5, 0.0, 1.0);
}

// ----- Slider body distance-field prepass (lazer PathDrawNode style) -----

struct VsOut2 {
    @builtin(position) pos: vec4<f32>,
    @location(0) start: vec2<f32>,
    @location(1) end: vec2<f32>,
    @location(2) radius: f32,
};

struct PrepassIn {
    @location(0) pos: vec2<f32>,
    @location(1) start: vec2<f32>,
    @location(2) end: vec2<f32>,
    @location(3) radius: f32,
};

@vertex
fn vs_body_pre(in: PrepassIn) -> VsOut2 {
    var out: VsOut2;
    let ndc = vec2<f32>(in.pos.x / screen.size.x * 2.0 - 1.0, 1.0 - in.pos.y / screen.size.y * 2.0);
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.start = in.start;
    out.end = in.end;
    out.radius = in.radius;
    return out;
}

fn distToSeg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let len2 = dot(ab, ab);
    var t = 0.0;
    if (len2 > 0.000001) {
        t = clamp(dot(p - a, ab) / len2, 0.0, 1.0);
    }
    let closest = a + ab * t;
    return distance(p, closest);
}

@fragment
fn fs_body_pre(in: VsOut2) -> @location(0) f32 {
    let p = vec2<f32>(in.pos.x, in.pos.y);
    return distToSeg(p, in.start, in.end) / in.radius;
}

@fragment
fn fs_body_main(in: VsOut) -> @location(0) vec4<f32> {
    // Single composite sample of the distance field: border band at the
    // rim, body colour inside, analytic AA at the outer edge, all scaled
    // by the fade alpha (no per-segment compositing).
    let uv = in.uv.xy / screen.size;
    let d = textureSampleLevel(tex, samp, uv, 0.0).r * in.aux.y;
    let r = in.aux.y;
    let b = in.aux.z;

    if (in.aux.w > 0.5) {
        // Legacy radial gradient (`LegacyDrawableSliderPath.ColourAt`,
        // position 0 = path edge, 1 = centre): transparent-black rim over
        // [0, shadow], the border colour over (shadow, border], then the
        // sRGB lerp accent.Darken(0.1) -> lighten(accent, 0.5). The inner
        // colour rides in local.xy + uv.z; its alpha equals the outer
        // (body) alpha. The shadow/border segment alphas take the fade
        // from the border alpha (borders ship opaque).
        let position = clamp(1.0 - d / r, 0.0, 1.0);
        let SHADOW = 0.078125;  // 1 - 59/64
        let BORDER = 0.1875;
        var col: vec4<f32>;
        if (position <= BORDER) {
            if (position <= SHADOW) {
                col = vec4<f32>(0.0, 0.0, 0.0, 0.25 * (position / SHADOW) * in.color2.a);
            } else {
                col = in.color2;
            }
        } else {
            let t = clamp((position - BORDER) / (1.0 - BORDER), 0.0, 1.0);
            let inner = vec3<f32>(in.local.x, in.local.y, in.uv.z);
            col = vec4<f32>(mix(in.color.rgb, inner, t), in.color.a);
        }
        let aa_a = aa(r - d) * col.a;
        return vec4<f32>(col.rgb * aa_a, aa_a);
    }

    var col = in.color;
    if (d > r - b) {
        col = in.color2;
    }
    let alpha = aa(r - d) * col.a;
    return vec4<f32>(col.rgb * alpha, alpha);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let mode = in.aux.x;
    var c: vec4<f32>;

    if (mode == 0.0) {
        // Textured (atlas): sample and premultiply by total alpha so the
        // One/OneMinusSrcAlpha blend behaves like straight-alpha compositing.
        let t = textureSampleLevel(tex, samp, in.uv.xy, 0.0);
        let a = in.color.a * t.a;
        c = vec4<f32>(in.color.rgb * t.rgb * a, a);
    } else if (mode == 1.0) {
        // Ring: annulus band, outer radius aux.y, thickness aux.z.
        let d = length(in.local);
        let outer = in.aux.y;
        let inner = outer - in.aux.z;
        let sdf = min(outer - d, d - inner);
        let a = aa(sdf) * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 2.0) {
        // Disc.
        let d = length(in.local);
        let a = aa(in.aux.y - d) * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 3.0) {
        // Additive radial glow (gaussian-ish).
        let d = length(in.local);
        let r = in.aux.y;
        let x = clamp(d / r, 0.0, 1.0);
        let a = exp(-x * x * 4.5) * (1.0 - x * x) * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 9.0) {
        // Ring-shaped glow: peaks at aux.y, falls off over aux.z both ways.
        let d = length(in.local);
        let q = (d - in.aux.y) / in.aux.z;
        let a = exp(-q * q * 4.5) * max(0.0, 1.0 - q * q) * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 10.0) {
        // Framework EdgeEffect glow, Hollow = false (lazer FlashPiece):
        // alpha 1 inside aux.y, quadratic falloff ((aux.y + aux.z - d) /
        // aux.z)^2 outward (masking shader with BlendRange = aux.z and
        // AlphaExponent = 2).
        let d = length(in.local);
        let r0 = in.aux.y;
        let ext = in.aux.z;
        var f = 1.0;
        if (d > r0) {
            f = clamp((r0 + ext - d) / ext, 0.0, 1.0);
            f = f * f;
        }
        let a = f * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 4.0) {
        // Stroke band: t = aux.y in [-1..1], border for |t| > 1 - aux.z.
        let t = abs(in.aux.y);
        let portion = in.aux.z;
        var col = in.color;
        if (t > 1.0 - portion) {
            col = in.color2;
        }
        c = vec4<f32>(col.rgb * col.a, col.a);
    } else if (mode == 5.0) {
        // Capsule: local.x along the segment (half length aux.y), radius aux.z.
        let hl = in.aux.y;
        let axial = clamp(in.local.x, -hl, hl);
        let d = length(vec2<f32>(in.local.x - axial, in.local.y));
        let a = aa(in.aux.z - d) * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 7.0) {
        // Arc band: radius aux.y, thickness aux.z, angles [aux.w, color2.x) rad.
        let d = length(in.local);
        let band = abs(d - in.aux.y) <= in.aux.z * 0.5 + 0.75;
        var ang = atan2(in.local.y, in.local.x);
        let a0 = in.aux.w;
        let a1 = in.color2.x;
        // Normalize angle into [a0, a0 + 2pi).
        var rel = ang - a0;
        let two_pi = 6.28318530718;
        rel = (rel - floor(rel / two_pi) * two_pi);
        let span = a1 - a0;
        let in_arc = rel <= span;
        let inside = select(0.0, 1.0, band && in_arc);
        // Angular soft edge (approximate AA at the caps).
        let ang_fade = min(1.0, min(rel, span - rel) * in.aux.y / 0.75);
        let a = inside * ang_fade * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else if (mode == 8.0) {
        // Cap disc: body fill with a radial border band at the rim (slider
        // end caps; overlapping the band seamlessly since colours match).
        let d = length(in.local);
        let r = in.aux.y;
        let b = in.aux.z;
        var col = in.color;
        if (d > r - b) {
            col = in.color2;
        }
        let a = aa(r - d) * col.a;
        c = vec4<f32>(col.rgb * a, a);
    } else if (mode == 11.0) {
        // Rounded rectangle: aux.y = corner radius, colour2.xy = half
        // extents. Corner colours interpolate into the vertical gradient.
        let half_ = in.color2.xy;
        let r = min(in.aux.y, min(half_.x, half_.y));
        let q = abs(in.local) - (half_ - vec2<f32>(r));
        let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
        let a = aa(-d) * in.color.a;
        c = vec4<f32>(in.color.rgb * a, a);
    } else {
        // Flat colour.
        c = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
    }

    return c;
}
"#;

/// RGB → YUV420 (BT.601 limited range) on the GPU. One Y invocation packs
/// 4 horizontal luma bytes into one storage word; one chroma invocation
/// packs a full storage word (or two for NV12), so no two invocations ever
/// touch the same word — no atomics needed. Requires width % 8 == 0 and
/// even height (word alignment of every plane row); callers fall back to a
/// CPU conversion otherwise.
const YUV_SHADER: &str = r#"
struct Params {
    size: vec2<u32>,
};
@group(0) @binding(0) var<uniform> params: Params;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(0) var<storage, read_write> out: array<u32>;

fn luma_y(c: vec3<f32>) -> u32 {
    let y = 16.0 + 219.0 * dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return u32(clamp(y, 16.0, 235.0) + 0.5);
}

fn chroma_uv(c: vec3<f32>) -> vec2<u32> {
    let cb = 128.0 + 224.0 * dot(c, vec3<f32>(-0.168736, -0.331264, 0.5));
    let cr = 128.0 + 224.0 * dot(c, vec3<f32>(0.5, -0.418688, -0.081312));
    return vec2<u32>(u32(clamp(cb, 16.0, 240.0) + 0.5), u32(clamp(cr, 16.0, 240.0) + 0.5));
}

fn block_avg(x: u32, y: u32) -> vec3<f32> {
    var sum = vec3<f32>(0.0, 0.0, 0.0);
    for (var dy = 0u; dy < 2u; dy = dy + 1u) {
        for (var dx = 0u; dx < 2u; dx = dx + 1u) {
            sum = sum + textureLoad(tex, vec2<i32>(i32(x + dx), i32(y + dy)), 0).rgb;
        }
    }
    return sum * 0.25;
}

@compute @workgroup_size(8, 4)
fn cs_y(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.size.x;
    let h = params.size.y;
    let x0 = gid.x * 4u;
    let y = gid.y;
    if (x0 + 3u >= w || y >= h) { return; }
    var word = 0u;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let c = textureLoad(tex, vec2<i32>(i32(x0 + i), i32(y)), 0).rgb;
        word = word | (luma_y(c) << (8u * i));
    }
    out[y * (w / 4u) + gid.x] = word;
}

// NV12: UV pairs interleaved after the Y plane, one pair per 2x2 block.
@compute @workgroup_size(8, 4)
fn cs_uv_nv12(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.size.x;
    let h = params.size.y;
    let cw = w / 2u;
    let ch = h / 2u;
    let cx0 = gid.x * 4u;
    let cy = gid.y;
    if (cx0 + 3u >= cw || cy >= ch) { return; }
    var words = array<u32, 2>(0u, 0u);
    for (var i = 0u; i < 4u; i = i + 1u) {
        let uv = chroma_uv(block_avg((cx0 + i) * 2u, cy * 2u));
        let j = i / 2u;
        let s = (i % 2u) * 16u;
        words[j] = words[j] | (uv.x << s) | (uv.y << (s + 8u));
    }
    let base = (w * h + cy * w + cx0 * 2u) / 4u;
    out[base] = words[0];
    out[base + 1u] = words[1];
}

// I420: U plane then V plane after the Y plane.
@compute @workgroup_size(8, 4)
fn cs_uv_i420(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.size.x;
    let h = params.size.y;
    let cw = w / 2u;
    let ch = h / 2u;
    let cx0 = gid.x * 4u;
    let cy = gid.y;
    if (cx0 + 3u >= cw || cy >= ch) { return; }
    var wu = 0u;
    var wv = 0u;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let uv = chroma_uv(block_avg((cx0 + i) * 2u, cy * 2u));
        wu = wu | (uv.x << (8u * i));
        wv = wv | (uv.y << (8u * i));
    }
    let u_base = (w * h + cy * cw + cx0) / 4u;
    let v_base = (w * h + cw * ch + cy * cw + cx0) / 4u;
    out[u_base] = wu;
    out[v_base] = wv;
}
"#;

struct YuvPass {
    pipeline_y: wgpu::ComputePipeline,
    pipeline_uv_nv12: wgpu::ComputePipeline,
    pipeline_uv_i420: wgpu::ComputePipeline,
    params_bind: wgpu::BindGroup,
    tex_bind: wgpu::BindGroup,
    out_bind: wgpu::BindGroup,
    out_buf: wgpu::Buffer,
    ring: Vec<wgpu::Buffer>,
    ring_next: usize,
    pending: std::collections::VecDeque<usize>,
    /// Packed frame size in bytes (w*h*3/2); ring buffers are this size.
    bytes: u32,
    groups_y: (u32, u32),
    groups_uv: (u32, u32),
}

pub struct Renderer {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline_alpha: wgpu::RenderPipeline,
    pipeline_additive: wgpu::RenderPipeline,
    atlas_bind: wgpu::BindGroup,
    atlas_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
    /// The live atlas GPU texture (kept for `copy_into_atlas`).
    atlas_tex: wgpu::Texture,
    screen_bind: wgpu::BindGroup,
    body_tex: wgpu::Texture,
    body_bind: wgpu::BindGroup,
    body_pre_pipeline: wgpu::RenderPipeline,
    body_main_pipeline: wgpu::RenderPipeline,
    /// Dedicated prepass buffers: the prepass commands must not see the
    /// scene vertex data (all queue writes land before the encoder runs).
    body_vbo: wgpu::Buffer,
    body_ibo: wgpu::Buffer,
    target: wgpu::Texture,
    msaa: wgpu::Texture,
    vbo: wgpu::Buffer,
    ibo: wgpu::Buffer,
    /// Ring of readback buffers: frames are submitted and copied into the
    /// next slot WITHOUT waiting; the oldest pending slot is mapped one or
    /// more frames later, so the GPU keeps a queue of work (no per-frame
    /// pipeline stall).
    readback_ring: Vec<wgpu::Buffer>,
    readback_next: usize,
    readback_pending: std::collections::VecDeque<usize>,
    /// Lazy GPU RGB→YUV420 conversion (see [`YuvPass`]); None until the
    /// first `render_deferred_yuv`.
    yuv: Option<YuvPass>,
    pub width: u32,
    pub height: u32,
    pub padded_row: u32,
}

fn sample_count() -> u32 {
    // Android (Mali/Adreno/SwiftShader): MSAA costs 4× the fragment work and
    // has driver-side resolve quirks that drop parts of the scene; the
    // software renderer benefits even more from disabling it.
    if cfg!(target_os = "android") || std::env::var("NO_MSAA").is_ok() { 1 } else { 4 }
}

pub fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    pollster::block_on(fut)
}

impl Renderer {
    pub fn new(width: u32, height: u32, atlas: &Atlas) -> Renderer {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter");
        Self::from_adapter(adapter, width, height, atlas)
    }

    /// Device + queue on a caller-provided adapter. The caller must obtain
    /// the adapter from the SAME `wgpu::Instance` that created any surface
    /// it intends to present to (a surface id from another instance is
    /// invalid and panics inside wgpu-core).

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// 诊断用一行 GPU/后端描述:后端、适配器名、vendor/device ID 与
    /// 驱动版本(日志记录用)。
    pub fn gpu_info(&self) -> String {
        let i = self.adapter.get_info();
        format!(
            "{:?} · {} · vendor {:#06x} device {:#06x} · driver {} ({})",
            i.backend, i.name, i.vendor, i.device, i.driver, i.driver_info
        )
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// 热替换图集(背景图开关时用):重建纹理并上传,重绑 bind group,
    /// 管线/设备/窗口全部保留——避免整套 wgpu 初始化重付。
    pub fn set_atlas(&mut self, atlas: &Atlas) {
        let tex_size = wgpu::Extent3d { width: atlas.width, height: atlas.height, depth_or_array_layers: 1 };
        let atlas_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width * 4),
                rows_per_image: Some(atlas.height),
            },
            tex_size,
        );
        let view = atlas_tex.create_view(&Default::default());
        self.atlas_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.atlas_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas_sampler) },
            ],
            label: Some("atlas bind"),
        });
        self.atlas_tex = atlas_tex;
    }

    /// GPU 侧把一张 Rgba8Unorm 纹理拷进图集的某个区域槽位(storyboard
    /// 合成层每帧刷新用)。`src` 的尺寸必须与区域一致;同一队列内
    /// 顺序在后续场景提交之前。
    pub fn copy_into_atlas(&self, src: &wgpu::Texture, atlas: &Atlas, region: crate::draw::Region) {
        let rect = atlas.region_rect(region);
        let (x, y) = (rect.x0 as u32, rect.y0 as u32);
        let size = wgpu::Extent3d {
            width: (rect.x1 - rect.x0) as u32,
            height: (rect.y1 - rect.y0) as u32,
            depth_or_array_layers: 1,
        };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("atlas slot copy"),
        });
        encoder.copy_texture_to_texture(
            src.as_image_copy(),
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            size,
        );
        self.queue.submit(Some(encoder.finish()));
    }

    pub fn target_view(&self) -> wgpu::TextureView {
        self.target.create_view(&Default::default())
    }

    pub fn from_adapter(
        adapter: wgpu::Adapter,
        width: u32,
        height: u32,
        atlas: &Atlas,
    ) -> Renderer {
        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("renderer"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("request device");

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(8),
                },
                count: None,
            }],
        });

        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&screen_layout, &atlas_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 32, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 48, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 64, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
            ],
        };

        let make_pipeline = |blend: wgpu::BlendState, sample_count: u32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scene pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[vertex_layout.clone()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            })
        };

        let premult_alpha = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let premult_add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let sc = sample_count();
        let pipeline_alpha = make_pipeline(premult_alpha, sc);
        let pipeline_additive = make_pipeline(premult_add, sc);


        // Atlas texture.
        let tex_size = wgpu::Extent3d { width: atlas.width, height: atlas.height, depth_or_array_layers: 1 };
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width * 4),
                rows_per_image: Some(atlas.height),
            },
            tex_size,
        );
        let atlas_view = atlas_tex.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let atlas_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &atlas_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
            label: Some("atlas bind"),
        });

        // Slider-body distance field: R16Float, min-blended capsule SDFs.
        let body_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("body layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let body_size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let body_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("body distfield"),
            size: body_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let body_view = body_tex.create_view(&Default::default());
        let body_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("body sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let body_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &body_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&body_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&body_sampler) },
            ],
            label: Some("body bind"),
        });

        let empty_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("empty layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let body_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("body main layout"),
            bind_group_layouts: &[&screen_layout, &body_layout],
            push_constant_ranges: &[],
        });

        let min_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Min,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Min,
            },
        };

        let prepass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prepass layout"),
            bind_group_layouts: &[&screen_layout],
            push_constant_ranges: &[],
        });
        let prepass_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PrepassVertex>() as u64, // 28
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 24, shader_location: 3, format: wgpu::VertexFormat::Float32 },
            ],
        };
        let body_pre_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("body prepass"),
            layout: Some(&prepass_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_body_pre"),
                compilation_options: Default::default(),
                buffers: &[prepass_vertex_layout.clone()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_body_pre"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R16Float,
                    blend: Some(min_blend),
                    write_mask: wgpu::ColorWrites::RED,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let body_main_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("body main"),
            layout: Some(&body_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout.clone()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_body_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    blend: Some(premult_alpha),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: sc, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
            cache: None,
        });

        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_data: [f32; 2] = [width as f32, height as f32];
        queue.write_buffer(&screen_buf, 0, cast_slice(&screen_data));
        let screen_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &screen_buf,
                    offset: 0,
                    size: Some(NonZeroU64::new(8).unwrap()),
                }),
            }],
            label: Some("screen bind"),
        });

        // Offscreen target + MSAA texture.
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa"),
            size,
            mip_level_count: 1,
            sample_count: sc,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let vbo = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vbo"),
            size: 4 << 20,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let ibo = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ibo"),
            size: 8 << 20,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let body_vbo = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("body vbo"),
            // 16 MiB ÷ (4 verts × 28 B) ≈ 149k slider segments per frame;
            // the observed dense-map spike was ~15k.
            size: 16 << 20,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let body_ibo = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("body ibo"),
            size: 8 << 20,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
        let padded_row = ((width * 4 + align - 1) / align) * align;

        let readback_ring: Vec<wgpu::Buffer> = (0..3)
            .map(|i| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("readback {i}")),
                    size: (padded_row * height) as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();

        Renderer {
            adapter,
            device,
            queue,
            body_tex,
            body_bind,
            body_pre_pipeline,
            body_main_pipeline,
            body_vbo,
            body_ibo,
            pipeline_alpha,
            pipeline_additive,
            atlas_bind,
            atlas_layout,
            atlas_sampler: sampler,
            atlas_tex,
            screen_bind,
            target,
            msaa,
            vbo,
            ibo,
            readback_ring,
            readback_next: 0,
            readback_pending: std::collections::VecDeque::new(),
            yuv: None,
            width,
            height,
            padded_row,
        }
    }

    /// Renders one frame and returns the BGRA pixels (with row padding).
    /// Synchronous convenience (PNG mode); the ffmpeg path uses
    /// `render_deferred` + `read_oldest_into` for pipelining.
    pub fn render(&mut self, list: &DrawList, clear: [f64; 4]) -> Vec<u8> {
        let encoder = self.encode_scene(list, clear);
        self.submit_frame_with(encoder);
        let mut out = Vec::new();
        self.read_oldest_into(&mut out);
        out
    }

    /// Builds the full scene command encoder (slider-body prepasses +
    /// composites + scene runs + MSAA resolve).
    pub fn encode_scene(&mut self, list: &DrawList, clear: [f64; 4]) -> wgpu::CommandEncoder {
        let vbytes = list.vertices.len() * std::mem::size_of::<Vertex>();
        let ibytes = list.indices.len() * 4;
        assert!(vbytes as u64 <= self.vbo.size(), "vertex buffer overflow: {}", vbytes);
        assert!(ibytes as u64 <= self.ibo.size(), "index buffer overflow: {}", ibytes);

        let use_msaa = sample_count() > 1;

        // ---- Slider bodies (lazer PathDrawNode style) --------------------
        // Per body: pass A min-blends that body's capsule-segment SDFs into
        // an R16Float field (cleared far beyond 1.0 so pixels covered by no
        // segment quad - e.g. corners of the body's AABB - stay transparent
        // instead of reading as the capsule edge). Pass B composites one quad
        // sampling the field (border band at the rim, body fill inside,
        // analytic AA), drawn under all scene elements. Each body gets its
        // own field pass so overlapping bodies never share distance values.
        let mut body_quads: Vec<Vertex> = Vec::new();
        let mut body_indices: Vec<u32> = Vec::new();
        let mut prepass_verts: Vec<PrepassVertex> = Vec::new();
        let mut prepass_indices: Vec<u32> = Vec::new();
        // Index range of each body's segments inside the prepass buffers.
        let mut prepass_ranges: Vec<(u32, u32)> = Vec::new();
        for body in &list.bodies {
            let pad = body.radius + 1.5;
            let mut minx = f32::MAX;
            let mut miny = f32::MAX;
            let mut maxx = f32::MIN;
            let mut maxy = f32::MIN;
            let start = prepass_indices.len() as u32;
            for (a, b) in &body.segments {
                minx = minx.min(a[0] - pad).min(b[0] - pad);
                miny = miny.min(a[1] - pad).min(b[1] - pad);
                maxx = maxx.max(a[0] + pad).max(b[0] + pad);
                maxy = maxy.max(a[1] + pad).max(b[1] + pad);

                let base = prepass_verts.len() as u32;
                for corner in [
                    [a[0].min(b[0]) - pad, a[1].min(b[1]) - pad],
                    [a[0].max(b[0]) + pad, a[1].min(b[1]) - pad],
                    [a[0].max(b[0]) + pad, a[1].max(b[1]) + pad],
                    [a[0].min(b[0]) - pad, a[1].max(b[1]) + pad],
                ] {
                    prepass_verts.push(PrepassVertex {
                        pos: corner,
                        start: [a[0], a[1]],
                        end: [b[0], b[1]],
                        radius: body.radius,
                    });
                }
                prepass_indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            prepass_ranges.push((start, prepass_indices.len() as u32));

            let base = body_quads.len() as u32;
            // Legacy gradient bodies smuggle the inner colour through the
            // idle channels: local.xy + uv.z (uv.w stays unused).
            let inner = body.inner_colour;
            for corner in [[minx, miny], [maxx, miny], [maxx, maxy], [minx, maxy]] {
                body_quads.push(Vertex {
                    pos: corner,
                    local: match inner {
                        Some(c) => [c.r, c.g],
                        None => [0.0; 2],
                    },
                    color: [body.body.r, body.body.g, body.body.b, body.body.a],
                    color2: [body.border_colour.r, body.border_colour.g, body.border_colour.b, body.border_colour.a],
                    uv: [corner[0], corner[1], inner.map(|c| c.b).unwrap_or(0.0), 0.0],
                    aux: [crate::draw::MODE_CAPSULE, body.radius, body.border, if inner.is_some() { 1.0 } else { 0.0 }],
                });
            }
            body_indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let pre_vbytes = prepass_verts.len() * std::mem::size_of::<Vertex>();
        let pre_ibytes = prepass_indices.len() * 4;
        assert!(pre_vbytes as u64 <= self.body_vbo.size(), "body vertex buffer overflow: {}", pre_vbytes);
        assert!(pre_ibytes as u64 <= self.body_ibo.size(), "body index buffer overflow: {}", pre_ibytes);

        // ---- Scene geometry (body composites prepended) ------------------
        let mut all_verts = body_quads.clone();
        let mut all_idx = body_indices.clone();
        all_verts.extend_from_slice(&list.vertices);
        for i in &list.indices {
            all_idx.push(*i + body_quads.len() as u32);
        }
        self.queue.write_buffer(&self.body_vbo, 0, cast_slice(&prepass_verts));
        self.queue.write_buffer(&self.body_ibo, 0, cast_slice(&prepass_indices));
        self.queue.write_buffer(&self.vbo, 0, cast_slice(&all_verts));
        self.queue.write_buffer(&self.ibo, 0, cast_slice(&all_idx));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame encoder"),
        });

        let target_view = self.target.create_view(&Default::default());
        let msaa_view = self.msaa.create_view(&Default::default());
        let clear_color = wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: clear[3] };

        let has_bodies = !list.bodies.is_empty();
        let body_view = self.body_tex.create_view(&Default::default());
        if has_bodies {
            // Clear the scene target first; the passes below must load.
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: if use_msaa { &msaa_view } else { &target_view },
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            }));

            // Interleave scene runs and body composites in draw order: each
            // body's composite lands where its slider sits in the object
            // ordering (lazer layers whole sliders by start time, so an
            // earlier slider's body covers later objects). Body marks split
            // any run that spans them.
            #[derive(Clone, Copy)]
            enum Op {
                Run(Blend, u32, u32),
                Body(usize),
            }
            let marks = &list.body_marks;
            let mut ops: Vec<Op> = Vec::new();
            let mut mi = 0usize;
            for &(blend, off, cnt) in &list.runs {
                let mut start = off;
                while mi < marks.len() && marks[mi].0 <= off {
                    ops.push(Op::Body(marks[mi].1));
                    mi += 1;
                }
                while mi < marks.len() && marks[mi].0 < off + cnt {
                    let (key, bi) = marks[mi];
                    if key > start {
                        ops.push(Op::Run(blend, start, key - start));
                    }
                    ops.push(Op::Body(bi));
                    start = key;
                    mi += 1;
                }
                if start < off + cnt {
                    ops.push(Op::Run(blend, start, off + cnt - start));
                }
            }
            while mi < marks.len() {
                ops.push(Op::Body(marks[mi].1));
                mi += 1;
            }

            // Group consecutive runs so each render pass is opened and
            // dropped within one statement (wgpu pass borrows the encoder).
            enum Seg {
                Runs(Vec<(Blend, u32, u32)>),
                Body(usize),
            }
            let mut segs: Vec<Seg> = Vec::new();
            for op in ops {
                match op {
                    Op::Run(blend, off, cnt) => match segs.last_mut() {
                        Some(Seg::Runs(runs)) => runs.push((blend, off, cnt)),
                        _ => segs.push(Seg::Runs(vec![(blend, off, cnt)])),
                    },
                    Op::Body(bi) => segs.push(Seg::Body(bi)),
                }
            }

            let base = body_indices.len() as u32;
            for seg in &segs {
                match seg {
                    Seg::Runs(runs) => {
                        let mut p = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("scene segment"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: if use_msaa { &msaa_view } else { &target_view },
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        p.set_bind_group(0, &self.screen_bind, &[]);
                        p.set_bind_group(1, &self.atlas_bind, &[]);
                        p.set_vertex_buffer(0, self.vbo.slice(..));
                        p.set_index_buffer(self.ibo.slice(..), wgpu::IndexFormat::Uint32);
                        for &(blend, off, cnt) in runs {
                            let pipeline = match blend {
                                Blend::Alpha => &self.pipeline_alpha,
                                Blend::Additive => &self.pipeline_additive,
                            };
                            p.set_pipeline(pipeline);
                            let o = off + base;
                            p.draw_indexed(o..(o + cnt), 0, 0..1);
                        }
                    }
                    Seg::Body(bi) => {
                        let (start, end) = prepass_ranges[*bi];
                        // Pass A: this body's segment SDFs, min-blended.
                        {
                            let mut pre = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("body prepass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &body_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        // Far above any real d/r (<= ~1.02
                                        // inside the segment quads): uncovered
                                        // pixels composite to zero alpha.
                                        load: wgpu::LoadOp::Clear(wgpu::Color {
                                            r: 256.0,
                                            g: 256.0,
                                            b: 256.0,
                                            a: 256.0,
                                        }),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });
                            pre.set_pipeline(&self.body_pre_pipeline);
                            pre.set_bind_group(0, &self.screen_bind, &[]);
                            pre.set_vertex_buffer(0, self.body_vbo.slice(..));
                            pre.set_index_buffer(self.body_ibo.slice(..), wgpu::IndexFormat::Uint32);
                            pre.draw_indexed(start..end, 0, 0..1);
                        }
                        // Pass B: composite this body over the scene.
                        {
                            let mut comp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("body composite"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: if use_msaa { &msaa_view } else { &target_view },
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });
                            comp.set_pipeline(&self.body_main_pipeline);
                            comp.set_bind_group(0, &self.screen_bind, &[]);
                            comp.set_bind_group(1, &self.body_bind, &[]);
                            comp.set_vertex_buffer(0, self.vbo.slice(..));
                            comp.set_index_buffer(self.ibo.slice(..), wgpu::IndexFormat::Uint32);
                            comp.draw_indexed((*bi * 6) as u32..(*bi * 6 + 6) as u32, 0, 0..1);
                        }
                    }
                }
            }

            if use_msaa {
                // Resolve the accumulated multisampled frame into the target.
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("resolve pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &msaa_view,
                        resolve_target: Some(&target_view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                }));
            }
        }

        if !has_bodies {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: if use_msaa { &msaa_view } else { &target_view },
                    resolve_target: if use_msaa { Some(&target_view) } else { None },
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_bind_group(0, &self.screen_bind, &[]);
            rpass.set_vertex_buffer(0, self.vbo.slice(..));
            rpass.set_index_buffer(self.ibo.slice(..), wgpu::IndexFormat::Uint32);

            rpass.set_bind_group(1, &self.atlas_bind, &[]);

            for (blend, offset, count) in &list.runs {
                let pipeline = match blend {
                    Blend::Alpha => &self.pipeline_alpha,
                    Blend::Additive => &self.pipeline_additive,
                };
                rpass.set_pipeline(pipeline);
                let off = offset + body_indices.len() as u32;
                rpass.draw_indexed(off..(off + *count), 0, 0..1);
            }
        }

        encoder
    }

    /// Renders one frame and SUBMITS it without waiting for the GPU; the
    /// copied-out frame data is retrieved later with `read_oldest` once
    /// `pending_len` frames are in flight.
    pub fn render_deferred(&mut self, list: &DrawList, clear: [f64; 4]) {
        let encoder = self.encode_scene(list, clear);
        self.submit_frame_with(encoder);
    }

    /// Number of submitted-but-not-yet-read frames.
    pub fn pending_len(&self) -> usize {
        self.readback_pending.len()
    }

    fn submit_frame_with(&mut self, mut encoder: wgpu::CommandEncoder) {
        let slot = self.readback_next;
        self.readback_next = (self.readback_next + 1) % self.readback_ring.len();
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_ring[slot],
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        self.readback_pending.push_back(slot);
    }

    /// Maps the OLDEST pending frame and copies it into `out` (reused
    /// between frames to avoid a per-frame allocation). With a frame or two
    /// of GPU work already queued this returns almost immediately; the GPU
    /// never starves while the CPU builds the next frame.
    pub fn read_oldest_into(&mut self, out: &mut Vec<u8>) {
        let slot = self
            .readback_pending
            .pop_front()
            .expect("read_oldest with empty pipeline");
        let buffer = &self.readback_ring[slot];
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        out.clear();
        out.extend_from_slice(&data);
        drop(data);
        buffer.unmap();
    }

    // ---- GPU YUV420 output (BGRA scene target → NV12/I420 buffer) ----

    /// Whether the GPU conversion supports this renderer's dimensions
    /// (width % 8 == 0, height % 2 == 0; every plane row must be
    /// word-aligned). Callers fall back to a CPU conversion otherwise.
    pub fn yuv_ready(&self) -> bool {
        self.width % 8 == 0 && self.height % 2 == 0
    }

    /// Packed NV12/I420 frame size: `w * h * 3 / 2` bytes.
    pub fn yuv_frame_bytes(&self) -> usize {
        (self.width as usize * self.height as usize * 3) / 2
    }

    fn init_yuv(&mut self) {
        assert!(self.yuv_ready(), "GPU YUV path requires width % 8 == 0 and even height");
        let (width, height) = (self.width, self.height);
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yuv shader"),
            source: wgpu::ShaderSource::Wgsl(YUV_SHADER.into()),
        });

        let params_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(8),
                },
                count: None,
            }],
        });
        let tex_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv tex layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let out_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv out layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("yuv layout"),
            bind_group_layouts: &[&params_layout, &tex_layout, &out_layout],
            push_constant_ranges: &[],
        });

        let make_pipeline = |entry: &str| {
            self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("yuv pipeline"),
                layout: Some(&layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let pipeline_y = make_pipeline("cs_y");
        let pipeline_uv_nv12 = make_pipeline("cs_uv_nv12");
        let pipeline_uv_i420 = make_pipeline("cs_uv_i420");

        let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("yuv params"),
            size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buf, 0, cast_slice(&[width, height]));
        let params_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params_buf,
                    offset: 0,
                    size: Some(NonZeroU64::new(8).unwrap()),
                }),
            }],
            label: Some("yuv params bind"),
        });

        let target_view = self.target.create_view(&Default::default());
        let tex_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &tex_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&target_view),
            }],
            label: Some("yuv tex bind"),
        });

        // w*h*3/2 is a multiple of 4 for any %8/%2 dimensions (copy size
        // requirement); round up anyway so odd cases still validate.
        let bytes = ((self.yuv_frame_bytes() + 3) / 4 * 4) as u32;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("yuv out"),
            size: bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let out_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &out_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &out_buf,
                    offset: 0,
                    size: None,
                }),
            }],
            label: Some("yuv out bind"),
        });
        let ring: Vec<wgpu::Buffer> = (0..3)
            .map(|i| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("yuv readback {i}")),
                    size: bytes as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();

        self.yuv = Some(YuvPass {
            pipeline_y,
            pipeline_uv_nv12,
            pipeline_uv_i420,
            params_bind,
            tex_bind,
            out_bind,
            out_buf,
            ring,
            ring_next: 0,
            pending: std::collections::VecDeque::new(),
            bytes,
            groups_y: ((width / 4 + 7) / 8, (height + 3) / 4),
            groups_uv: ((width / 8 + 7) / 8, (height / 2 + 3) / 4),
        });
    }

    /// Renders one frame and converts it to YUV420 on the GPU (NV12 when
    /// `interleaved`, I420 otherwise), submitting without waiting for the
    /// GPU — mirror of [`Renderer::render_deferred`]. Retrieve converted
    /// frames with [`Renderer::read_oldest_yuv_into`].
    pub fn render_deferred_yuv(&mut self, list: &DrawList, clear: [f64; 4], interleaved: bool) {
        if self.yuv.is_none() {
            self.init_yuv();
        }
        let slot = {
            let y = self.yuv.as_mut().unwrap();
            let s = y.ring_next;
            y.ring_next = (s + 1) % y.ring.len();
            s
        };
        let mut encoder = self.encode_scene(list, clear);
        {
            let y = self.yuv.as_ref().unwrap();
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("yuv pass"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &y.params_bind, &[]);
            pass.set_bind_group(1, &y.tex_bind, &[]);
            pass.set_bind_group(2, &y.out_bind, &[]);
            pass.set_pipeline(&y.pipeline_y);
            pass.dispatch_workgroups(y.groups_y.0, y.groups_y.1, 1);
            pass.set_pipeline(if interleaved { &y.pipeline_uv_nv12 } else { &y.pipeline_uv_i420 });
            pass.dispatch_workgroups(y.groups_uv.0, y.groups_uv.1, 1);
        }
        let y = self.yuv.as_ref().unwrap();
        encoder.copy_buffer_to_buffer(&y.out_buf, 0, &y.ring[slot], 0, y.bytes as u64);
        self.queue.submit(Some(encoder.finish()));
        self.yuv.as_mut().unwrap().pending.push_back(slot);
    }

    /// Number of submitted-but-not-yet-read YUV frames.
    pub fn yuv_pending_len(&self) -> usize {
        self.yuv.as_ref().map_or(0, |y| y.pending.len())
    }

    /// Maps the OLDEST pending YUV frame and copies it into `out`
    /// (`yuv_frame_bytes()` bytes). Mirror of [`Renderer::read_oldest_into`].
    pub fn read_oldest_yuv_into(&mut self, out: &mut Vec<u8>) {
        out.clear();
        out.resize(self.yuv_frame_bytes(), 0);
        self.read_oldest_yuv_into_slice(out);
    }

    /// [`Self::read_oldest_yuv_into`] writing straight into a caller-owned
    /// buffer (exactly `yuv_frame_bytes()` bytes) — the JNI export path
    /// hands its direct buffer over directly, skipping the extra
    /// full-frame copy through a staging Vec.
    pub fn read_oldest_yuv_into_slice(&mut self, out: &mut [u8]) {
        let slot = self
            .yuv
            .as_mut()
            .and_then(|y| y.pending.pop_front())
            .expect("read_oldest_yuv with empty pipeline");
        let buffer = &self.yuv.as_ref().unwrap().ring[slot];
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        out.copy_from_slice(&data);
        drop(data);
        buffer.unmap();
    }
}

/// Compact slider-body prepass vertex: 28 bytes (the fat 80-byte scene
/// Vertex wastes 60+ bytes per prepass corner).
#[repr(C)]
#[derive(Clone, Copy)]
struct PrepassVertex {
    pos: [f32; 2],
    start: [f32; 2],
    end: [f32; 2],
    radius: f32,
}

// Minimal bytemuck-style cast (avoids the extra dependency).
unsafe trait Pod {}
unsafe impl Pod for Vertex {}
unsafe impl Pod for PrepassVertex {}
unsafe impl Pod for u32 {}
unsafe impl Pod for f32 {}

fn cast_slice<T: Pod>(data: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}
