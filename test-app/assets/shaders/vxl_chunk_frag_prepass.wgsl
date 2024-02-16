#import "shaders/vxl_chunk_io.wgsl"::PrepassOutput
#import bevy_pbr::{
    prepass_io::FragmentOutput,
    mesh_functions,
    prepass_bindings,
    mesh_view_bindings::{view, previous_view_proj},
}

#import "shaders/chunk_bindings.wgsl"::quads

#import "shaders/utils.wgsl"::normal_from_face
#import "shaders/utils.wgsl"::extract_face

@fragment
fn fragment(
    in: PrepassOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    let quad = quads[in.quad_idx];

    var out: FragmentOutput;

    let face = extract_face(quad);

    let world_normal = mesh_functions::mesh_normal_local_to_world(
        normal_from_face(face),
        in.instance_index
    );

#ifdef NORMAL_PREPASS
    // not sure why this happens but we need to do this little funny operation on the normal otherwise rendering is all
    // messed up. this code essentially replicates what bevy does here:
    // https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/deferred/pbr_deferred_functions.wgsl#L106
    out.normal = vec4f(world_normal * 0.5 + vec3(0.5), 0.0);
#endif

#ifdef MOTION_VECTOR_PREPASS
    let clip_position_t = view.unjittered_view_proj * in.world_position;
    let clip_position = clip_position_t.xy / clip_position_t.w;
    let previous_clip_position_t = prepass_bindings::previous_view_proj * in.previous_world_position;
    let previous_clip_position = previous_clip_position_t.xy / previous_clip_position_t.w;
    // These motion vectors are used as offsets to UV positions and are stored
    // in the range -1,1 to allow offsetting from the one corner to the
    // diagonally-opposite corner in UV coordinates, in either direction.
    // A difference between diagonally-opposite corners of clip space is in the
    // range -2,2, so this needs to be scaled by 0.5. And the V direction goes
    // down where clip space y goes up, so y needs to be flipped.
    out.motion_vector = (clip_position - previous_clip_position) * vec2(0.5, -0.5);
#endif

#ifdef DEFERRED_PREPASS
    out.deferred = vec4<u32>(0u, 0u, 0u, 0u);
    out.deferred_lighting_pass_id = 0u;
#endif

#ifdef DEPTH_CLAMP_ORTHO
    out.frag_depth = in.clip_position_unclamped.z;
#endif

    return out;

    // TODO: implement
}