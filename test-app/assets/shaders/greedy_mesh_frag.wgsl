#import bevy_pbr::{
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#endif

@group(2) @binding(100)
var<uniform> texture_scale: f32;

@fragment
fn fragment(
    raw_in: VertexOutput,
    @location(10) @interpolate(flat) texture: vec2<f32>,
    @location(11) @interpolate(flat) texture_rot: f32,
    @location(12) @interpolate(flat) flip_uv_x: u32,
    @location(13) @interpolate(flat) flip_uv_y: u32,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in: VertexOutput;

    in.position = raw_in.position;
    in.world_position = raw_in.world_position;
    in.world_normal = raw_in.world_normal;

    let fract_uv = fract(raw_in.uv);
    var uv: vec2<f32> = fract_uv;

    if flip_uv_x != 0u {
        uv.x = (1.0 - uv.x);
    }

    if flip_uv_y != 0u {
        uv.y = (1.0 - uv.y);
    }

    let a = texture_rot;
    let M = mat2x2(
        cos(a), -sin(a),
        sin(a),  cos(a)
    );

    let offset = vec2(0.5, 0.5);
    uv = (M * (uv - offset)) + offset;

    in.uv = ((uv / texture_scale) + texture);

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    in.instance_index = raw_in.instance_index;
#endif

    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    // write the gbuffer, lighting pass id, and optionally normal and motion_vector textures
    let out = deferred_output(in, pbr_input);
#else
    // in forward mode, we calculate the lit color immediately, and then apply some post-lighting effects here.
    // in deferred mode the lit color and these effects will be calculated in the deferred lighting shader
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}