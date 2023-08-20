#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_functions mesh_position_local_to_clip

#define_import_path bevy_pbr::fragment

#import bevy_pbr::pbr_functions as pbr_functions
#import bevy_pbr::pbr_bindings as pbr_bindings
#import bevy_pbr::pbr_types as pbr_types
#import bevy_pbr::prepass_utils

#import bevy_pbr::mesh_vertex_output       MeshVertexOutput
#import bevy_pbr::mesh_bindings            mesh
#import bevy_pbr::mesh_view_bindings       view, fog, screen_space_ambient_occlusion_texture
#import bevy_pbr::mesh_view_types          FOG_MODE_OFF
#import bevy_core_pipeline::tonemapping    screen_space_dither, powsafe, tone_mapping
#import bevy_pbr::parallax_mapping         parallaxed_uv

#import bevy_pbr::prepass_utils

#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
#import bevy_pbr::gtao_utils gtao_multibounce
#endif
// The time since startup data is in the globals binding which is part of the mesh_view_bindings import

// const UINT_BITS = 32u;
// const G_BS = 3u; // Global bitshift

// const FACE_BITS = 3u;

const FACE_BITMASK: u32 = #{FACE_BITMASK}u;
const FACE_RSHIFT: u32 = #{FACE_RSHIFT}u;

const VXL_X_BITMASK: u32 = #{VXL_X_BITMASK}u;
const VXL_X_RSHIFT: u32 = #{VXL_X_RSHIFT}u;
const VXL_Y_BITMASK: u32 = #{VXL_Y_BITMASK}u;
const VXL_Y_RSHIFT: u32 = #{VXL_Y_RSHIFT}u;
const VXL_Z_BITMASK: u32 = #{VXL_Z_BITMASK}u;
const VXL_Z_RSHIFT: u32 = #{VXL_Z_RSHIFT}u;

const TEX_ATLAS_X_BITMASK: u32 = #{TEX_ATLAS_X_BITMASK}u;
const TEX_ATLAS_X_RSHIFT: u32 = #{TEX_ATLAS_X_RSHIFT}u;
const TEX_ATLAS_Y_BITMASK: u32 = #{TEX_ATLAS_Y_BITMASK}u;
const TEX_ATLAS_Y_RSHIFT: u32 = #{TEX_ATLAS_Y_RSHIFT}u;

const CORNER_BITMASK: u32 = #{CORNER_BITMASK}u;
const CORNER_RSHIFT: u32 = #{CORNER_RSHIFT}u;

// Extract what face this corner is on.
fn extract_face(raw: u32) -> u32 {
    return (raw & FACE_BITMASK) >> FACE_RSHIFT;
}

// let VXL_POS_CMPNT_SIZE = 4u;

// let IDX_X = 3u;
// let IDX_Y = 7u;
// let IDX_Z = 11u;

// Extract the position of the voxel this corner is on.
fn extract_voxel_pos(raw: u32) -> vec3<u32> {
    let x: u32 = (raw & VXL_X_BITMASK) >> VXL_X_RSHIFT;
    let y: u32 = (raw & VXL_Y_BITMASK) >> VXL_Y_RSHIFT;
    let z: u32 = (raw & VXL_Z_BITMASK) >> VXL_Z_RSHIFT;

    return vec3(x, y, z);
}


// let TEX_POS_CMPNT_SIZE = 6u;

// let IDX_TEXTURE_X = 15u;
// let IDX_TEXTURE_Y = 21u;

// Extract which texture the face this corner belongs to uses.
// We need to use this to calculate the UVs.
fn extract_texture_pos(raw: u32) -> vec2<u32> {
    let x = (raw & TEX_ATLAS_X_BITMASK) >> TEX_ATLAS_X_RSHIFT;
    let y = (raw & TEX_ATLAS_Y_BITMASK) >> TEX_ATLAS_Y_RSHIFT;
    
    return vec2(x, y);
}

// let IDX_CORNER = 27u;
// let CORNER_SIZE = 2u;

// Extract the ID of this corner, can be 4 different values.
fn extract_corner_id(raw: u32) -> u32 {
    return (raw & CORNER_BITMASK) >> CORNER_RSHIFT;
}

fn corner_id_to_offset(cornerId: u32) -> vec2<f32> {
    // 0---1
    // |   |
    // 2---3

    switch cornerId {
        case 0u: {return vec2(-0.5,  0.5);} 
        case 1u: {return vec2( 0.5,  0.5);}
        case 2u: {return vec2(-0.5, -0.5);}
        case 3u: {return vec2( 0.5, -0.5);}
        default: {return vec2(0.0);}
    }
}

fn extract_corner(raw: u32) -> vec2<f32> {
    return corner_id_to_offset(extract_corner_id(raw));
}

