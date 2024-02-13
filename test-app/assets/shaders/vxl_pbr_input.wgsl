#import "shaders/registry_bindings.wgsl"::faces
#import "shaders/registry_bindings.wgsl"::color_texture
#import "shaders/registry_bindings.wgsl"::color_sampler
#import "shaders/registry_bindings.wgsl"::normal_texture
#import "shaders/registry_bindings.wgsl"::normal_sampler

#import "shaders/utils.wgsl"::index_from_3d_pos
#import "shaders/utils.wgsl"::project_to_3d
#import "shaders/utils.wgsl"::axis_from_face
#import "shaders/utils.wgsl"::face_signum
#import "shaders/utils.wgsl"::ivec_project_to_3d
#import "shaders/utils.wgsl"::opposite_face
#import "shaders/utils.wgsl"::face_from_normal
#import "shaders/utils.wgsl"::normal_from_face
#import "shaders/utils.wgsl"::extract_face

#import "shaders/vxl_chunk_io.wgsl"::VertexOutput
#import "shaders/vxl_types.wgsl"::FaceTexture
#import "shaders/vxl_types.wgsl"::ChunkQuad

#import "shaders/chunk_bindings.wgsl"::occlusion

#import "shaders/constants.wgsl"::HAS_NORMAL_MAP_BIT
#import "shaders/constants.wgsl"::CHUNK_OCCLUSION_BUFFER_DIMENSIONS

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_view_bindings::view,
    pbr_types,
    mesh_view_bindings as view_bindings,
    prepass_utils,
}

fn standard_material_new() -> pbr_types::StandardMaterial {
    var material: pbr_types::StandardMaterial;

    material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    material.emissive = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    material.perceptual_roughness = 0.5;
    material.metallic = 0.00;
    material.reflectance = 0.5;
    material.diffuse_transmission = 0.0;
    material.specular_transmission = 0.0;
    material.thickness = 0.0;
    material.ior = 1.5;
    material.attenuation_distance = 1.0;
    material.attenuation_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    material.flags = pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE;
    material.alpha_cutoff = 0.5;
    material.parallax_depth_scale = 0.1;
    material.max_parallax_layer_count = 16.0;
    material.max_relief_mapping_search_steps = 5u;
    material.deferred_lighting_pass_id = 1u;

    return material;
}

fn calculate_view(
    world_position: vec4<f32>,
    is_orthographic: bool,
) -> vec3<f32> {
    var V: vec3<f32>;
    if is_orthographic {
        // Orthographic view vector
        V = normalize(vec3<f32>(view_bindings::view.view_proj[0].z, view_bindings::view.view_proj[1].z, view_bindings::view.view_proj[2].z));
    } else {
        // Only valid for a perpective projection
        V = normalize(view_bindings::view.world_position.xyz - world_position.xyz);
    }
    return V;
}

const U8_BITS: u32 = 8u;
const U32_BYTES: u32 = 4u;

fn get_block_occlusion(whole_idx: u32) -> u32 {

    // based on the unit test "test_shader_logic" in "occlusion.rs"
    let actual_idx = whole_idx / U32_BYTES;
    let raw_occlusion_value = occlusion[actual_idx];

    // which byte in the u32 are we interested in
    let subidx = whole_idx % U32_BYTES;
    // a mask that selects the bits in that byte
    let mask = 0xffu << (subidx * U8_BITS);

    // mask and shift our raw value so we get a bitmask of the occluded faces for this block
    let normalized_value = (raw_occlusion_value & mask) >> (subidx * U8_BITS);
    return normalized_value;
}

fn is_occluded_at_offset(
    pos: vec2i,
    // offset must have a magnitude of 1
    offset: vec2i,
    magnitude: i32,
    axis: u32,
) -> bool {
    let centered_pos = ivec_project_to_3d(pos, axis, magnitude);

    let offset_pos = pos + offset;
    let occlusion_pos = ivec_project_to_3d(offset_pos, axis, magnitude);
    let normal = occlusion_pos - centered_pos;

    let index = index_from_3d_pos(vec3u(occlusion_pos + vec3i(1)), CHUNK_OCCLUSION_BUFFER_DIMENSIONS);
    let occlusion = get_block_occlusion(index);

    return (occlusion & (1u << opposite_face(face_from_normal(normal)))) != 0u;
}

