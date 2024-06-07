#import vxl::chunk_io::PrepassOutput
#import vxl::multidraw_chunk_bindings::quads

#import vxl::utils::{
    normal_from_face,
    extract_face
}

#import bevy_pbr::{
    prepass_io::FragmentOutput,
    prepass_bindings,
    mesh_view_bindings::{view, previous_view_proj},
}

@fragment
fn fragment(
    in: PrepassOutput,
) -> FragmentOutput {
    let quad = quads[in.quad_idx];

    let face = extract_face(quad);
    let world_normal = normal_from_face(face);

    var out: FragmentOutput;

#ifdef NORMAL_PREPASS
    // not sure why this happens but we need to do this little funny operation on the normal otherwise rendering is all
    // messed up. this code essentially replicates what bevy does here:
    // https://github.com/bevyengine/bevy/blob/ecdd1624f302c5f71aaed95b0984cbbecf8880b7/crates/bevy_pbr/src/deferred/pbr_deferred_functions.wgsl#L121
    out.normal = vec4f(world_normal * 0.5 + vec3(0.5), 0.0);
#endif

#ifdef MOTION_VECTOR_PREPASS
    let clip_position_t = view.unjittered_view_proj * in.world_position;
    let clip_position = clip_position_t.xy / clip_position_t.w;
    let previous_clip_position_t = prepass_bindings::previous_view_proj * in.previous_world_position;
    let previous_clip_position = previous_clip_position_t.xy / previous_clip_position_t.w;

    out.motion_vector = (clip_position - previous_clip_position) * vec2(0.5, -0.5);
#endif

    // TODO: do we need this?
#ifdef DEFERRED_PREPASS
    out.deferred = vec4<u32>(0u, 0u, 0u, 0u);
    out.deferred_lighting_pass_id = 0u;
#endif

#ifdef DEPTH_CLAMP_ORTHO
    out.frag_depth = in.clip_position_unclamped.z;
#endif

    return out;
}