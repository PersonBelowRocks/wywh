#import "shaders/vxl_chunk_io.wgsl"::PrepassOutput
#import "shaders/chunk_bindings.wgsl"::quads
#import "shaders/chunk_bindings.wgsl"::chunk_position
#import "shaders/utils.wgsl"::extract_normal
#import "shaders/utils.wgsl"::extract_position
#import "shaders/utils.wgsl"::project_to_2d
#import "shaders/utils.wgsl"::axis_from_face
#import "shaders/utils.wgsl"::extract_face
#import bevy_pbr::{
    view_transformations::position_world_to_clip
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance_index: u32,
    // @location(1) vertex_position: vec3<f32>,
) -> PrepassOutput {

    let quad = quads[vertex / 4u];
    var position = extract_position(quad, vertex % 4u);
    // var position = vertex_position;
    let face = extract_face(quad);

    var out: PrepassOutput;
    out.quad_idx = vertex / 4u;

    out.uv = project_to_2d(position, axis_from_face(face)) - quad.min;

    out.world_position = vec4f(position + (chunk_position * 16.0), 1.0);

    out.position = position_world_to_clip(out.world_position.xyz);
    out.local_position = position;
    
#ifdef DEPTH_CLAMP_ORTHO
    out.clip_position_unclamped = out.position;
    out.position.z = min(out.position.z, 1.0);
#endif // DEPTH_CLAMP_ORTHO

    out.instance_index = instance_index;
    
    return out;
}