fn calculate_occlusion(
    // facespace position on a face, the origin is at the bottom left
    // corner of the face
    fs_pos_on_face: vec2<f32>,
    // localspace position on a face, the origin is at the bottom left
    // of the chunk "layer" this face is on
    ls_pos_on_face: vec2<f32>,
    face: u32,
    mag: i32
) -> f32 {
    let axis = axis_from_face(face);

    let centered_pos = vec2i(floor(ls_pos_on_face));

    let mag_above = mag + face_signum(face);
    let above_centered_pos = ivec_project_to_3d(centered_pos, axis, mag_above);

    // TODO: occlusion math

    // up

    // down
    if is_occluded_at_offset(centered_pos, vec2i(0, -1), mag_above, axis) {
        let centered_fs_pos_on_face = fs_pos_on_face - vec2(0.5);
        
        let t = abs(min(centered_fs_pos_on_face.y, 0.0));
        return t;
    }

    // left

    // right

    return 0.0;
}

fn pbr_input_from_vertex_output(
    in: VertexOutput,
    quad: ChunkQuad,
) -> pbr_types::PbrInput {
    let world_normal = normal_from_face(extract_face(quad));

    var pbr_input: pbr_types::PbrInput = pbr_types::pbr_input_new();

    pbr_input.flags = mesh[in.instance_index].flags;
    pbr_input.is_orthographic = view.projection[3].w == 1.0;
    pbr_input.V = calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;

    pbr_input.world_normal = world_normal;

#ifdef LOAD_PREPASS_NORMALS
    pbr_input.N = prepass_utils::prepass_normal(in.position, 0u);
#else
    pbr_input.N = normalize(pbr_input.world_normal);
#endif

    return pbr_input;
}

fn create_pbr_input(
    in: VertexOutput,
    is_front: bool,
    face: FaceTexture,
    scale: f32,
) -> pbr_types::PbrInput {

    var pbr_input: pbr_types::PbrInput = pbr_input_from_vertex_output(in);
    
    // TODO: UV calculation is a bit more complicated than this, fix it!!
    var uv = fract(in.uv);

    let color_uv = ((uv / scale) + (face.color_tex_pos / scale) / scale);
    pbr_input.material.base_color *= textureSampleBias(color_texture, color_sampler, color_uv, view.mip_bias);

    pbr_input.occlusion = vec3(1.0);

    // N (normal vector)
//#ifndef LOAD_PREPASS_NORMALS

    if (face.flags & HAS_NORMAL_MAP_BIT) != 0u {
        let normal_map_uv = ((uv / scale) + (face.normal_tex_pos / scale) / scale);

        pbr_input.N = apply_normal_mapping(
            0u,
            pbr_input.world_normal,
            vec4(0.0, 0.0, 0.0, 0.0),
            normal_map_uv,
            view.mip_bias,
        );
    } else {
        pbr_input.N = pbr_input.world_normal;
    }

    return pbr_input;
}

// TODO: theres a lot of yapping about mikktspace and whatever here, find out if we care about
// what mikktspace is doing in our case.
fn apply_normal_mapping(
    standard_material_flags: u32,
    world_normal: vec3<f32>,
    world_tangent: vec4<f32>,
    uv: vec2<f32>,
    mip_bias: f32,
) -> vec3<f32> {
    // NOTE: The mikktspace method of normal mapping explicitly requires that the world normal NOT
    // be re-normalized in the fragment shader. This is primarily to match the way mikktspace
    // bakes vertex tangents and normal maps so that this is the exact inverse. Blender, Unity,
    // Unreal Engine, Godot, and more all use the mikktspace method. Do not change this code
    // unless you really know what you are doing.
    // http://www.mikktspace.com/
    var N: vec3<f32> = world_normal;

    // NOTE: The mikktspace method of normal mapping explicitly requires that these NOT be
    // normalized nor any Gram-Schmidt applied to ensure the vertex normal is orthogonal to the
    // vertex tangent! Do not change this code unless you really know what you are doing.
    // http://www.mikktspace.com/
    var T: vec3<f32> = world_tangent.xyz;
    var B: vec3<f32> = world_tangent.w * cross(N, T);

    // Nt is the tangent-space normal.
    var Nt = textureSampleBias(normal_texture, normal_sampler, uv, mip_bias).rgb;
    Nt = Nt * 2.0 - 1.0;
    // TODO: do we need this?
    // Normal maps authored for DirectX require flipping the y component
    if (standard_material_flags & pbr_types::STANDARD_MATERIAL_FLAGS_FLIP_NORMAL_MAP_Y) != 0u {
        Nt.y = -Nt.y;
    }

    // NOTE: The mikktspace method of normal mapping applies maps the tangent-space normal from
    // the normal map texture in this way to be an EXACT inverse of how the normal map baker
    // calculates the normal maps so there is no error introduced. Do not change this code
    // unless you really know what you are doing.
    // http://www.mikktspace.com/
    N = Nt.x * T + Nt.y * B + Nt.z * N;

    return normalize(N);
}