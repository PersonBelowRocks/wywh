#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import bevy_pbr::forward_io::FragmentOutput

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var out: FragmentOutput;

    out.color = vec4(in.color, 1.0);

    return out;

    // TODO: implement
}