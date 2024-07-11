#import vxl::chunk_io::{MultidrawVertex, PrepassOutput}
#import vxl::multidraw_chunk_bindings::quads

#import bevy_pbr::{
    prepass_io::FragmentOutput,
    pbr_types::PbrInput,
    pbr_deferred_functions::deferred_gbuffer_from_pbr_input,
    pbr_prepass_functions::calculate_motion_vector,
    view_transformations::position_world_to_clip,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

#import vxl::pbr_input::create_pbr_input
#import vxl::utils::{
    normal_from_face,
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

fn deferred_output(in: PrepassOutput, world_normal: vec3f, pbr_input: PbrInput) -> FragmentOutput {
    var out: FragmentOutput;

#ifdef DEFERRED_PREPASS
    // gbuffer
    out.deferred = deferred_gbuffer_from_pbr_input(pbr_input);
    // lighting pass id (used to determine which lighting shader to run for the fragment)
    out.deferred_lighting_pass_id = pbr_input.material.deferred_lighting_pass_id;
    // normal if required
#endif
#ifdef NORMAL_PREPASS
    // TODO: check that normals (and normal mapping) is done correctly
    out.normal = vec4f(world_normal * 0.5 + vec3(0.5), 0.0);
#endif
    // motion vectors if required
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif

    return out;
}

@fragment
fn chunk_fragment(in: PrepassOutput) -> FragmentOutput {
    let quad = quads[in.quad_idx];
    let face = extract_face(quad);
    let normal = normal_from_face(face);

    var pbr_input = create_pbr_input(in, quad);
    pbr_input.material.base_color.a = 1.0;

    let out = deferred_output(in, normal, pbr_input);

    return out;
}