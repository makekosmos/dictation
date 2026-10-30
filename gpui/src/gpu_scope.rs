//! HEX's original WGSL Dot Scope shader, run through WGPU and presented as a
//! GPUI image because GPUI does not expose its compositor's render pass.
use std::sync::Arc;

use ::gpui::*;
use image::{Frame, ImageBuffer, Rgba};

use anyhow::Context as _;

use crate::dot_scope::{palette, Scope};

const WIDTH: u32 = 380;
const HEIGHT: u32 = 126;
const MAX_DOTS: usize = 1024;

pub(crate) type SharedGpuScope = std::sync::Arc<std::sync::Mutex<GpuScopeRenderer>>;

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    resolution: [f32; 2],
    backing_scale: f32,
    width: f32,
    height: f32,
    opacity: f32,
    scale: f32,
    softness: f32,
    pitch: f32,
    columns: f32,
    rows: f32,
    glow: f32,
    grid: f32,
    live: f32,
    cool: f32,
    dark: f32,
    base: [f32; 4],
    core: [f32; 4],
    plate: [f32; 4],
    rim: [f32; 4],
    shadow: [f32; 4],
    plate_layout: [f32; 4],
    island: [f32; 4],
    field: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<Uniforms>() == 192);

pub struct GpuScopeRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    dots: wgpu::Buffer,
    output: wgpu::Texture,
    readback: wgpu::Buffer,
    row_pitch: u32,
}

impl GpuScopeRenderer {
    pub fn new() -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("HEX Dot Scope"),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await?;

        let shader_source = include_str!("../assets/rune_hud.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HEX Dot Scope WGSL"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HEX Dot Scope uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dots = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HEX Dot Scope dots"),
            size: (MAX_DOTS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HEX Dot Scope bindings"),
            entries: &[0, 1].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HEX Dot Scope bindings"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dots.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HEX Dot Scope pipeline layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let constants = [("linear_target", 1.0)];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HEX Dot Scope pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("indicator_vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("dot_scope_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HEX Dot Scope frame"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let row_pitch = WIDTH * 4;
        let row_pitch = row_pitch.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HEX Dot Scope readback"),
            size: u64::from(row_pitch) * u64::from(HEIGHT),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            uniforms,
            dots,
            output,
            readback,
            row_pitch,
        })
    }

    pub fn render(&mut self, scope: &Scope) -> anyhow::Result<Arc<RenderImage>> {
        let pose = scope.pose();
        let colors = palette(true, &pose, 1.0);
        let rgba = |color: [f32; 3]| [color[0], color[1], color[2], 1.0];
        let uniforms = Uniforms {
            resolution: [WIDTH as f32, HEIGHT as f32],
            backing_scale: 1.0,
            width: pose.width,
            height: pose.height,
            opacity: pose.opacity,
            scale: pose.scale,
            softness: pose.softness,
            pitch: 2.6,
            columns: scope.columns() as f32,
            rows: scope.rows() as f32,
            glow: pose.glow,
            grid: pose.grid,
            live: pose.live,
            cool: pose.cool,
            dark: 1.0,
            base: rgba(colors.base),
            core: rgba(colors.core),
            plate: colors.plate,
            rim: colors.rim,
            shadow: colors.shadow,
            plate_layout: [0.0, 0.0, 1.0, 0.0],
            island: [0.0; 4],
            field: [pose.height / 2.0, 0.0, 0.0, 0.0],
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&uniforms as *const Uniforms).cast::<u8>(),
                std::mem::size_of::<Uniforms>(),
            )
        };
        self.queue.write_buffer(&self.uniforms, 0, bytes);
        let mut packed = vec![0u32; MAX_DOTS];
        for (to, value) in packed.iter_mut().zip(scope.dots()) {
            let power = (value[0].clamp(0.0, 1.0) * 65535.0).round() as u32;
            let fresh = (value[1].clamp(0.0, 1.0) * 65535.0).round() as u32;
            *to = power | (fresh << 16);
        }
        let dot_bytes =
            unsafe { std::slice::from_raw_parts(packed.as_ptr().cast::<u8>(), packed.len() * 4) };
        self.queue.write_buffer(&self.dots, 0, dot_bytes);

        let view = self.output.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("HEX Dot Scope"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            self.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.row_pitch),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        receiver.recv()??;
        let mapped = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
        for row in mapped
            .chunks_exact(self.row_pitch as usize)
            .take(HEIGHT as usize)
        {
            pixels.extend_from_slice(&row[..(WIDTH * 4) as usize]);
        }
        drop(mapped);
        self.readback.unmap();
        let image = ImageBuffer::<Rgba<u8>, _>::from_raw(WIDTH, HEIGHT, pixels)
            .context("invalid WGPU frame buffer")?;
        Ok(Arc::new(RenderImage::new(vec![Frame::new(image)])))
    }
}
