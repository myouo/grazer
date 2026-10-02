#[cfg(target_arch = "wasm32")]
fn main() {}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use grazer::{Input, Runtime, graphics::Renderer};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use winit::{
        application::ApplicationHandler,
        event::{ElementState, WindowEvent},
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        keyboard::{KeyCode, PhysicalKey},
        window::{Window, WindowId},
    };
    struct App {
        runtime: Runtime,
        window: Option<Arc<Window>>,
        renderer: Option<Renderer>,
        last: Instant,
        accumulator: Duration,
        keys: [bool; 4],
        focused: bool,
        drawable: bool,
        frames: usize,
        limit: usize,
        samples: Vec<f64>,
        error: Option<String>,
    }
    impl ApplicationHandler for App {
        fn exiting(&mut self, _: &ActiveEventLoop) {
            // Surfaces and windows must be released while the display connection
            // owned by the event loop is still alive (notably on Wayland/EGL).
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
                                .with_title("Grazer M0 — arrow keys to move")
                                .with_inner_size(winit::dpi::PhysicalSize::new(1920, 1080)),
                        )
                        .map_err(|e| e.to_string())?,
                );
                let size = window.inner_size();
                let instance = wgpu::Instance::default();
                let surface = instance
                    .create_surface(window.clone())
                    .map_err(|e| e.to_string())?;
                let renderer = pollster::block_on(Renderer::new(
                    &instance,
                    surface,
                    size.width,
                    size.height,
                    self.runtime.bullet_count() + 1,
                ))?;
                println!(
                    "adapter={} count={} viewport={}x{}",
                    renderer.adapter(),
                    self.runtime.bullet_count(),
                    size.width,
                    size.height
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
                    self.drawable = size.width != 0 && size.height != 0;
                    if let Some(r) = &mut self.renderer {
                        r.resize(size.width, size.height);
                    }
                    self.last = Instant::now();
                    self.accumulator = Duration::ZERO;
                }
                WindowEvent::Focused(focused) => {
                    self.focused = focused;
                    self.keys = [false; 4];
                    self.last = Instant::now();
                    self.accumulator = Duration::ZERO;
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        let idx = match code {
                            KeyCode::ArrowLeft => Some(0),
                            KeyCode::ArrowRight => Some(1),
                            KeyCode::ArrowUp => Some(2),
                            KeyCode::ArrowDown => Some(3),
                            _ => None,
                        };
                        if let Some(i) = idx {
                            self.keys[i] = event.state == ElementState::Pressed;
                        }
                        if code == KeyCode::Escape {
                            event_loop.exit();
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    let now = Instant::now();
                    let elapsed = now.duration_since(self.last);
                    self.last = now;
                    if !self.focused || !self.drawable {
                        return;
                    }
                    let start = Instant::now();
                    self.accumulator += elapsed;
                    let step = Duration::from_secs_f64(1.0 / 60.0);
                    let input = Input {
                        x: i32::from(self.keys[1]) - i32::from(self.keys[0]),
                        y: i32::from(self.keys[3]) - i32::from(self.keys[2]),
                    };
                    self.runtime
                        .set_input(input)
                        .expect("bounded keyboard input");
                    for _ in 0..8 {
                        if self.accumulator < step {
                            break;
                        }
                        if let Err(error) = self.runtime.step() {
                            self.error = Some(error.to_string());
                            event_loop.exit();
                            return;
                        }
                        self.accumulator -= step;
                    }
                    if let Some(renderer) = &mut self.renderer {
                        match renderer.draw(&self.runtime) {
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
                    if self.limit > 0 && self.frames >= self.limit {
                        event_loop.exit();
                    }
                }
                _ => {}
            }
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if !self.focused || !self.drawable {
                event_loop.set_control_flow(ControlFlow::Wait);
            } else {
                event_loop.set_control_flow(ControlFlow::Poll);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let args: Vec<_> = std::env::args().collect();
        let count = args
            .get(1)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(100_000);
        let limit = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(0);
        let mut app = App {
            runtime: grazer::demo::scene(count, 42)?,
            window: None,
            renderer: None,
            last: Instant::now(),
            accumulator: Duration::ZERO,
            keys: [false; 4],
            focused: true,
            drawable: true,
            frames: 0,
            limit,
            samples: Vec::new(),
            error: None,
        };
        EventLoop::new()?.run_app(&mut app)?;
        if let Some(error) = app.error {
            return Err(error.into());
        }
        app.samples.sort_by(f64::total_cmp);
        let n = app.samples.len();
        println!(
            "frames={} ticks={} hash={:016x}",
            app.frames,
            app.runtime.tick(),
            app.runtime.state_hash()
        );
        if n > 0 {
            println!(
                "CPU update+submit (includes surface wait, not GPU duration) samples={n} warmup=120 p95_ms={:.3}",
                app.samples[(n * 95).div_ceil(100) - 1]
            );
        }
        Ok(())
    }
}
