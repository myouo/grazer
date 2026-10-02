#[cfg(target_arch = "wasm32")]
fn main() {}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use grazer::{Simulation, graphics::Renderer, performance};
    use std::{sync::Arc, time::Instant};
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        window::{Window, WindowId},
    };
    #[cfg(target_os = "linux")]
    fn cpu_time() -> f64 {
        #[repr(C)]
        struct Timespec {
            sec: i64,
            nsec: i64,
        }
        unsafe extern "C" {
            fn clock_gettime(clock: i32, out: *mut Timespec) -> i32;
        }
        let mut t = Timespec { sec: 0, nsec: 0 };
        // SAFETY: Linux clock_gettime writes a correctly aligned timespec; clock
        // 3 is CLOCK_THREAD_CPUTIME_ID. This is host profiling, not simulation.
        let result = unsafe { clock_gettime(3, &mut t) };
        if result == 0 {
            t.sec as f64 * 1000.0 + t.nsec as f64 / 1_000_000.0
        } else {
            0.0
        }
    }
    #[cfg(not(target_os = "linux"))]
    fn cpu_time() -> f64 {
        0.0
    }
    struct App {
        world: Simulation,
        count: u32,
        samples: usize,
        frames: usize,
        window: Option<Arc<Window>>,
        renderer: Option<Renderer>,
        update: Vec<f64>,
        cpu: Vec<f64>,
        recycle: Vec<f64>,
        draw: Vec<f64>,
        total: Vec<f64>,
        error: Option<String>,
        hits: u64,
        minimum: usize,
    }
    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let result = (|| {
                let window = Arc::new(
                    event_loop.create_window(
                        Window::default_attributes()
                            .with_title("Grazer M6 full-frame benchmark")
                            .with_inner_size(winit::dpi::PhysicalSize::new(1920, 1080)),
                    )?,
                );
                let size = window.inner_size();
                let instance = wgpu::Instance::default();
                let surface = instance.create_surface(window.clone())?;
                let mut renderer = pollster::block_on(Renderer::new(
                    &instance,
                    surface,
                    size.width,
                    size.height,
                    self.count as usize + 1,
                ))
                .map_err(std::io::Error::other)?;
                renderer.set_unlimited_present();
                println!(
                    "adapter={} viewport={}x{} vsync=false gpu_completion=true",
                    renderer.adapter(),
                    size.width,
                    size.height
                );
                self.window = Some(window);
                self.renderer = Some(renderer);
                Ok::<(), Box<dyn std::error::Error>>(())
            })();
            if let Err(e) = result {
                self.error = Some(e.to_string());
                event_loop.exit();
            }
        }
        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _id: WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::RedrawRequested => {
                    let result = (|| {
                        let start = Instant::now();
                        let cpu_start = cpu_time();
                        assert_eq!(self.world.projectile_count(), self.count as usize);
                        self.world
                            .step_with_input(performance::input(self.world.tick()))?;
                        let update = start.elapsed().as_secs_f64() * 1000.0;
                        let cpu = cpu_time() - cpu_start;
                        self.minimum = self.minimum.min(self.world.projectile_count());
                        self.hits += self
                            .world
                            .events()
                            .iter()
                            .filter(|e| matches!(e, grazer::Event::Hit { .. }))
                            .count() as u64;
                        let recycle_start = Instant::now();
                        performance::replenish(&mut self.world, self.count)?;
                        let recycle = recycle_start.elapsed().as_secs_f64() * 1000.0;
                        let draw_start = Instant::now();
                        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
                        if !renderer.draw_simulation(&self.world)? {
                            return Ok::<(), Box<dyn std::error::Error>>(());
                        }
                        renderer.wait_for_gpu()?;
                        let draw = draw_start.elapsed().as_secs_f64() * 1000.0;
                        let total = start.elapsed().as_secs_f64() * 1000.0;
                        if self.frames >= 120 {
                            self.update.push(update);
                            self.cpu.push(cpu);
                            self.recycle.push(recycle);
                            self.draw.push(draw);
                            self.total.push(total);
                        }
                        self.frames += 1;
                        if self.total.len() == self.samples {
                            event_loop.exit();
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        self.error = Some(e.to_string());
                        event_loop.exit();
                    }
                }
                _ => {}
            }
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            event_loop.set_control_flow(ControlFlow::Poll);
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        fn exiting(&mut self, _: &ActiveEventLoop) {
            self.renderer.take();
            self.window.take();
        }
    }
    fn p95(values: &mut [f64]) -> f64 {
        values.sort_unstable_by(f64::total_cmp);
        values[(values.len() * 95).div_ceil(100) - 1]
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let args: Vec<_> = std::env::args().collect();
        let count = args
            .get(1)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(100000);
        let samples = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1200);
        if samples == 0 || count == 0 {
            return Err("positive count/sample count required".into());
        }
        let mut app = App {
            world: performance::scene(count, 42)?,
            count,
            samples,
            frames: 0,
            window: None,
            renderer: None,
            update: Vec::with_capacity(samples),
            cpu: Vec::with_capacity(samples),
            recycle: Vec::with_capacity(samples),
            draw: Vec::with_capacity(samples),
            total: Vec::with_capacity(samples),
            error: None,
            hits: 0,
            minimum: count as usize,
        };
        EventLoop::new()?.run_app(&mut app)?;
        if let Some(error) = app.error {
            return Err(error.into());
        }
        if app.total.len() != samples {
            return Err("benchmark stopped before completing samples".into());
        }
        println!("simulation thread CPU p95_ms={:.6}", p95(&mut app.cpu));
        println!(
            "{{\"workload\":\"M6-full-frame\",\"count\":{count},\"width\":1920,\"height\":1080,\"warmup\":120,\"samples\":{samples},\"simulation_p95_ms\":{:.6},\"replenish_p95_ms\":{:.6},\"draw_gpu_complete_p95_ms\":{:.6},\"frame_p95_ms\":{:.6},\"minimum_after_step\":{},\"hits\":{},\"grazes\":{},\"hash\":\"{:016x}\"}}",
            p95(&mut app.update),
            p95(&mut app.recycle),
            p95(&mut app.draw),
            p95(&mut app.total),
            app.minimum,
            app.hits,
            app.world.player().grazes,
            app.world.state_hash()
        );
        Ok(())
    }
}
