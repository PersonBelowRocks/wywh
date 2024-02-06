#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import bevy_pbr::forward_io::FragmentOutput

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // TODO: implement
}