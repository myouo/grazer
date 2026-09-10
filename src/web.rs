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
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
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
