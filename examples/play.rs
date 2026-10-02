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
        game::{
            FrameClock, Game, GameConfig, GameInput,
            debug::DebugSession,
            replay::{GameReplay, RecordingOptions},
        },
        graphics::{DebugDraw, GameRenderer},
        language::{Program, ScriptStage, VmLimits},
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
        session: DebugSession<ScriptStage>,
        script_path: String,
        project_path: String,
        record_path: Option<String>,
        watch: bool,
        last_watch: Instant,
        checkpoint: Option<Vec<u8>>,
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
    impl App {
        fn archive(&mut self, replay: Option<GameReplay>) {
            if let Some(replay) = replay {
                let path = self
                    .record_path
                    .clone()
                    .unwrap_or_else(|| "target/desktop-replay.grz".into());
                match replay.to_bytes().and_then(|bytes| {
                    std::fs::write(&path, bytes).map_err(|_| {
                        grazer::game::replay::ReplayError::Data("cannot save recording")
                    })
                }) {
                    Ok(()) => println!("saved replay={path} frames={}", replay.frames().len()),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        }
        fn reload_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            let pack = ResourcePack::load(&self.project_path)?;
            let stage = if self.script_path.ends_with(".gzb") {
                ScriptStage::new(
                    Arc::new(Program::from_bytes(&std::fs::read(&self.script_path)?)?),
                    self.session.game().stage().vm().limits(),
                    self.session.game().seed(),
                )?
            } else {
                ScriptStage::compile(
                    &self.script_path,
                    &std::fs::read_to_string(&self.script_path)?,
                    self.session.game().stage().vm().limits(),
                    self.session.game().seed(),
                )?
            };
            let prepared = self
                .renderer
                .as_ref()
                .ok_or("renderer not ready")?
                .prepare_resources(&pack)?;
            let old = self.session.reload_stage(stage, pack)?;
            self.renderer
                .as_mut()
                .expect("renderer")
                .commit_resources(prepared);
            self.archive(old);
            self.paused = true;
            self.clock.reset();
            self.last = Instant::now();
            self.error = None;
            println!(
                "reloaded source={} content={:016x}",
                self.script_path,
                self.session.game().stage().vm().program().content_hash()
            );
            Ok(())
        }
        fn reload_if_changed(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            let program = if self.script_path.ends_with(".gzb") {
                Program::from_bytes(&std::fs::read(&self.script_path)?)?
            } else {
                Program::compile(
                    &self.script_path,
                    &std::fs::read_to_string(&self.script_path)?,
                )?
            };
            let pack = ResourcePack::load(&self.project_path)?;
            if program.content_hash() != self.session.game().stage().vm().program().content_hash()
                || pack.content_hash() != self.session.game().resources().content_hash()
            {
                self.reload_files()?;
            }
            Ok(())
        }
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
                let renderer = pollster::block_on(GameRenderer::new(
                    &instance,
                    surface,
                    size.width,
                    size.height,
                    self.session.game().presentation_capacity(),
                    self.session.game().resources(),
                ))?;
                println!(
                    "adapter={} viewport={}x{} resources={:016x}",
                    renderer.adapter(),
                    size.width,
                    size.height,
                    self.session.game().resources().content_hash()
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
                            KeyCode::KeyR | KeyCode::Enter => {
                                if pressed && self.error.is_some() {
                                    if let Err(error) = self.session.restart() {
                                        self.error = Some(error.to_string());
                                    } else {
                                        self.error = None;
                                        self.paused = false;
                                        self.session.set_paused(false);
                                        self.clock.reset();
                                        self.last = Instant::now();
                                    }
                                } else {
                                    self.input.restart = pressed;
                                }
                            }
                            KeyCode::KeyP if pressed && !event.repeat => {
                                self.paused = !self.paused;
                                self.session.set_paused(self.paused);
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
                            KeyCode::KeyN if pressed && !event.repeat && self.paused => {
                                if let Err(e) = self.session.single_step(GameInput {
                                    fire: self.autoplay,
                                    ..GameInput::default()
                                }) {
                                    self.error = Some(e.to_string());
                                }
                            }
                            KeyCode::F1 if pressed && !event.repeat => {
                                self.session.set_hitboxes(!self.session.hitboxes())
                            }
                            KeyCode::F2 if pressed && !event.repeat => self
                                .session
                                .set_performance_visible(!self.session.performance_visible()),
                            KeyCode::F5 if pressed && !event.repeat => {
                                if let Err(e) = self.reload_files() {
                                    self.error = Some(e.to_string());
                                    self.paused = true;
                                    self.session.set_paused(true);
                                }
                            }
                            KeyCode::F6 if pressed && !event.repeat => {
                                match self.session.game().checkpoint() {
                                    Ok(bytes) => self.checkpoint = Some(bytes),
                                    Err(e) => self.error = Some(e.to_string()),
                                }
                            }
                            KeyCode::F7 if pressed && !event.repeat => {
                                if let Some(bytes) = self.checkpoint.clone() {
                                    match self.session.restore(&bytes) {
                                        Ok(old) => {
                                            self.archive(old);
                                            self.paused = true;
                                            self.clock.reset();
                                        }
                                        Err(e) => self.error = Some(e.to_string()),
                                    }
                                }
                            }
                            KeyCode::F8 if pressed && !event.repeat => {
                                if let Err(e) = self.session.fast_forward(
                                    600,
                                    GameInput {
                                        fire: true,
                                        ..GameInput::default()
                                    },
                                ) {
                                    self.error = Some(e.to_string());
                                }
                                self.clock.reset();
                            }
                            KeyCode::F9 if pressed && !event.repeat => {
                                if self.session.recording() {
                                    match self.session.stop_recording() {
                                        Ok(replay) => self.archive(Some(replay)),
                                        Err(e) => self.error = Some(e.to_string()),
                                    }
                                } else if let Err(e) = self
                                    .session
                                    .start_recording(RecordingOptions::default(), "Desktop run")
                                {
                                    self.error = Some(e.to_string());
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
                    if self.watch && self.last_watch.elapsed().as_secs_f64() >= 1.0 {
                        self.last_watch = Instant::now();
                        if let Err(e) = self.reload_if_changed() {
                            self.error = Some(e.to_string());
                            self.paused = true;
                            self.session.set_paused(true);
                        }
                    }
                    let count = self.clock.advance(elapsed, self.active && !self.paused);
                    for _ in 0..count {
                        let mut input = self.input;
                        input.x = i32::from(self.keys[1]) - i32::from(self.keys[0]);
                        input.y = i32::from(self.keys[3]) - i32::from(self.keys[2]);
                        if self.autoplay {
                            input.fire = true;
                        }
                        if input.restart {
                            if let Err(e) = self.session.restart() {
                                self.error = Some(e.to_string());
                            }
                            self.input.restart = false;
                            self.clock.reset();
                            continue;
                        }
                        let before_tick = self.session.game().hud().tick;
                        let result = self.session.advance(input);
                        if matches!(result, Ok(false)) {
                            self.paused = self.session.paused();
                            break;
                        }
                        if let Err(error) = result {
                            self.error = Some(error.to_string());
                            eprintln!("Stage paused: {error}");
                            self.paused = true;
                            self.session.set_paused(true);
                            self.clock.reset();
                            break;
                        }
                        if self.session.game().hud().tick == before_tick.saturating_add(1)
                            && let Some(audio) = &mut self.audio
                        {
                            audio.events(
                                self.session.game().audio_events(),
                                self.session.game().resources(),
                            );
                        }
                    }
                    let draw_start = Instant::now();
                    if let Some(renderer) = &mut self.renderer {
                        match renderer.draw_debug(
                            self.session.game(),
                            DebugDraw {
                                hitboxes: self.session.hitboxes(),
                                paused: self.paused,
                                tasks: self.session.game().stage().vm().task_count() as u32,
                                performance: self
                                    .session
                                    .performance_visible()
                                    .then(|| self.session.performance()),
                            },
                        ) {
                            Ok(true) => self.frames += 1,
                            Ok(false) => return,
                            Err(error) => {
                                self.error = Some(error);
                                event_loop.exit();
                                return;
                            }
                        }
                    }
                    self.session.observe_frame(
                        draw_start.duration_since(start).as_secs_f64() * 1000.0,
                        draw_start.elapsed().as_secs_f64() * 1000.0,
                        start.elapsed().as_secs_f64() * 1000.0,
                    );
                    self.paused = self.session.paused();
                    if self.frames > 120 && self.samples.len() < 1200 {
                        self.samples.push(start.elapsed().as_secs_f64() * 1000.0);
                    }
                    if self.frames.is_multiple_of(30)
                        && let Some(window) = &self.window
                    {
                        let hud = self.session.game().hud();
                        window.set_title(&format!(
                            "Grazer | HP {}  BOMBS {}  SCORE {} | {}",
                            hud.health,
                            hud.bombs,
                            hud.score,
                            if let Some(error) = &self.error {
                                error.as_str()
                            } else if self.paused {
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
        let mut script_path = None;
        let mut difficulty = grazer::advanced::Difficulty::Normal;
        let mut health = 0;
        let mut practice = 0;
        let mut record_path = None;
        let mut replay_path = None;
        let mut checkpoint_path = None;
        let mut start_paused = false;
        let mut hitboxes = false;
        let mut performance = false;
        let mut watch = false;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => limit = args.next().ok_or("--frames needs a count")?.parse()?,
                "--autoplay" => autoplay = true,
                "--practice" => {
                    practice = args
                        .next()
                        .ok_or("--practice needs Boss phase 1..3")?
                        .parse()?
                }
                "--record" => record_path = Some(args.next().ok_or("--record needs output path")?),
                "--replay" => replay_path = Some(args.next().ok_or("--replay needs input path")?),
                "--checkpoint" => {
                    checkpoint_path = Some(args.next().ok_or("--checkpoint needs input path")?)
                }
                "--paused" => start_paused = true,
                "--hitboxes" => hitboxes = true,
                "--performance" => performance = true,
                "--watch" => watch = true,
                "--health" => health = args.next().ok_or("--health needs a count")?.parse()?,
                "--difficulty" => {
                    difficulty = match args.next().as_deref() {
                        Some("easy") => grazer::advanced::Difficulty::Easy,
                        Some("normal") => grazer::advanced::Difficulty::Normal,
                        Some("hard") => grazer::advanced::Difficulty::Hard,
                        _ => return Err("--difficulty must be easy, normal or hard".into()),
                    }
                }
                "--project" => project = Some(args.next().ok_or("--project needs a path")?),
                "--script" => {
                    script_path = Some(args.next().ok_or("--script needs a source/bytecode path")?)
                }
                _ => return Err(format!("unknown option {arg}").into()),
            }
        }
        let pack = if let Some(path) = &project {
            ResourcePack::load(path)?
        } else {
            ResourcePack::from_json(
                include_str!("../assets/demo/project.json"),
                include_bytes!("../assets/demo/sprites.rgba").to_vec(),
            )?
        };
        let stage = if let Some(path) = &script_path {
            if path.ends_with(".gzb") {
                ScriptStage::new(
                    Arc::new(Program::from_bytes(&std::fs::read(path)?)?),
                    VmLimits::default(),
                    42,
                )?
            } else {
                ScriptStage::compile(
                    path,
                    &std::fs::read_to_string(path)?,
                    VmLimits::default(),
                    42,
                )?
            }
        } else {
            ScriptStage::showcase(42)?
        };
        let mut config = GameConfig::default();
        if health > 0 {
            config.simulation.player.health = health;
        }
        let game = if stage.vm().program().uses_advanced() {
            Game::with_advanced_stage(
                config,
                42,
                pack,
                stage,
                grazer::advanced::AdvancedConfig {
                    difficulty,
                    ..grazer::advanced::AdvancedConfig::default()
                },
            )?
        } else {
            Game::with_stage(config, 42, pack, stage)?
        };
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
        let mut session = DebugSession::new(game)?;
        if practice > 0 {
            session.practice(practice)?;
        }
        if let Some(path) = replay_path {
            session.load_replay(Arc::new(GameReplay::from_bytes(&std::fs::read(path)?)?))?;
        }
        if let Some(path) = checkpoint_path {
            session.restore(&std::fs::read(path)?)?;
        }
        session.set_paused(start_paused);
        session.set_hitboxes(hitboxes);
        session.set_performance_visible(performance);
        if record_path.is_some() {
            session.start_recording(RecordingOptions::default(), "Desktop run")?;
        }
        let mut app = App {
            session,
            script_path: script_path
                .unwrap_or_else(|| "assets/demo/advanced_showcase.graze".into()),
            project_path: project.unwrap_or_else(|| "assets/demo/project.json".into()),
            record_path,
            watch,
            last_watch: Instant::now(),
            checkpoint: None,
            window: None,
            renderer: None,
            audio,
            clock: FrameClock::default(),
            last: Instant::now(),
            input: GameInput::default(),
            keys: [false; 4],
            active: true,
            drawable: true,
            paused: start_paused,
            frames: 0,
            limit,
            autoplay,
            error: None,
            samples: Vec::with_capacity(1200),
        };
        EventLoop::new()?.run_app(&mut app)?;
        if app.session.recording() {
            let replay = app.session.stop_recording()?;
            let path = app
                .record_path
                .clone()
                .unwrap_or_else(|| "target/desktop-replay.grz".into());
            std::fs::write(&path, replay.to_bytes()?)?;
            println!("recorded replay={path} frames={}", replay.frames().len());
        }
        if let Some(error) = app.error {
            return Err(error.into());
        }
        println!(
            "frames={} tick={} phase={:?} hash={:016x}",
            app.frames,
            app.session.game().hud().tick,
            app.session.game().phase(),
            app.session.game().state_hash()
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
