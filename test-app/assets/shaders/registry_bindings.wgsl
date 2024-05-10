#import "shaders/vxl_types.wgsl"::FaceTexture

@group(1) @binding(0) var<storage> faces: array<FaceTexture>;
// the base texture
@group(1) @binding(1) var color_texture: texture_2d_array<f32>;
@group(1) @binding(2) var color_sampler: sampler;
// the normal map
@group(1) @binding(3) var normal_texture: texture_2d_array<f32>;
@group(1) @binding(4) var normal_sampler: sampler;