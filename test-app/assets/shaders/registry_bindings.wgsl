#import "shaders/vxl_types.wgsl"::FaceTexture

@group(2) @binding(0) var<storage> faces: array<FaceTexture>;