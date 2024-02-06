#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import "shaders/chunk_bindings.wgsl"::quads
#import "shaders/utils.wgsl"::extract_normal
#import "shaders/utils.wgsl"::extract_position
#import "shaders/utils.wgsl"::project_to_2d
#import "shaders/utils.wgsl"::axis_from_face
#import "shaders/utils.wgsl"::extract_face

#import bevy_render::instance_index::get_instance_index
#import bevy_pbr::{
    mesh_functions, 
    view_transformations::position_world_to_clip
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance_index: u32,
    @location(0) chunk_quad_index: u32,
) -> VertexOutput {

    let quad = quads[chunk_quad_index];
    let position = extract_position(quad, vertex % 4);
    let face = extract_face(quad);
    let model = mesh_functions::get_model_matrix(instance_index);

    var out: VertexOutput;
    out.texture = quad.texture_id;
    out.bitfields = quad.bitfields;

    out.normal = mesh_functions::mesh_normal_local_to_world(
        extract_normal(quad),
        get_instance_index(instance_index)
    );

    out.uv = project_to_2d(position, axis_from_face(face)) - quad.min;

    out.world_position = mesh_functions::mesh_position_local_to_world(model, vec4<f32>(position, 1.0))
    out.position = position_world_to_clip(out.world_position.xyz);

    // TODO: tangents
    out.world_tangent = vec4(0.0, 0.0, 0.0, 0.0);

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = get_instance_index(instance_index);
#endif

    return out;
}

