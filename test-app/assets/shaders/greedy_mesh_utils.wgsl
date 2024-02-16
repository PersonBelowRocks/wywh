#import bevy_pbr::{
    pbr_types,
    pbr_bindings,
    pbr_functions,
    pbr_fragment,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    lighting,
    transmission,
    clustered_forward as clustering,
    shadows,
    ambient,
    mesh_types::{MESH_FLAGS_SHADOW_RECEIVER_BIT, MESH_FLAGS_TRANSMITTED_SHADOW_RECEIVER_BIT},
    utils::E,
    prepass_utils,
    mesh_bindings::mesh,
    mesh_view_bindings::view,
    parallax_mapping::parallaxed_uv,
    forward_io::VertexOutput,
}

#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
#import bevy_pbr::mesh_view_bindings::screen_space_ambient_occlusion_texture
#import bevy_pbr::gtao_utils::gtao_multibounce
#endif

#ifdef ENVIRONMENT_MAP
#import bevy_pbr::environment_map
#endif

#import bevy_core_pipeline::tonemapping::{screen_space_dither, powsafe, tone_mapping}

struct FaceTexture {
    flags: u32,
    color_tex_pos: vec2<f32>,
    normal_tex_pos: vec2<f32>,
}

struct GreedyVertexOutput {
    // This is `clip position` when the struct is used as a vertex stage output
    // and `frag coord` when used as a fragment stage input
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_tangent: vec4<f32>,

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(5) @interpolate(flat) instance_index: u32,
#endif
    @location(10) @interpolate(flat) texture_id: u32,
    @location(11) @interpolate(flat) texture_rot: f32,

    @location(12) @interpolate(flat) flip_uv_x: u32,
    @location(13) @interpolate(flat) flip_uv_y: u32,

    @location(14) occlusion: f32,
}

fn preprocess_greedy_vertex_output(raw_in: GreedyVertexOutput) -> GreedyVertexOutput {
    var in: GreedyVertexOutput = raw_in;

    let fract_uv = fract(in.uv);
    var uv: vec2<f32> = fract_uv;

    if in.flip_uv_x != 0u {
        uv.x = (1.0 - uv.x);
    }

    if in.flip_uv_y != 0u {
        uv.y = (1.0 - uv.y);
    }

    let a = in.texture_rot;
    let M = mat2x2(
        cos(a), -sin(a),
        sin(a),  cos(a)
    );

    let offset = vec2(0.5, 0.5);
    in.uv = (M * (uv - offset)) + offset;

    in.occlusion = raw_in.occlusion;

    return in;
}



fn occlusion_curve(o: f32) -> f32 {
    let unclamped = (0.1 / (-o + 1.11)) - 0.25;
    return clamp(unclamped, 0.0, 1.0);
}