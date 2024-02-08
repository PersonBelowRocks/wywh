#import "shaders/vxl_chunk_io.wgsl"::PrepassOutput
#import bevy_pbr::prepass_io::FragmentOutput

@fragment
fn fragment(
    in: PrepassOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {

    var out: FragmentOutput;

#ifdef NORMAL_PREPASS
    out.normal = vec4<f32>(0.0, 0.0, 0.0, 0.0);
#endif

#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = vec2<f32>(0.0, 0.0);
#endif

#ifdef DEFERRED_PREPASS
    out.deferred = vec4<u32>(0u, 0u, 0u, 0u);
    out.deferred_lighting_pass_id = 0u;
#endif

#ifdef DEPTH_CLAMP_ORTHO
    out.frag_depth = 0.0;
#endif

    return out;

    // TODO: implement
}