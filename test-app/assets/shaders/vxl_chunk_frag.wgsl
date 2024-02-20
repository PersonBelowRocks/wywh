#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import "shaders/vxl_pbr_input.wgsl"::create_pbr_input

#import "shaders/utils.wgsl"::face_from_normal
#import "shaders/utils.wgsl"::extract_face
#import "shaders/utils.wgsl"::project_to_2d
#import "shaders/utils.wgsl"::axis_from_face
#import "shaders/utils.wgsl"::get_magnitude
#import "shaders/utils.wgsl"::face_signum


#import bevy_pbr::{
    forward_io::FragmentOutput,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

#import "shaders/chunk_bindings.wgsl"::quads

const TEXTURE_SCALING: f32 = 16.0;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    let quad = quads[in.quad_idx];
    let face = extract_face(quad);

    var out: FragmentOutput;

    var pbr_input = create_pbr_input(in, quad, TEXTURE_SCALING);
    pbr_input.material.base_color.a = 1.0;

    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    // out.color = pbr_input.material.base_color;

    return out;

    // TODO: implement
}