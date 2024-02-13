#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import "shaders/vxl_pbr_input.wgsl"::calculate_occlusion
#import "shaders/utils.wgsl"::face_from_normal
#import "shaders/utils.wgsl"::extract_face
#import "shaders/utils.wgsl"::project_to_2d
#import "shaders/utils.wgsl"::axis_from_face
#import "shaders/utils.wgsl"::get_magnitude
#import "shaders/utils.wgsl"::face_signum
#import bevy_pbr::forward_io::FragmentOutput

#import "shaders/chunk_bindings.wgsl"::quads

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    let quad = quads[in.quad_idx];
    let face = extract_face(quad);

    var out: FragmentOutput;

    let ls_pos_2d = project_to_2d(in.local_position, axis_from_face(face));
    let fs_pos_on_face = fract(ls_pos_2d);

    let occlusion = calculate_occlusion(
        fs_pos_on_face,
        ls_pos_2d,
        face,
        // the magnitude passed to the shader is actually different from the magnitude used on the CPU side.
        // if the quad is pointed in the positive direction of the axis, the magnitude is 1 more than the CPU magnitude.
        // this is to actually give the blocks some volume, as otherwise quads would be placed at the same magnitude
        // no matter the direction they're facing along an axis.
        // e.g., consider two quads coming from the same block, A and B, such that:
        // A is pointing east
        // B is pointing west
        // their dimensions and positions are identical
        // 
        // in this arrangement the CPU side magnitude in a CQS would be the same for both of these
        // because we use the face to distinguish between the two. however for rendering we should "puff" quad A
        // out a bit to give the block its volume.
        // this happens in the mesher as part of converting the data to the format used in shaders, but we need to
        // reverse it here to calculate our occlusion!
        // TODO: this is (probably) not the best way of doing this, investigate better ways of doing it
        quad.magnitude - max(0, face_signum(face)),
    );

    out.color = vec4(occlusion * 1.25, 0.15, 0.15, 1.0);

    return out;

    // TODO: implement
}