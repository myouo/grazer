@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;
struct VertexOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };
@vertex fn vs_main(@builtin(vertex_index) vertex: u32, @location(0) center: vec2<f32>,
    @location(1) size: vec2<f32>, @location(2) uv_origin: vec2<f32>, @location(3) uv_size: vec2<f32>,
    @location(4) color: vec4<f32>) -> VertexOut {
    var corners = array<vec2<f32>,6>(vec2(-1.0,-1.0),vec2(1.0,-1.0),vec2(-1.0,1.0),vec2(-1.0,1.0),vec2(1.0,-1.0),vec2(1.0,1.0));
    let local = corners[vertex];
    var out: VertexOut; out.position = vec4(center + vec2(local.x,-local.y) * size,0.0,1.0);
    out.uv = uv_origin + (local+vec2(1.0))*0.5*uv_size; out.color=color; return out;
}
@fragment fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(atlas,atlas_sampler,in.uv) * in.color;
}
