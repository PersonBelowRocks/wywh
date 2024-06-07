#import vxl::chunk_io::VertexOutput
#import vxl::pbr_input::create_pbr_input
#import vxl::utils::extract_face
#import vxl::multidraw_chunk_bindings::quads

#import bevy_pbr::{
    forward_io::FragmentOutput,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

@fragment
fn fragment(
    in: VertexOutput
) -> FragmentOutput {
    let quad = quads[in.quad_idx];
    let face = extract_face(quad);

    var pbr_input = create_pbr_input(in, quad);
    pbr_input.material.base_color.a = 1.0;

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    return out;
}