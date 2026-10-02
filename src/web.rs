use crate::{Input, Runtime, graphics::Renderer};
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub struct WebFrameBenchmark {
    world: crate::Simulation,
    renderer: Renderer,
    count: u32,
    hits: u64,
    minimum: u32,
}
#[wasm_bindgen]
impl WebFrameBenchmark {
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        backend: String,
        count: u32,
    ) -> Result<Self, JsValue> {
        if count == 0 || count > 1_000_000 {
            return Err(JsValue::from_str("benchmark count must be 1..1,000,000"));
        }
        let backends = match backend.as_str() {
            "webgpu" => wgpu::Backends::BROWSER_WEBGPU,
            "webgl" => wgpu::Backends::GL,
            _ => {
                return Err(JsValue::from_str(
                    "benchmark backend must be webgpu or webgl",
                ));
            }
        };
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let width = canvas.width();
        let height = canvas.height();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let renderer = Renderer::new(&instance, surface, width, height, count as usize + 1)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        let world =
            crate::performance::scene(count, 42).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self {
            world,
            renderer,
            count,
            hits: 0,
            minimum: count,
        })
    }
    pub fn adapter(&self) -> String {
        self.renderer.adapter().into()
    }
    pub fn step(&mut self) -> Result<(), JsValue> {
        if self.world.projectile_count() != self.count as usize {
            return Err(JsValue::from_str("incomplete workload before step"));
        }
        self.world
            .step_with_input(crate::performance::input(self.world.tick()))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.minimum = self.minimum.min(self.world.projectile_count() as u32);
        self.hits += self
            .world
            .events()
            .iter()
            .filter(|e| matches!(e, crate::Event::Hit { .. }))
            .count() as u64;
        Ok(())
    }
    pub fn replenish(&mut self) -> Result<(), JsValue> {
        crate::performance::replenish(&mut self.world, self.count)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn draw(&mut self) -> Result<bool, JsValue> {
        self.renderer
            .draw_simulation(&self.world)
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn count(&self) -> u32 {
        self.world.projectile_count() as u32
    }
    pub fn metrics(&self) -> String {
        serde_json::json!({"hash":format!("{:016x}",self.world.state_hash()),"grazes":self.world.player().grazes,"hits":self.hits,"minimumAfterStep":self.minimum,"tick":self.world.tick(),"playerX":self.world.player().position.x.to_f32(),"playerY":self.world.player().position.y.to_f32()}).to_string()
    }
}
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
    session: crate::game::debug::DebugSession<crate::language::ScriptStage>,
    renderer: Option<crate::graphics::GameRenderer>,
    canvas: web_sys::HtmlCanvasElement,
    backend: String,
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
        Self::create_with_source(
            canvas,
            backend,
            manifest,
            atlas,
            health,
            include_str!("../assets/demo/advanced_showcase.graze").into(),
        )
        .await
    }
    pub async fn create_with_source(
        canvas: web_sys::HtmlCanvasElement,
        backend: String,
        manifest: String,
        atlas: Vec<u8>,
        health: u32,
        source: String,
    ) -> Result<WebGame, JsValue> {
        Self::create_with_difficulty(canvas, backend, manifest, atlas, health, source, 1).await
    }
    pub async fn create_with_difficulty(
        canvas: web_sys::HtmlCanvasElement,
        backend: String,
        manifest: String,
        atlas: Vec<u8>,
        health: u32,
        source: String,
        difficulty: u32,
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
        let stage = crate::language::ScriptStage::compile(
            "stage.graze",
            &source,
            crate::language::VmLimits::default(),
            42,
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let difficulty = crate::advanced::Difficulty::from_u32(difficulty)
            .ok_or_else(|| JsValue::from_str("difficulty must be 0, 1 or 2"))?;
        let game = if stage.vm().program().uses_advanced() {
            crate::game::Game::with_advanced_stage(
                config,
                42,
                pack,
                stage,
                crate::advanced::AdvancedConfig {
                    difficulty,
                    ..crate::advanced::AdvancedConfig::default()
                },
            )
        } else {
            crate::game::Game::with_stage(config, 42, pack, stage)
        }
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
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let renderer = crate::graphics::GameRenderer::new(
            &instance,
            surface,
            width,
            height,
            game.presentation_capacity(),
            game.resources(),
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(Self {
            session: crate::game::debug::DebugSession::new(game)
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
            renderer: Some(renderer),
            canvas,
            backend,
            clock: crate::game::FrameClock::default(),
        })
    }
    pub fn adapter(&self) -> String {
        self.renderer.as_ref().map_or_else(
            || "Renderer unavailable".into(),
            |r| r.adapter().to_string(),
        )
    }
    pub fn device_lost(&self) -> bool {
        self.renderer.as_ref().is_none_or(|r| r.is_lost())
    }
    /// Recreate presentation while retaining the exact Game/replay state.
    pub async fn recover_renderer(&mut self) -> Result<(), JsValue> {
        self.session.set_paused(true);
        self.clock.reset();
        self.renderer.take();
        let backends = match self.backend.as_str() {
            "webgpu" => wgpu::Backends::BROWSER_WEBGPU,
            "webgl" => wgpu::Backends::GL,
            _ => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
        };
        let descriptor = wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        };
        let instance = if self.backend == "auto" {
            wgpu::util::new_instance_with_webgpu_detection(&descriptor).await
        } else {
            wgpu::Instance::new(&descriptor)
        };
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(self.canvas.clone()))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let renderer = crate::graphics::GameRenderer::new(
            &instance,
            surface,
            self.canvas.width(),
            self.canvas.height(),
            self.session.game().presentation_capacity(),
            self.session.game().resources(),
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        self.renderer = Some(renderer);
        Ok(())
    }
    pub fn load_project(&mut self, bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
        let project = crate::project::Project::from_bytes(&bytes)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let prepared = self
            .renderer
            .as_ref()
            .ok_or_else(|| JsValue::from_str("renderer unavailable"))?
            .prepare_resources(project.resources())
            .map_err(|e| JsValue::from_str(&e))?;
        let game = project
            .create_game()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let old = self
            .session
            .replace_game(game)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.renderer
            .as_mut()
            .expect("prepared renderer")
            .commit_resources(prepared);
        self.clock.reset();
        old.map_or(Ok(Vec::new()), |r| {
            r.to_bytes().map_err(|e| JsValue::from_str(&e.to_string()))
        })
    }
    pub fn tick(&self) -> u64 {
        self.session.game().hud().tick
    }
    pub fn score(&self) -> u64 {
        self.session.game().hud().score
    }
    pub fn state_hash(&self) -> String {
        format!("{:016x}", self.session.game().state_hash())
    }
    pub fn resource_hash(&self) -> String {
        format!("{:016x}", self.session.game().resources().content_hash())
    }
    pub fn diagnostic(&self) -> String {
        self.session
            .game()
            .diagnostic()
            .map_or_else(String::new, ToString::to_string)
    }
    pub fn program_hash(&self) -> String {
        format!(
            "{:016x}",
            self.session.game().stage().vm().program().content_hash()
        )
    }
    pub fn hud(&self) -> Vec<u32> {
        let h = self.session.game().hud();
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
    pub fn advanced_hud(&self) -> Vec<u64> {
        self.session
            .game()
            .advanced_hud()
            .map_or_else(Vec::new, |h| {
                vec![
                    u64::from(h.difficulty),
                    u64::from(h.power),
                    u64::from(h.drops),
                    u64::from(h.boss_phase),
                    u64::from(h.phase_ticks),
                    u64::from(h.phases_started),
                    h.collected,
                    h.cancelled,
                    h.phase_bonus,
                ]
            })
    }
    /// Bulk beam geometry, ten numeric fields per segment, matching the C POD.
    pub fn laser_segments(&self) -> Vec<f64> {
        self.session
            .game()
            .laser_segments()
            .flat_map(|s| {
                [
                    f64::from(s.slot),
                    f64::from(s.generation),
                    f64::from(s.segment),
                    f64::from(s.phase),
                    f64::from(s.x1),
                    f64::from(s.y1),
                    f64::from(s.x2),
                    f64::from(s.y2),
                    f64::from(s.width),
                    f64::from(s.rgba),
                ]
            })
            .collect()
    }
    pub fn step(&mut self, x: i32, y: i32, flags: u32) -> Result<Vec<u32>, JsValue> {
        let input = crate::game::GameInput::from_flags(x, y, flags)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let before = self.session.game().hud().tick;
        let advanced = if self.session.paused() {
            self.session.single_step(input)
        } else {
            self.session.advance(input)
        }
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        if !advanced || self.session.game().hud().tick != before.saturating_add(1) {
            return Ok(Vec::new());
        }
        Ok(self
            .session
            .game()
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
            active && !self.session.paused(),
        ) as u32)
    }
    pub fn reset_clock(&mut self) {
        self.clock.reset();
    }
    pub fn set_paused(&mut self, value: bool) {
        self.session.set_paused(value);
        self.clock.reset();
    }
    pub fn set_hitboxes(&mut self, value: bool) {
        self.session.set_hitboxes(value);
    }
    pub fn set_performance_panel(&mut self, value: bool) {
        self.session.set_performance_visible(value);
    }
    pub fn observe_frame(&mut self, update: f64, draw: f64, total: f64) -> bool {
        self.session.observe_frame(update, draw, total)
    }
    pub fn debug_status(&self) -> String {
        let p = self.session.performance();
        serde_json::json!({"paused":self.session.paused(),"recording":self.session.recording(),"playback":self.session.playback(),"frame":self.session.replay_frame(),"length":self.session.replay_length(),"hitboxes":self.session.hitboxes(),"performance":self.session.performance_visible(),"samples":p.samples,"updateMs":p.update_ms,"drawMs":p.draw_ms,"frameMs":p.frame_ms,"updateP95":p.update_p95_ms,"frameP95":p.frame_p95_ms}).to_string()
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, JsValue> {
        self.session
            .game()
            .checkpoint()
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn restore_checkpoint(&mut self, bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
        let old = self
            .session
            .restore(&bytes)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.clock.reset();
        old.map_or(Ok(Vec::new()), |r| {
            r.to_bytes().map_err(|e| JsValue::from_str(&e.to_string()))
        })
    }
    pub fn start_recording(&mut self, interval: u32) -> Result<(), JsValue> {
        self.session
            .start_recording(
                crate::game::replay::RecordingOptions {
                    checkpoint_interval: interval,
                    ..Default::default()
                },
                "Browser run",
            )
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn stop_recording(&mut self) -> Result<Vec<u8>, JsValue> {
        self.session
            .stop_recording()
            .and_then(|r| r.to_bytes().map_err(Into::into))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn load_replay(&mut self, bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
        let replay = std::sync::Arc::new(
            crate::game::replay::GameReplay::from_bytes(&bytes)
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
        );
        let old = self
            .session
            .load_replay(replay)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.clock.reset();
        old.map_or(Ok(Vec::new()), |r| {
            r.to_bytes().map_err(|e| JsValue::from_str(&e.to_string()))
        })
    }
    pub fn seek(&mut self, frame: u32) -> Result<(), JsValue> {
        self.session
            .seek(frame as usize)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.clock.reset();
        Ok(())
    }
    pub fn fast_forward(&mut self, frames: u32, flags: u32) -> Result<u32, JsValue> {
        let input = crate::game::GameInput::from_flags(0, 0, flags)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let n = self
            .session
            .fast_forward(frames as usize, input)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.clock.reset();
        Ok(n as u32)
    }
    pub fn practice(&mut self, phase: u32) -> Result<Vec<u8>, JsValue> {
        let old = self
            .session
            .practice(phase)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.clock.reset();
        old.map_or(Ok(Vec::new()), |r| {
            r.to_bytes().map_err(|e| JsValue::from_str(&e.to_string()))
        })
    }
    pub fn reload(
        &mut self,
        source: String,
        manifest: String,
        atlas: Vec<u8>,
    ) -> Result<Vec<u8>, JsValue> {
        let pack = crate::resources::ResourcePack::from_json(&manifest, atlas)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let prepared = self
            .renderer
            .as_ref()
            .ok_or_else(|| JsValue::from_str("renderer unavailable; recover first"))?
            .prepare_resources(&pack)
            .map_err(|e| JsValue::from_str(&e))?;
        let old = self
            .session
            .reload("stage.graze", &source, pack)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.renderer
            .as_mut()
            .expect("prepared renderer")
            .commit_resources(prepared);
        self.clock.reset();
        old.map_or(Ok(Vec::new()), |r| {
            r.to_bytes().map_err(|e| JsValue::from_str(&e.to_string()))
        })
    }
    pub fn inspect(&self) -> String {
        let game = self.session.game();
        let vm = game.stage().vm();
        let tasks:Vec<_>=vm.inspect_tasks().iter().map(|t|serde_json::json!({"slot":t.handle.slot,"generation":t.handle.generation,"state":format!("{:?}",t.state),"wake":t.wake_tick.to_string(),"owner":t.owner.map(|h|format!("{:?}:{}:{}",h.kind(),h.slot(),h.generation())),"frames":t.frames.iter().map(|f|serde_json::json!({"function":f.function,"pc":f.pc,"line":f.line,"column":f.column,"start":f.span.start,"end":f.span.end,"registers":f.registers.iter().map(|v|format!("{v:?}")).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect();
        let entities:Vec<_>=game.simulation().snapshots().take(256).map(|e|serde_json::json!({"kind":e.handle.kind() as u32,"slot":e.handle.slot(),"generation":e.handle.generation(),"xRaw":e.position.x.bits(),"yRaw":e.position.y.bits(),"radiusRaw":e.collider.radius().bits(),"health":e.health})).collect();
        serde_json::json!({"tick":game.hud().tick.to_string(),"hash":game.state_hash().to_string(),"file":vm.program().source_name(),"program":format!("{:016x}",vm.program().content_hash()),"tasks":tasks,"entities":entities,"totalEntities":game.simulation().enemy_count()+game.simulation().projectile_count()+1+game.advanced_hud().map_or(0,|h|h.drops as usize)}).to_string()
    }
    pub fn restart(&mut self) -> Result<(), JsValue> {
        self.clock.reset();
        self.session
            .restart()
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height);
        }
        self.clock.reset();
    }
    pub fn draw(&mut self) -> Result<bool, JsValue> {
        self.renderer
            .as_mut()
            .ok_or_else(|| JsValue::from_str("renderer unavailable; recover first"))?
            .draw_debug(
                self.session.game(),
                crate::graphics::DebugDraw {
                    hitboxes: self.session.hitboxes(),
                    paused: self.session.paused(),
                    tasks: self.session.game().stage().vm().task_count() as u32,
                    performance: self
                        .session
                        .performance_visible()
                        .then(|| self.session.performance()),
                },
            )
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
#[wasm_bindgen]
pub fn script_conformance_trace(frames: u32) -> Result<Vec<u64>, JsValue> {
    if frames > 100000 {
        return Err(JsValue::from_str("maximum 100,000 frames"));
    }
    Ok(crate::language::trace(frames))
}
#[wasm_bindgen]
pub fn advanced_conformance_trace(frames: u32) -> Result<Vec<u64>, JsValue> {
    if frames > 100000 {
        return Err(JsValue::from_str("maximum 100,000 frames"));
    }
    Ok(crate::game::showcase::trace(frames))
}
