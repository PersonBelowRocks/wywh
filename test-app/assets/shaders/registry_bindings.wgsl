#import "shaders/vxl_types.wgsl"::FaceTexture

@group(2) @binding(0) var<storage> faces: array<FaceTexture>;
// the base texture
@group(2) @binding(1) var color_texture: texture_2d<f32>;
@group(2) @binding(2) var color_sampler: sampler;
// the normal map
@group(2) @binding(3) var normal_texture: texture_2d<f32>;
@group(2) @binding(4) var normal_sampler: sampler;