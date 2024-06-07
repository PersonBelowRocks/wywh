#define_import_path vxl::chunk_io

struct MultidrawVertex {
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
    @location(0) chunk_position: vec3<f32>,
    @location(1) base_quad: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) local_position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) quad_idx: u32,
    @location(4) @interpolate(flat) instance_index: u32,
}

struct PrepassOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) local_position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) quad_idx: u32,
    @location(4) @interpolate(flat) instance_index: u32,
#ifdef MOTION_VECTOR_PREPASS
    @location(5) previous_world_position: vec4<f32>,
#endif
#ifdef DEPTH_CLAMP_ORTHO
    @location(6) clip_position_unclamped: vec4<f32>,
#endif
}