fn face_to_transform(face: u32) -> mat3x3<f32> {
    // TODO: verify that these transforms are correct
    switch (face) {
        case 0u: {return mat3x3(
            vec3(1.0,  0.0,  0.0),
            vec3(0.0,  1.0,  0.0),
            vec3(0.0,  0.0,  1.0)
        );}
        case 1u: {return mat3x3(
            vec3(1.0,  0.0,  0.0),
            vec3(0.0, -1.0,  0.0),
            vec3(0.0,  0.0, -1.0)
        );}
        case 2u: {return mat3x3(
            vec3(1.0,  0.0,  0.0),
            vec3(0.0,  0.0,  1.0),
            vec3(0.0, -1.0,  0.0)
        );}
        case 3u: {return mat3x3(
            vec3(0.0,  0.0, -1.0),
            vec3(1.0,  0.0,  0.0),
            vec3(0.0, -1.0,  0.0)
        );}
        case 4u: {return mat3x3(
            vec3(-1.0, 0.0,  0.0),
            vec3(0.0,  0.0, -1.0),
            vec3(0.0, -1.0,  0.0)
        );}
        case 5u: {return mat3x3(
            vec3(0.0,  0.0,  1.0),
            vec3(-1.0, 0.0,  0.0),
            vec3(0.0, -1.0,  0.0)
        );}
        default: {return mat3x3(
            vec3(1.0,  0.0,  0.0),
            vec3(0.0,  1.0,  0.0),
            vec3(0.0,  0.0,  1.0)
        );}
    }
}

fn normal_from_face(face: u32) -> vec3<f32> {
    switch face {
        case 0u: {return vec3(0.0,  1.0,  0.0);} // top
        case 1u: {return vec3(0.0, -1.0,  0.0);} // bottom
        case 2u: {return vec3(1.0,  0.0,  0.0);} // north
        case 3u: {return vec3(0.0,  0.0,  1.0);} // east
        case 4u: {return vec3(-1.0, 0.0,  0.0);} // south
        case 5u: {return vec3(0.0,  0.0, -1.0);} // west
        default: {return vec3(0.0);}
    }
}

struct VoxelCorner {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
};

fn unpack_data(raw: u32) -> VoxelCorner {
    let face: u32 = extract_face(raw);

    let voxel_pos = extract_voxel_pos(raw);
    // This is the center of the voxel cube
    let centered_pos = vec3<f32>(voxel_pos) + vec3(0.5);

    let texture_pos = extract_texture_pos(raw);
    // We initially just get the corner of a 2D square
    let corner = extract_corner(raw);

    // Think about this as "moving" the 2D square up by 0.5, giving us the coordinates
    // of the "top" face of the voxel
    let corner_3d = vec3(corner.x, 0.5, corner.y);
    // Rotate the corner to be positioned on the correct face. This does nothing if
    // we're on the top face.
    let rotated_corner = face_to_transform(face) * corner_3d;
    // Now we calculate where in the chunk this corner would be by using our voxel
    // position from earlier.
    let final_corner = centered_pos + rotated_corner;
    
    var voxel_corner: VoxelCorner;

    voxel_corner.position = final_corner;
    voxel_corner.normal = normal_from_face(face);
    voxel_corner.uv = vec2(0.0, 0.0); // TODO: texture coordinate system + texture atlas
    
    return voxel_corner;
}

struct Vertex {
    @location(0) voxel_corner: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> MeshVertexOutput {
    let corner = unpack_data(vertex.voxel_corner);

    var out: MeshVertexOutput;

    out.world_normal = corner.normal;
    out.world_position = vec4(corner.position.xyz, 1.0);
    out.position = mesh_position_local_to_clip(mesh.model, vec4(corner.position, 1.0));

    return out;
}

// struct FragmentInput {
//     @location(0) normal: vec3<f32>,
// };

// @fragment
// fn fragment(input: FragmentInput) -> @location(0) vec4<f32> {
//     return vec4(0.5, 0.25, 0.5, 1.0);
// }

@fragment
fn fragment(
    in: MeshVertexOutput,
    @builtin(front_facing) is_front: bool,
) -> @location(0) vec4<f32> {
    var output_color: vec4<f32> = pbr_bindings::material.base_color;

    let is_orthographic = view.projection[3].w == 1.0;
    let V = pbr_functions::calculate_view(in.world_position, is_orthographic);
#ifdef VERTEX_UVS
    var uv = in.uv;
#ifdef VERTEX_TANGENTS
    if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_DEPTH_MAP_BIT) != 0u) {
        let N = in.world_normal;
        let T = in.world_tangent.xyz;
        let B = in.world_tangent.w * cross(N, T);
        // Transform V from fragment to camera in world space to tangent space.
        let Vt = vec3(dot(V, T), dot(V, B), dot(V, N));
        uv = parallaxed_uv(
            pbr_bindings::material.parallax_depth_scale,
            pbr_bindings::material.max_parallax_layer_count,
            pbr_bindings::material.max_relief_mapping_search_steps,
            uv,
            // Flip the direction of Vt to go toward the surface to make the
            // parallax mapping algorithm easier to understand and reason
            // about.
            -Vt,
        );
    }
