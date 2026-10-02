struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) vertex: u32,
    @location(0) center: vec2<f32>, @location(1) radius: vec2<f32>,
    @location(2) color: vec4<f32>) -> VertexOut {
    var corners = array<vec2<f32>, 6>(vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0),
        vec2(-1.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
    var out: VertexOut;
    out.position = vec4(center + corners[vertex] * radius, 0.0, 1.0);
    out.local = corners[vertex]; out.color = color;
    return out;
}
@fragment fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if dot(in.local, in.local) > 1.0 { discard; }
    return in.color;
}
