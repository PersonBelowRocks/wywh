#define_import_path vxl::pbr_input

#import vxl::registry_bindings::{
    faces,
    color_texture,
    color_sampler,
    normal_texture,
    normal_sampler,
}   

#import vxl::utils::{
    index_from_3d_pos,
    project_to_3d,
    project_to_2d,
    axis_from_face,
    face_signum,
    ivec_project_to_3d,
    opposite_face,
    face_from_normal,
    normal_from_face,
    tangent_from_face,
    tex_rotation_matrix_around_axis,
    extract_face,
    extract_texture_rot,
    create_rotation_matrix,
    flipped_uv_x,
    flipped_uv_y,
    uv_coords_from_fs_pos_and_params,
    calculate_mip_level,
}

#import vxl::chunk_io::VertexOutput
#import vxl::types::{
    FaceTexture,
    ChunkQuad,
}

#import vxl::constants::{
    HAS_NORMAL_MAP_BIT,
    CHUNK_OCCLUSION_BUFFER_DIMENSIONS,
    FLIP_UV_X_BIT,
    FLIP_UV_Y_BIT,
    ROTATION_MASK,
    ROTATION_SHIFT,
    DEFAULT_PBR_INPUT_FLAGS,
}

#import bevy_pbr::{
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
        V = normalize(vec3<f32>(view_bindings::view.clip_from_world[0].z, view_bindings::view.clip_from_world[1].z, view_bindings::view.clip_from_world[2].z));
    } else {
        // Only valid for a perpective projection
        V = normalize(view_bindings::view.world_position.xyz - world_position.xyz);
    }
    return V;
}

fn create_pbr_input(
    in: VertexOutput,
    quad: ChunkQuad,
) -> pbr_types::PbrInput {
    var pbr_input: pbr_types::PbrInput = pbr_types::pbr_input_new();

    let face = extract_face(quad);
    let axis = axis_from_face(face);
    let raw_normal = normal_from_face(face);
    let ls_pos = project_to_2d(in.local_position, axis_from_face(face));
    let fs_pos = fract(ls_pos);

    let texture_rot = extract_texture_rot(quad);

    let tangent_rotation_matrix = tex_rotation_matrix_around_axis(texture_rot, axis);
    let tangent = tangent_from_face(face);

    let uv_rotation_matrix = create_rotation_matrix(texture_rot);

    let uv = uv_coords_from_fs_pos_and_params(
        fs_pos,
        uv_rotation_matrix,
        face,
        flipped_uv_x(quad),
        flipped_uv_y(quad),
    );

    pbr_input.flags = DEFAULT_PBR_INPUT_FLAGS;
    pbr_input.is_orthographic = view.clip_from_view[3].w == 1.0;
    pbr_input.V = calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;

    pbr_input.world_normal = raw_normal;

#ifdef LOAD_PREPASS_NORMALS
    pbr_input.N = prepass_utils::prepass_normal(in.position, 0u);
#else
    pbr_input.N = normalize(pbr_input.world_normal);
#endif

    let face_texture = faces[quad.texture_id];
    // TODO: investigate if we can mipmapping working properly
    let mip_level = 0.0;

    pbr_input.material.base_color *= textureSampleLevel(
        color_texture,
        color_sampler,
        uv,
        face_texture.color_tex_idx,
        mip_level
    );

    pbr_input.diffuse_occlusion = vec3(1.0);

    if (face_texture.flags & HAS_NORMAL_MAP_BIT) != 0u {
        pbr_input.N = apply_normal_mapping(
            0u,
            pbr_input.world_normal,
            vec4f(tangent_rotation_matrix * tangent, 0.0),
            uv,
            face_texture.normal_tex_idx,
            mip_level,
        );
    } else {
        pbr_input.N = pbr_input.world_normal;
    }

    return pbr_input;
}

fn apply_normal_mapping(
    standard_material_flags: u32,
    world_normal: vec3<f32>,
    world_tangent: vec4<f32>,
    uv: vec2<f32>,
    texture_array_idx: u32,
    mip_level: f32,
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
    var B: vec3<f32> = 1.0 * cross(N, T);

    // Nt is the tangent-space normal.
    var Nt = textureSampleLevel(
        normal_texture,
        normal_sampler,
        uv,
        texture_array_idx,
        mip_level
    ).rgb;
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