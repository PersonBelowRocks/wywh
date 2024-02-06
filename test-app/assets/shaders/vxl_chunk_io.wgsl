struct VertexOutput {
    @location(0) position: vec4<f32>,
    @location(1) world_position: vec4<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) world_tangent: vec4<f32>,
    @location(4) uv: vec2<f32>,
    @location(5) @interpolate(flat) texture: u32,
    @location(6) @interpolate(flat) bitfields: u32,
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(7) @interpolate(flat) instance_index: u32
#endif
}