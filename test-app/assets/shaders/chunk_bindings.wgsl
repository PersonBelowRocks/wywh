#import "shaders/vxl_types.wgsl"::ChunkQuad

@group(3) @binding(0) var<storage> quads: array<ChunkQuad>;
@group(3) @binding(1) var<storage> occlusion: array<u32>;