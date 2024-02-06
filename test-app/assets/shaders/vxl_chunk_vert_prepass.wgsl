#import "shaders/vxl_chunk_io.wgsl"::VertexOutput

@vertex
fn vertex(
    @builtin(vertex_index) vertex: u32,
    @location(0) chunk_quad_index: u32,
) -> VertexOutput {
    // TODO: implement
}
