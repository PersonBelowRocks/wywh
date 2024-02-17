// from https://eliemichel.github.io/LearnWebGPU/basic-compute/image-processing/mipmap-generation.html

@group(0) @binding(0) var previous_mip_level: texture_2d_array<f32>;
@group(0) @binding(1) var next_mip_level: texture_storage_2d_array<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn compute_mipmap(
    @builtin(global_invocation_id) id: vec3<u32>,
) {
    let texture_index = id.z;

    let offset = vec2<u32>(0, 1);
    let color = (
        textureLoad(previous_mip_level, 2 * id.xy + offset.xx, texture_index) +
        textureLoad(previous_mip_level, 2 * id.xy + offset.xy, texture_index) +
        textureLoad(previous_mip_level, 2 * id.xy + offset.yx, texture_index) +
        textureLoad(previous_mip_level, 2 * id.xy + offset.yy, texture_index)
    ) * 0.25;
    textureStore(next_mip_level, id.xy, texture_index, color);
}