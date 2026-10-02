@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;
struct VertexOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32>, @location(2) local: vec2<f32>, @location(3) @interpolate(flat) geometry: vec4<f32> };
@vertex fn vs_main(@builtin(vertex_index) vertex: u32, @location(0) center: vec2<f32>,
    @location(1) horizontal: vec2<f32>, @location(2) vertical: vec2<f32>, @location(3) uv_origin: vec2<f32>, @location(4) uv_size: vec2<f32>,
    @location(5) color: vec4<f32>, @location(6) geometry: vec4<f32>) -> VertexOut {
    var corners = array<vec2<f32>,6>(vec2(-1.0,-1.0),vec2(1.0,-1.0),vec2(-1.0,1.0),vec2(-1.0,1.0),vec2(1.0,-1.0),vec2(1.0,1.0));
    let local = corners[vertex];
    var out: VertexOut; out.position = vec4(center + local.x * horizontal + local.y * vertical,0.0,1.0);
    out.uv = uv_origin + (local+vec2(1.0))*0.5*uv_size; out.color=color; out.local=local; out.geometry=geometry; return out;
}
@fragment fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if (in.geometry.x > 0.5) {
        let half_length=in.geometry.y; let radius=in.geometry.z;
        let q=vec2(max(abs(in.local.x)*(half_length+radius)-half_length,0.0),in.local.y*radius);
        let distance=length(q);
        if (distance>radius || distance<max(radius-in.geometry.w,0.0)) {discard;}
        return in.color;
    }
    return textureSampleLevel(atlas,atlas_sampler,in.uv,0.0) * in.color;
}
