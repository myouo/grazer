//! Optional instanced-circle presentation shared by desktop and browser demos.
//! This module never changes authoritative simulation state.
use crate::Runtime;
mod game;
pub use game::GameRenderer;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    center: [f32; 2],
    radius: [f32; 2],
    color: [f32; 4],
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    instances: Vec<Instance>,
    capacity: usize,
    lost: Arc<AtomicBool>,
    adapter: String,
}

impl Renderer {
    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        capacity: usize,
    ) -> Result<Self, String> {
        if capacity == 0 || capacity > 1_000_001 {
            return Err("invalid renderer capacity".into());
        }
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        let adapter_name = format!("{} / {:?}", info.name, info.backend);
        let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("grazer device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let lost = Arc::new(AtomicBool::new(false));
        let lost_callback = lost.clone();
        device.set_device_lost_callback(move |_, _| {
            lost_callback.store(true, Ordering::Relaxed);
        });
        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface has no supported configuration")?;
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grazer circles"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grazer sprite pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grazer instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            buffer,
            instances: Vec::with_capacity(capacity),
            capacity,
            lost,
            adapter: adapter_name,
        })
    }
    pub fn adapter(&self) -> &str {
        &self.adapter
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
    /// Returns false for a recoverable surface failure; caller retries next frame.
    /// Device loss is reported as an error; hosts can rebuild the renderer while
    /// keeping their simulation. Zero-size surfaces must be paused by the host.
    pub fn draw(&mut self, runtime: &Runtime) -> Result<bool, String> {
        if self.lost.load(Ordering::Relaxed) {
            return Err("GPU device lost; recreate the renderer".into());
        }
        if runtime.bullet_count() + 1 > self.capacity {
            return Err("renderer capacity exceeded".into());
        }
        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(false);
            }
            Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => return Ok(false),
            Err(e) => return Err(e.to_string()),
        };
        let world = runtime.config();
        let w = world.width.to_f32();
        let h = world.height.to_f32();
        self.instances.clear();
        for s in runtime.sprites() {
            self.instances.push(Instance {
                center: [s.x / w * 2.0 - 1.0, 1.0 - s.y / h * 2.0],
                radius: [s.radius / w * 2.0, s.radius / h * 2.0],
                color: [
                    ((s.rgba >> 24) & 255) as f32 / 255.0,
                    ((s.rgba >> 16) & 255) as f32 / 255.0,
                    ((s.rgba >> 8) & 255) as f32 / 255.0,
                    (s.rgba & 255) as f32 / 255.0,
                ],
            });
        }
        // Draw the player last so it remains visible in dense workloads.
        self.instances.rotate_left(1);
        self.queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.instances));
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("grazer frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.015,
                            g: 0.02,
                            b: 0.045,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let scale = (self.config.width as f32 / w).min(self.config.height as f32 / h);
            pass.set_viewport(
                (self.config.width as f32 - w * scale) / 2.0,
                (self.config.height as f32 - h * scale) / 2.0,
                w * scale,
                h * scale,
                0.0,
                1.0,
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw(0..6, 0..self.instances.len() as u32);
        }
        self.queue.submit([encoder.finish()]);
        output.present();
        Ok(true)
    }
}