#endif
#endif

#ifdef VERTEX_COLORS
    output_color = output_color * in.color;
#endif
#ifdef VERTEX_UVS
    if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {
        output_color = output_color * textureSampleBias(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uv, view.mip_bias);
    }
#endif

    // NOTE: Unlit bit not set means == 0 is true, so the true case is if lit
    if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u) {
        // Prepare a 'processed' StandardMaterial by sampling all textures to resolve
        // the material members
        var pbr_input: pbr_functions::PbrInput;

        pbr_input.material.base_color = output_color;
        pbr_input.material.reflectance = pbr_bindings::material.reflectance;
        pbr_input.material.flags = pbr_bindings::material.flags;
        pbr_input.material.alpha_cutoff = pbr_bindings::material.alpha_cutoff;

        // TODO use .a for exposure compensation in HDR
        var emissive: vec4<f32> = pbr_bindings::material.emissive;
#ifdef VERTEX_UVS
        if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT) != 0u) {
            emissive = vec4<f32>(emissive.rgb * textureSampleBias(pbr_bindings::emissive_texture, pbr_bindings::emissive_sampler, uv, view.mip_bias).rgb, 1.0);
        }
#endif
        pbr_input.material.emissive = emissive;

        var metallic: f32 = pbr_bindings::material.metallic;
        var perceptual_roughness: f32 = pbr_bindings::material.perceptual_roughness;
#ifdef VERTEX_UVS
        if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_METALLIC_ROUGHNESS_TEXTURE_BIT) != 0u) {
            let metallic_roughness = textureSampleBias(pbr_bindings::metallic_roughness_texture, pbr_bindings::metallic_roughness_sampler, uv, view.mip_bias);
            // Sampling from GLTF standard channels for now
            metallic = metallic * metallic_roughness.b;
            perceptual_roughness = perceptual_roughness * metallic_roughness.g;
        }
#endif
        pbr_input.material.metallic = metallic;
        pbr_input.material.perceptual_roughness = perceptual_roughness;

        // TODO: Split into diffuse/specular occlusion?
        var occlusion: vec3<f32> = vec3(1.0);
#ifdef VERTEX_UVS
        if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_OCCLUSION_TEXTURE_BIT) != 0u) {
            occlusion = vec3(textureSampleBias(pbr_bindings::occlusion_texture, pbr_bindings::occlusion_sampler, uv, view.mip_bias).r);
        }
#endif
#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
        let ssao = textureLoad(screen_space_ambient_occlusion_texture, vec2<i32>(in.position.xy), 0i).r;
        let ssao_multibounce = gtao_multibounce(ssao, pbr_input.material.base_color.rgb);
        occlusion = min(occlusion, ssao_multibounce);
#endif
        pbr_input.occlusion = occlusion;

        pbr_input.frag_coord = in.position;
        pbr_input.world_position = in.world_position;

        pbr_input.world_normal = pbr_functions::prepare_world_normal(
            in.world_normal,
            (pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u,
            is_front,
        );

        pbr_input.is_orthographic = is_orthographic;

#ifdef LOAD_PREPASS_NORMALS
        pbr_input.N = bevy_pbr::prepass_utils::prepass_normal(in.position, 0u);
#else
        pbr_input.N = pbr_functions::apply_normal_mapping(
            pbr_bindings::material.flags,
            pbr_input.world_normal,
#ifdef VERTEX_TANGENTS
#ifdef STANDARDMATERIAL_NORMAL_MAP
            in.world_tangent,
#endif
#endif
#ifdef VERTEX_UVS
            uv,
#endif
            view.mip_bias,
        );
#endif

        pbr_input.V = V;
        pbr_input.occlusion = occlusion;

        pbr_input.flags = mesh.flags;

        output_color = pbr_functions::pbr(pbr_input);
    } else {
        output_color = pbr_functions::alpha_discard(pbr_bindings::material, output_color);
    }

    // fog
    if (fog.mode != FOG_MODE_OFF && (pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u) {
        output_color = pbr_functions::apply_fog(fog, output_color, in.world_position.xyz, view.world_position.xyz);
    }

#ifdef TONEMAP_IN_SHADER
    output_color = tone_mapping(output_color, view.color_grading);
#ifdef DEBAND_DITHER
    var output_rgb = output_color.rgb;
    output_rgb = powsafe(output_rgb, 1.0 / 2.2);
    output_rgb = output_rgb + screen_space_dither(in.position.xy);
    // This conversion back to linear space is required because our output texture format is
    // SRGB; the GPU will assume our output is linear and will apply an SRGB conversion.
    output_rgb = powsafe(output_rgb, 2.2);
    output_color = vec4(output_rgb, output_color.a);
#endif
#endif
#ifdef PREMULTIPLY_ALPHA
    output_color = pbr_functions::premultiply_alpha(pbr_bindings::material.flags, output_color);
#endif
    return output_color;
}