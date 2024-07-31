@group(0) @binding(0) var last_mip: texture_depth_2d;
@group(0) @binding(1) var smplr: sampler;

fn min_texel_depth(texels: array<f32, 4>) -> f32 {
    let i0_1 = min(texels[0], texels[1]);
    let i2_3 = min(texels[2], texels[3]);

    return min(i0_1, i2_3);
}

@fragment
fn construct_hzb_level(
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f
) -> @builtin(frag_depth) f32 {
    
    var texels: array<f32, 4>;
    texels[0] = textureSample(last_mip, smplr, uv);
    texels[1] = textureSample(last_mip, smplr, uv, vec2i(-1, 0));
    texels[2] = textureSample(last_mip, smplr, uv, vec2i(-1, -1));
    texels[3] = textureSample(last_mip, smplr, uv, vec2i(0, -1));

    let furthest: f32 = min_texel_depth(texels);

    return furthest;
}