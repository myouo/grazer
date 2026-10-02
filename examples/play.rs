#[cfg(target_arch = "wasm32")]
fn main() {}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use grazer::{
        audio::DesktopAudio,
        game::{DemoStage, FrameClock, Game, GameConfig, GameInput},
        graphics::GameRenderer,
        resources::ResourcePack,
    };
    use std::{sync::Arc, time::Instant};
    use winit::{
        application::ApplicationHandler,
        event::{ElementState, WindowEvent},
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        keyboard::{KeyCode, PhysicalKey},
        window::{Window, WindowId},
    };
    struct App {
        game: Game,
        window: Option<Arc<Window>>,
        renderer: Option<GameRenderer>,
        audio: Option<DesktopAudio>,
        clock: FrameClock,
        last: Instant,
        input: GameInput,
        keys: [bool; 4],
        active: bool,
        drawable: bool,
        paused: bool,
        frames: u64,
        limit: u64,
        autoplay: bool,
        error: Option<String>,
        samples: Vec<f64>,
    }
    impl ApplicationHandler for App {
        fn exiting(&mut self, _: &ActiveEventLoop) {
            self.renderer.take();
            self.window.take();
        }
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let result = (|| {
                let window = Arc::new(
                    event_loop
                        .create_window(
                            Window::default_attributes()
                                .with_title(
                                    "Grazer — Z shoot / X bomb / Shift focus / P pause / R restart",
                                )
                                .with_inner_size(winit::dpi::PhysicalSize::new(720, 1152)),
                        )
                        .map_err(|e| e.to_string())?,
                );
                let instance = wgpu::Instance::default();
                let size = window.inner_size();
                let surface = instance
                    .create_surface(window.clone())
                    .map_err(|e| e.to_string())?;
                let cfg = self.game.simulation().config();
                let renderer = pollster::block_on(GameRenderer::new(
                    &instance,
                    surface,
                    size.width,
                    size.height,
                    cfg.projectile_capacity as usize + cfg.enemy_capacity as usize + 1024,
                    self.game.resources(),
                ))?;
                println!(
                    "adapter={} viewport={}x{} resources={:016x}",
                    renderer.adapter(),
                    size.width,
                    size.height,
                    self.game.resources().content_hash()
                );
                self.renderer = Some(renderer);
                self.window = Some(window);
                self.last = Instant::now();
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                self.error = Some(error);
                event_loop.exit();
            }
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    self.drawable = size.width > 0 && size.height > 0;
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(size.width, size.height);
                    }
                    self.clock.reset();
                    self.last = Instant::now();
                }
                WindowEvent::Focused(active) => {
                    self.active = active;
                    self.keys = [false; 4];
                    self.input = GameInput::default();
                    self.clock.reset();
                    self.last = Instant::now();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        let pressed = event.state == ElementState::Pressed;
                        match code {
                            KeyCode::ArrowLeft | KeyCode::KeyA => self.keys[0] = pressed,
                            KeyCode::ArrowRight | KeyCode::KeyD => self.keys[1] = pressed,
                            KeyCode::ArrowUp | KeyCode::KeyW => self.keys[2] = pressed,
                            KeyCode::ArrowDown | KeyCode::KeyS => self.keys[3] = pressed,
                            KeyCode::KeyZ | KeyCode::Space => self.input.fire = pressed,
                            KeyCode::KeyX => self.input.bomb = pressed,
                            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.focus = pressed,
                            KeyCode::KeyR | KeyCode::Enter => self.input.restart = pressed,
                            KeyCode::KeyP if pressed && !event.repeat => {
                                self.paused = !self.paused;
                                self.clock.reset();
                                self.last = Instant::now();
                                if let Some(audio) = &self.audio {
                                    let result = if self.paused {
                                        audio.pause()
                                    } else {
                                        audio.resume()
                                    };
                                    if let Err(error) = result {
                                        eprintln!("Audio pause/resume: {error}");
                                    }
                                }
                            }
                            KeyCode::Escape => event_loop.exit(),
                            _ => {}
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    let now = Instant::now();
                    let elapsed = now.duration_since(self.last);
                    self.last = now;
                    if !self.drawable {
                        return;
                    }
                    let start = Instant::now();
                    let count = self.clock.advance(elapsed, self.active && !self.paused);
                    for _ in 0..count {
                        let mut input = self.input;
                        input.x = i32::from(self.keys[1]) - i32::from(self.keys[0]);
                        input.y = i32::from(self.keys[3]) - i32::from(self.keys[2]);
                        if self.autoplay {
                            input.fire = true;
                        }
                        if let Err(error) = self.game.step(input) {
                            self.error = Some(error.to_string());
                            event_loop.exit();
                            return;
                        }
                        if let Some(audio) = &mut self.audio {
                            audio.events(self.game.audio_events(), self.game.resources());
                        }
                    }
                    if let Some(renderer) = &mut self.renderer {
                        match renderer.draw(&self.game) {
                            Ok(true) => self.frames += 1,
                            Ok(false) => return,
                            Err(error) => {
                                self.error = Some(error);
                                event_loop.exit();
                                return;
                            }
                        }
                    }
                    if self.frames > 120 && self.samples.len() < 1200 {
                        self.samples.push(start.elapsed().as_secs_f64() * 1000.0);
                    }
                    if self.frames.is_multiple_of(30)
                        && let Some(window) = &self.window
                    {
                        let hud = self.game.hud();
                        window.set_title(&format!(
                            "Grazer | HP {}  BOMBS {}  SCORE {} | {}",
                            hud.health,
                            hud.bombs,
                            hud.score,
                            if self.paused {
                                "PAUSED"
                            } else {
                                "Z shoot / X bomb / R restart"
                            }
                        ));
                    }
                    if self.limit > 0 && self.frames >= self.limit {
                        event_loop.exit();
                    }
                }
                _ => {}
            }
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if !self.active || !self.drawable {
                event_loop.set_control_flow(ControlFlow::Wait);
            } else {
                event_loop.set_control_flow(ControlFlow::Poll);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let mut limit = 0;
        let mut autoplay = false;
        let mut project = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => limit = args.next().ok_or("--frames needs a count")?.parse()?,
                "--autoplay" => autoplay = true,
                "--project" => project = Some(args.next().ok_or("--project needs a path")?),
                _ => return Err(format!("unknown option {arg}").into()),
            }
        }
        let pack = if let Some(path) = project {
            ResourcePack::load(path)?
        } else {
            ResourcePack::from_json(
                include_str!("../assets/demo/project.json"),
                include_bytes!("../assets/demo/sprites.rgba").to_vec(),
            )?
        };
        let game = Game::with_stage(GameConfig::default(), 42, pack, DemoStage::default())?;
        let audio = match DesktopAudio::new() {
            Ok(audio) => {
                println!("audio=running");
                Some(audio)
            }
            Err(error) => {
                eprintln!("Audio unavailable: {error}");
                None
            }
        };
        let mut app = App {
            game,
            window: None,
            renderer: None,
            audio,
            clock: FrameClock::default(),
            last: Instant::now(),
            input: GameInput::default(),
            keys: [false; 4],
            active: true,
            drawable: true,
            paused: false,
            frames: 0,
            limit,
            autoplay,
            error: None,
            samples: Vec::with_capacity(1200),
        };
        EventLoop::new()?.run_app(&mut app)?;
        if let Some(error) = app.error {
            return Err(error.into());
        }
        println!(
            "frames={} tick={} phase={:?} hash={:016x}",
            app.frames,
            app.game.hud().tick,
            app.game.phase(),
            app.game.state_hash()
        );
        if let Some(audio) = &app.audio {
            println!(
                "audio scheduled={} started={} dropped={} peak={:.4} failed={}",
                audio.scheduled,
                audio.started(),
                audio.dropped,
                audio.peak(),
                audio.failed()
            );
        }
        app.samples.sort_by(f64::total_cmp);
        if !app.samples.is_empty() {
            println!(
                "CPU update+submit samples={} warmup=120 p95_ms={:.3}",
                app.samples.len(),
                app.samples[(app.samples.len() * 95).div_ceil(100) - 1]
            );
        }
        Ok(())
    }
}
