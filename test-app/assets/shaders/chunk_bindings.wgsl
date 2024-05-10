#import "shaders/vxl_types.wgsl"::ChunkQuad

@group(2) @binding(0) var<uniform> chunk_position: vec3f;
@group(2) @binding(1) var<storage> quads: array<ChunkQuad>;
@group(2) @binding(2) var<storage> occlusion: array<u32>;