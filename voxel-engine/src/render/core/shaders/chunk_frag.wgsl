#import vxl::chunk_io::PrepassOutput
#import vxl::multidraw_chunk_bindings::quads

#import bevy_pbr::{
    prepass_io::FragmentOutput,
    pbr_types::PbrInput,
    pbr_deferred_functions::deferred_gbuffer_from_pbr_input,
    pbr_prepass_functions::calculate_motion_vector,
}

#import vxl::pbr_input::create_pbr_input
#import vxl::utils::{
    normal_from_face,
    extract_face,
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

    #ifdef DEPTH_CLAMP_ORTHO
        out.frag_depth = in.clip_position_unclamped.z;
    #endif

    return out;
}