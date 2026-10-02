use super::Renderer;
use crate::{
    game::{Game, GamePhase, Stage},
    resources::{self, ResourcePack},
};
use std::{fmt::Write, sync::atomic::Ordering};
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteInstance {
    center: [f32; 2],
    size: [f32; 2],
    uv_origin: [f32; 2],
    uv_size: [f32; 2],
    color: [f32; 4],
}
pub struct GameRenderer {
    base: Renderer,
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    instances: Vec<SpriteInstance>,
    capacity: usize,
    resource_hash: u64,
}
impl GameRenderer {
    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        capacity: usize,
        resources: &ResourcePack,
    ) -> Result<Self, String> {
        for &c in resources::FONT_CHARACTERS {
            if resources
                .sprite(resources::FONT_ID_BASE + u32::from(c))
                .is_none()
            {
                return Err(format!("missing HUD glyph {c}"));
            }
        }
        let base = Renderer::new(instance, surface, width, height, capacity).await?;
        let device = &base.device;
        if resources.width() > device.limits().max_texture_dimension_2d
            || resources.height() > device.limits().max_texture_dimension_2d
        {
            return Err("atlas exceeds this backend's texture limits".into());
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stage atlas"),
            size: wgpu::Extent3d {
                width: resources.width(),
                height: resources.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        base.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            resources.atlas(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(resources.width() * 4),
                rows_per_image: Some(resources.height()),
            },
            texture.size(),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pixel atlas sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("textured stage sprites"),
            source: wgpu::ShaderSource::Wgsl(include_str!("game.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let attributes = wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x2,3=>Float32x2,4=>Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite atlas pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: base.config.format,
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
            label: Some("reused sprite instances"),
            size: (capacity * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            base,
            pipeline,
            bind,
            buffer,
            instances: Vec::with_capacity(capacity),
            capacity,
            resource_hash: resources.content_hash(),
        })
    }
    pub fn adapter(&self) -> &str {
        self.base.adapter()
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.base.resize(width, height);
    }
    fn sprite(
        &mut self,
        pack: &ResourcePack,
        id: u32,
        position: [f32; 2],
        size: [f32; 2],
        rgba: u32,
        world: [f32; 2],
    ) -> Result<(), String> {
        if self.instances.len() == self.capacity {
            return Err("sprite/HUD capacity exceeded".into());
        }
        let s = pack
            .sprite(id)
            .ok_or_else(|| format!("missing sprite {id}"))?;
        self.instances.push(SpriteInstance {
            center: [
                position[0] / world[0] * 2.0 - 1.0,
                1.0 - position[1] / world[1] * 2.0,
            ],
            size: [size[0] / world[0], size[1] / world[1]],
            uv_origin: [
                s.x as f32 / pack.width() as f32,
                s.y as f32 / pack.height() as f32,
            ],
            uv_size: [
                s.width as f32 / pack.width() as f32,
                s.height as f32 / pack.height() as f32,
            ],
            color: [
                ((rgba >> 24) & 255) as f32 / 255.0,
                ((rgba >> 16) & 255) as f32 / 255.0,
                ((rgba >> 8) & 255) as f32 / 255.0,
                (rgba & 255) as f32 / 255.0,
            ],
        });
        Ok(())
    }
    fn text(
        &mut self,
        pack: &ResourcePack,
        bytes: &[u8],
        position: [f32; 2],
        scale: f32,
        rgba: u32,
        world: [f32; 2],
    ) -> Result<(), String> {
        for (i, &c) in bytes.iter().enumerate() {
            if c != b' ' {
                self.sprite(
                    pack,
                    resources::FONT_ID_BASE + u32::from(c),
                    [
                        position[0] + i as f32 * 6.0 * scale + 2.5 * scale,
                        position[1] + 3.5 * scale,
                    ],
                    [5.0 * scale, 7.0 * scale],
                    rgba,
                    world,
                )?;
            }
        }
        Ok(())
    }
    fn compose<S: Stage>(&mut self, game: &Game<S>) -> Result<[f32; 2], String> {
        self.instances.clear();
        let pack = game.resources();
        let cfg = game.simulation().config();
        let hud = game.hud();
        let world = [cfg.width.to_f32(), cfg.height.to_f32() + 128.0];
        let offset = 64.0;
        for star in 0..64u32 {
            let x = (star * 139 + 31) % cfg.width.to_f32().ceil().max(1.0) as u32;
            let y = ((u64::from(star) * 83 + hud.tick / 3)
                % cfg.height.to_f32().ceil().max(1.0) as u64) as f32;
            self.sprite(
                pack,
                resources::STAR,
                [x as f32, y + offset],
                [3.0, 3.0],
                0x6b8cb066,
                world,
            )?;
        }
        for layer in [10, 20, 30] {
            for s in game.sprites().filter(|s| s.layer == layer) {
                self.sprite(
                    pack,
                    s.resource_id,
                    [s.x, s.y + offset],
                    [s.width, s.height],
                    s.rgba,
                    world,
                )?;
            }
        }
        self.sprite(
            pack,
            resources::SOLID,
            [world[0] / 2.0, 32.0],
            [world[0], 64.0],
            0x142435ff,
            world,
        )?;
        let mut line = StackText::default();
        write!(
            &mut line,
            "GRAZER   {:02}:{:02}",
            hud.tick / 3600,
            (hud.tick / 60) % 60
        )
        .map_err(|e| e.to_string())?;
        self.text(pack, line.as_bytes(), [12.0, 10.0], 2.0, 0xc3edffff, world)?;
        line.clear();
        write!(&mut line, "SCORE {:06}", hud.score).map_err(|e| e.to_string())?;
        self.text(
            pack,
            line.as_bytes(),
            [world[0] - 180.0, 10.0],
            1.5,
            0xffeab2ff,
            world,
        )?;
        for i in 0..hud.health.min(9) {
            self.sprite(
                pack,
                resources::HEART,
                [20.0 + i as f32 * 20.0, 44.0],
                [16.0, 16.0],
                0xff919dff,
                world,
            )?;
        }
        for i in 0..hud.bombs.min(9) {
            self.sprite(
                pack,
                resources::BOMB,
                [world[0] - 20.0 - i as f32 * 20.0, 44.0],
                [15.0, 15.0],
                0xb9adffff,
                world,
            )?;
        }
        if hud.boss_max_health > 0 {
            self.sprite(
                pack,
                resources::SOLID,
                [world[0] / 2.0, 76.0],
                [world[0] - 40.0, 8.0],
                0x542739ff,
                world,
            )?;
            let length = (world[0] - 40.0) * hud.boss_health as f32 / hud.boss_max_health as f32;
            self.sprite(
                pack,
                resources::SOLID,
                [20.0 + length / 2.0, 76.0],
                [length, 8.0],
                0xf795d4ff,
                world,
            )?;
        }
        self.text(
            pack,
            b"ARROWS MOVE   Z SHOOT   X BOMB",
            [14.0, world[1] - 45.0],
            1.7,
            0x90b7d4ff,
            world,
        )?;
        self.text(
            pack,
            b"SHIFT FOCUS   P PAUSE   R RESTART",
            [14.0, world[1] - 23.0],
            1.5,
            0x90b7d4ff,
            world,
        )?;
        if hud.bomb_flash > 0 {
            self.sprite(
                pack,
                resources::SOLID,
                [world[0] / 2.0, (world[1] - 128.0) / 2.0 + 64.0],
                [world[0], world[1] - 128.0],
                0x81b4ff30,
                world,
            )?;
        }
        if game.phase() != GamePhase::Playing {
            self.sprite(
                pack,
                resources::SOLID,
                [world[0] / 2.0, world[1] / 2.0],
                [world[0], 130.0],
                0x122438e8,
                world,
            )?;
            let title: &[u8] = match game.phase() {
                GamePhase::Cleared => b"STAGE CLEAR",
                GamePhase::GameOver => b"GAME OVER",
                _ => b"STAGE ERROR",
            };
            self.text(
                pack,
                title,
                [
                    world[0] / 2.0 - title.len() as f32 * 9.0,
                    world[1] / 2.0 - 28.0,
                ],
                3.0,
                0xffeab2ff,
                world,
            )?;
            self.text(
                pack,
                b"PRESS R TO RESTART",
                [world[0] / 2.0 - 102.0, world[1] / 2.0 + 12.0],
                2.0,
                0xc3edffff,
                world,
            )?;
        }
        Ok(world)
    }
    pub fn draw<S: Stage>(&mut self, game: &Game<S>) -> Result<bool, String> {
        if game.resources().content_hash() != self.resource_hash {
            return Err("resource pack changed; recreate the renderer".into());
        }
        if self.base.lost.load(Ordering::Relaxed) {
            return Err("GPU device lost; recreate renderer".into());
        }
        let world = self.compose(game)?;
        let output = match self.base.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.base
                    .surface
                    .configure(&self.base.device, &self.base.config);
                return Ok(false);
            }
            Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => return Ok(false),
            Err(e) => return Err(e.to_string()),
        };
        self.base
            .queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.instances));
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self.base.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            let scale = (self.base.config.width as f32 / world[0])
                .min(self.base.config.height as f32 / world[1]);
            pass.set_viewport(
                (self.base.config.width as f32 - world[0] * scale) / 2.0,
                (self.base.config.height as f32 - world[1] * scale) / 2.0,
                world[0] * scale,
                world[1] * scale,
                0.0,
                1.0,
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw(0..6, 0..self.instances.len() as u32);
        }
        self.base.queue.submit([encoder.finish()]);
        output.present();
        Ok(true)
    }
}
struct StackText {
    bytes: [u8; 96],
    len: usize,
}
impl Default for StackText {
    fn default() -> Self {
        Self {
            bytes: [0; 96],
            len: 0,
        }
    }
}
impl StackText {
    fn clear(&mut self) {
        self.len = 0;
    }
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
impl std::fmt::Write for StackText {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let end = self.len + text.len();
        let out = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        out.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
