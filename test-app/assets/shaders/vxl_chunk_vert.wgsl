#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
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
) -> VertexOutput {

    let quad = quads[vertex / 4u];
    var position = extract_position(quad, vertex % 4u);
    let face = extract_face(quad);
    let model = mesh_functions::get_model_matrix(instance_index);

    var out: VertexOutput;
    out.quad_idx = chunk_quad_index;

    out.uv = project_to_2d(position, axis_from_face(face)) - quad.min;

    out.local_position = position;
    out.world_position = vec4f(position + (chunk_position * 16.0), 1.0);
    out.position = position_world_to_clip(out.world_position.xyz);

    out.instance_index = instance_index;

    return out;
}

