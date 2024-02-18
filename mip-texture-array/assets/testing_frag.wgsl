#import bevy_pbr::forward_io::{
    VertexOutput,
    FragmentOutput,
}

@group(2) @binding(0) var texarr: texture_2d_array<f32>;
@group(2) @binding(1) var texarr_sampler: sampler;
@group(2) @binding(2) var<uniform> mip_level: u32;
@group(2) @binding(3) var<uniform> array_idx: u32;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

#ifdef VERTEX_UVS
    out.color = textureSampleLevel(
        texarr, 
        texarr_sampler,
        in.uv,
        array_idx,
        f32(mip_level)
    );
#else
    out.color = vec4(0.5, 0.5, 0.5, 1.0);
#endif

    return out;
}