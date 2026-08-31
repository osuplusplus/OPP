//! Window-surface rendering (live preview): the scene is rendered with the
//! offscreen `Renderer` pipeline into the internal target texture, then
//! blitted (letterboxed) onto a window surface and presented. This keeps a
//! single GPU context shared by the embedder's window.

use crate::draw::{Atlas, DrawList};
use crate::render::Renderer;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

pub struct SurfaceRenderer {
    renderer: Renderer,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    blit_pipeline: wgpu::RenderPipeline,
    params_buf: wgpu::Buffer,
    params_bind: wgpu::BindGroup,
    blit_bind: wgpu::BindGroup,
    surface_format: wgpu::TextureFormat,
    /// Composite alpha mode supported by the platform (Android only allows
    /// `Inherit`; desktop prefers `Opaque`).
    alpha_mode: wgpu::CompositeAlphaMode,
    /// (surface_w, surface_h); 0x0 means "not yet usable, skip frames".
    surface_size: (u32, u32),
    frame_aspect: f32,
}

const BLIT_SHADER: &str = r#"
struct Params {
    src_aspect: f32,
    dst_aspect: f32,
};
@group(0) @binding(0) var<uniform> params: Params;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Full-screen quad.
    var quad = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    var out: VsOut;
    let p = quad[vi];
    // Letterbox: shrink on the axis that overflows.
    var q = p;
    if (params.src_aspect > params.dst_aspect) {
        q.y = p.y * params.dst_aspect / params.src_aspect;
    } else {
        q.x = p.x * params.src_aspect / params.dst_aspect;
    }
    out.pos = vec4<f32>(q, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(tex, samp, in.uv, 0.0);
}
"#;

fn f32_bytes(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

impl SurfaceRenderer {
    /// 诊断用一行 GPU/后端描述(委托内部 Renderer)。
    pub fn gpu_info(&self) -> String {
        self.renderer.gpu_info()
    }

    /// Creates the renderer plus a wgpu surface for the given raw window
    /// handle (Windows: Win32;Linux: Xlib/XWayland)。The scene is rendered
    /// internally at `width`x`height` and letterboxed onto the window.
    pub fn new(
        width: u32,
        height: u32,
        atlas: &Atlas,
        raw_display: RawDisplayHandle,
        raw_window: RawWindowHandle,
    ) -> Result<SurfaceRenderer, String> {
        // 单一 Instance:surface 与 adapter 必须同源,跨实例的 surface id
        // 在 wgpu-core 里直接 panic("Surface does not exist")。
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: raw_display,
                raw_window_handle: raw_window,
            })
        }
        .map_err(|e| format!("create surface: {e:?}"))?;
        let adapter = crate::render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or("没有支持该窗口的 GPU 适配器")?;

        let renderer = Renderer::from_adapter(adapter, width, height, atlas);
        let device = renderer.device().clone();
        let queue = renderer.queue().clone();
        let frame_aspect = width as f32 / height as f32;

        let caps = surface.get_capabilities(renderer.adapter());
        // The scene target stores display-encoded (sRGB) byte values and
        // the blit copies them verbatim, so the swapchain must be a plain
        // UNORM variant: an `*Srgb` format would re-encode on store and
        // wash the whole image out. Bgra8Unorm first (desktop GL/DX12
        // list it); Android Vulkan offers only Rgba8* — then any non-sRGB
        // format beats `formats.first()`, which is driver-order and
        // commonly the Srgb twin.
        let surface_format = if caps.formats.iter().any(|f| *f == wgpu::TextureFormat::Bgra8Unorm) {
            wgpu::TextureFormat::Bgra8Unorm
        } else if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            *f
        } else {
            caps.formats.first().copied().unwrap_or(wgpu::TextureFormat::Bgra8Unorm)
        };
        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|m| *m == wgpu::CompositeAlphaMode::Opaque)
            .or_else(|| caps.alpha_modes.first().copied())
            .unwrap_or(wgpu::CompositeAlphaMode::Inherit);

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(8),
                },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit tex layout"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit layout"),
            bind_group_layouts: &[&params_layout, &tex_layout],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit params"),
            size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit params bind"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params_buf,
                    offset: 0,
                    size: std::num::NonZeroU64::new(8),
                }),
            }],
        });
        let target_view = renderer.target_view();
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit bind"),
            layout: &tex_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&target_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        Ok(SurfaceRenderer {
            renderer,
            surface,
            device,
            queue,
            blit_pipeline,
            params_buf,
            params_bind,
            blit_bind,
            surface_format,
            alpha_mode,
            surface_size: (0, 0),
            frame_aspect,
        })
    }

    /// 热替换图集(透传给内部离屏 Renderer)。
    pub fn set_atlas(&mut self, atlas: &crate::draw::Atlas) {
        self.renderer.set_atlas(atlas);
    }

    /// (Re)configures the surface for the window's current size; call on
    /// resize. `0` extents mean "hide" — frames are skipped until a real
    /// size arrives.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.surface_size == (width, height) {
            return;
        }
        self.surface_size = (width, height);
        if width == 0 || height == 0 {
            return;
        }
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface_format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: self.alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
        let params: [f32; 2] = [self.frame_aspect, width as f32 / height as f32];
        self.queue.write_buffer(&self.params_buf, 0, f32_bytes(&params));
    }

    /// Renders one scene list and presents it. Returns false when the frame
    /// was skipped (surface hidden or unavailable).
    pub fn render(&mut self, list: &DrawList, clear: [f64; 4]) -> bool {
        let (w, h) = self.surface_size;
        if w == 0 || h == 0 {
            return false;
        }
        let mut encoder = self.renderer.encode_scene(list, clear);
        let Ok(frame) = self.surface.get_current_texture() else { return false };
        let view = frame.texture.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &self.params_bind, &[]);
            pass.set_bind_group(1, &self.blit_bind, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        // No device.poll here: window presentation provides the needed
        // synchronization, and Maintain::Poll busy-syncs against the driver
        // on some Android GPUs, eating CPU and stalling the frame loop.
        true
    }
}
