#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import "shaders/vxl_pbr_input.wgsl"::calculate_occlusion
#import "shaders/utils.wgsl"::face_from_normal
#import "shaders/utils.wgsl"::project_to_2d
#import "shaders/utils.wgsl"::axis_from_face
#import "shaders/utils.wgsl"::get_magnitude
#import bevy_pbr::forward_io::FragmentOutput

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var out: FragmentOutput;

    let ls_pos_2d = project_to_2d(in.local_position, axis_from_face(in.face));
    let fs_pos_on_face = fract(ls_pos_2d);

    let occlusion = calculate_occlusion(
        fs_pos_on_face,
        ls_pos_2d,
        in.face,
        get_magnitude(in.local_position, axis_from_face(in.face)),
    );

    out.color = vec4(occlusion, 0.5, 0.5, 1.0);

    return out;

    // TODO: implement
}