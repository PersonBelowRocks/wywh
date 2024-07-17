#import vxl::chunk_io::{MultidrawVertex, PrepassOutput}
#import vxl::multidraw_chunk_bindings::quads

#import bevy_pbr::view_transformations::position_world_to_clip

#import vxl::utils::{
    extract_face,
    extract_position,
    project_to_2d,
    axis_from_face,
}

@vertex
fn chunk_vertex(in: MultidrawVertex) -> PrepassOutput {
    let quad_idx = (in.vertex_index / 4u) + in.base_quad;
    let quad = quads[quad_idx];

    let position = extract_position(quad, in.vertex_index % 4u);
    let face = extract_face(quad);

    var out: PrepassOutput;
    out.quad_idx = quad_idx;
    out.instance_index = in.instance_index;

    out.uv = project_to_2d(position, axis_from_face(face)) - quad.min;

    out.local_position = position;
    out.world_position = vec4f(in.chunk_position + position, 1.0);
    out.position = position_world_to_clip(out.world_position.xyz);
    
#ifdef DEPTH_CLAMP_ORTHO
    out.clip_position_unclamped = out.position;
    out.position.z = min(out.position.z, 1.0);
#endif

    return out;
}