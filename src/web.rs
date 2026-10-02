use crate::{Input, Runtime, graphics::Renderer};
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub struct WebDemo {
    runtime: Runtime,
    renderer: Renderer,
}
#[wasm_bindgen]
impl WebDemo {
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        count: u32,
        backend: String,
    ) -> Result<WebDemo, JsValue> {
        console_error_panic_hook::set_once();
        let backends = match backend.as_str() {
            "webgpu" => wgpu::Backends::BROWSER_WEBGPU,
            "webgl" => wgpu::Backends::GL,
            "auto" => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            _ => return Err(JsValue::from_str("backend must be auto, webgpu or webgl")),
        };
        let runtime =
            crate::demo::scene(count, 42).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let descriptor = wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        };
        let instance = if backend == "auto" {
            wgpu::util::new_instance_with_webgpu_detection(&descriptor).await
        } else {
            wgpu::Instance::new(&descriptor)
        };
        let width = canvas.width();
        let height = canvas.height();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let renderer = Renderer::new(
            &instance,
            surface,
            width,
            height,
            runtime.bullet_count() + 1,
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(Self { runtime, renderer })
    }
    pub fn adapter(&self) -> String {
        self.renderer.adapter().to_string()
    }
    pub fn tick(&self) -> u64 {
        self.runtime.tick()
    }
    pub fn state_hash(&self) -> String {
        format!("{:016x}", self.runtime.state_hash())
    }
    pub fn step(&mut self, x: i32, y: i32) -> Result<(), JsValue> {
        self.runtime
            .set_input(Input { x, y })
            .and_then(|_| self.runtime.step())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }
    pub fn draw(&mut self) -> Result<bool, JsValue> {
        self.renderer
            .draw(&self.runtime)
            .map_err(|e| JsValue::from_str(&e))
    }
}
#[wasm_bindgen]
pub fn conformance_trace(ticks: u32) -> Result<Vec<u64>, JsValue> {
    if ticks > 100_000 {
        return Err(JsValue::from_str("maximum 100,000 trace ticks"));
    }
    Ok(crate::demo::trace(ticks))
}

#[wasm_bindgen]
pub struct WebGame {
    game: crate::game::Game,
    renderer: crate::graphics::GameRenderer,
    clock: crate::game::FrameClock,
}
#[wasm_bindgen]
impl WebGame {
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        backend: String,
        manifest: String,
        atlas: Vec<u8>,
        health: u32,
    ) -> Result<WebGame, JsValue> {
        console_error_panic_hook::set_once();
        let backends = match backend.as_str() {
            "webgpu" => wgpu::Backends::BROWSER_WEBGPU,
            "webgl" => wgpu::Backends::GL,
            "auto" => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            _ => return Err(JsValue::from_str("backend must be auto, webgpu or webgl")),
        };
        let pack = crate::resources::ResourcePack::from_json(&manifest, atlas)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut config = crate::game::GameConfig::default();
        if health > 0 {
            config.simulation.player.health = health;
        }
        let game =
            crate::game::Game::with_stage(config, 42, pack, crate::game::DemoStage::default())
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let descriptor = wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        };
        let instance = if backend == "auto" {
            wgpu::util::new_instance_with_webgpu_detection(&descriptor).await
        } else {
            wgpu::Instance::new(&descriptor)
        };
        let width = canvas.width();
        let height = canvas.height();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let cfg = game.simulation().config();
        let renderer = crate::graphics::GameRenderer::new(
            &instance,
            surface,
            width,
            height,
            cfg.projectile_capacity as usize + cfg.enemy_capacity as usize + 1024,
            game.resources(),
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(Self {
            game,
            renderer,
            clock: crate::game::FrameClock::default(),
        })
    }
    pub fn adapter(&self) -> String {
        self.renderer.adapter().to_string()
    }
    pub fn tick(&self) -> u64 {
        self.game.hud().tick
    }
    pub fn score(&self) -> u64 {
        self.game.hud().score
    }
    pub fn state_hash(&self) -> String {
        format!("{:016x}", self.game.state_hash())
    }
    pub fn resource_hash(&self) -> String {
        format!("{:016x}", self.game.resources().content_hash())
    }
    pub fn hud(&self) -> Vec<u32> {
        let h = self.game.hud();
        vec![
            h.health,
            h.bombs,
            h.phase,
            h.wave,
            h.boss_health,
            h.boss_max_health,
            h.projectiles,
            h.enemies,
            h.bomb_flash,
        ]
    }
    pub fn step(&mut self, x: i32, y: i32, flags: u32) -> Result<Vec<u32>, JsValue> {
        let input = crate::game::GameInput::from_flags(x, y, flags)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.game
            .step(input)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(self
            .game
            .audio_events()
            .iter()
            .map(|e| e.resource_id)
            .collect())
    }
    pub fn ticks_due(&mut self, milliseconds: f64, active: bool) -> Result<u32, JsValue> {
        if !milliseconds.is_finite() || !(0.0..=1_000_000.0).contains(&milliseconds) {
            return Err(JsValue::from_str("invalid frame interval"));
        }
        Ok(self.clock.advance(
            std::time::Duration::from_secs_f64(milliseconds / 1000.0),
            active,
        ) as u32)
    }
    pub fn reset_clock(&mut self) {
        self.clock.reset();
    }
    pub fn restart(&mut self) -> Result<(), JsValue> {
        self.clock.reset();
        self.game
            .restart()
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        self.clock.reset();
    }
    pub fn draw(&mut self) -> Result<bool, JsValue> {
        self.renderer
            .draw(&self.game)
            .map_err(|e| JsValue::from_str(&e))
    }
}
#[wasm_bindgen]
pub fn game_conformance_trace(frames: u32) -> Result<Vec<u64>, JsValue> {
    if frames > 100000 {
        return Err(JsValue::from_str("maximum 100,000 frames"));
    }
    Ok(crate::game::trace(frames))
}
