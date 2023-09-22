#import bevy_pbr::mesh_functions
#import bevy_pbr::skinning
#import bevy_pbr::morph
#import bevy_pbr::mesh_bindings mesh
#import bevy_render::instance_index get_instance_index
#import bevy_pbr::mesh_vertex_output MeshVertexOutput

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
        case 3u: {return mat3x3(
            vec3(1.0,  0.0,  0.0),
            vec3(0.0,  0.0,  1.0),
            vec3(0.0, -1.0,  0.0)
        );}
        case 2u: {return mat3x3(
            vec3(0.0,  0.0, -1.0),
            vec3(1.0,  0.0,  0.0),
            vec3(0.0, -1.0,  0.0)
        );}
        case 5u: {return mat3x3(
            vec3(-1.0, 0.0,  0.0),
            vec3(0.0,  0.0, -1.0),
            vec3(0.0, -1.0,  0.0)
        );}
        case 4u: {return mat3x3(
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
        case 0u: {return vec3(0.0, 1.0, 0.0);} // top
        case 1u: {return vec3(0.0, -1.0, 0.0);} // bottom
        case 2u: {return vec3(1.0, 0.0, 0.0);} // north
        case 3u: {return vec3(0.0, 0.0, 1.0);} // east
        case 4u: {return vec3(-1.0, 0.0, 0.0);} // south
        case 5u: {return vec3(0.0, 0.0, -1.0);} // west
        default: {return vec3(0.0);}
    }
}

fn swizzle_transform_corner(tfc: vec2<f32>, face_id: u32) -> vec3<f32> {
    // tfc = top face corner
    switch face_id {
        case 0u: {return vec3(tfc.x, 0.5, tfc.y);}
        case 1u: {return vec3(tfc.x, -0.5, tfc.y);}
        case 2u: {return vec3(0.5, tfc.x, tfc.y);}
        case 3u: {return vec3(tfc.x, tfc.y, 0.5);}
        case 4u: {return vec3(-0.5, tfc.x, tfc.y);}
        case 5u: {return vec3(tfc.x, tfc.y, -0.5);}
        default: {return vec3(tfc.x, 0.5, tfc.y);}
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

    let corner_3d = swizzle_transform_corner(corner, face);
    // Think about this as "moving" the 2D square up by 0.5, giving us the coordinates
    // of the "top" face of the voxel
    // let corner_3d = vec3(corner.x, 0.5, corner.y);
    // Rotate the corner to be positioned on the correct face. This does nothing if
    // we're on the top face.
    // let rotated_corner = face_to_transform(face) * corner_3d;
    // Now we calculate where in the chunk this corner would be by using our voxel
    // position from earlier.
    let final_corner = centered_pos + corner_3d;
    
    var voxel_corner: VoxelCorner;

    voxel_corner.position = final_corner;
    voxel_corner.normal = normal_from_face(face);
    voxel_corner.uv = vec2(1.0, 1.0); // TODO: texture coordinate system + texture atlas
    
    return voxel_corner;
}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) voxel_corner: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> MeshVertexOutput {
    let corner = unpack_data(vertex.voxel_corner);
    var model = bevy_pbr::mesh_functions::get_model_matrix(vertex.instance_index);

    var out: MeshVertexOutput;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = get_instance_index(vertex.instance_index);
#endif
    
#ifdef VERTEX_NORMALS
    out.world_normal = bevy_pbr::mesh_functions::mesh_normal_local_to_world(
        corner.normal,

        get_instance_index(vertex.instance_index)
    );
#endif

#ifdef VERTEX_UVS
    out.uv = corner.uv;
#endif
    // out.world_position = vec4(corner.position.xyz, 1.0);
    // out.position = bevy_pbr::mesh_functions::mesh_position_local_to_clip(
    //     model, 
    //     vec4(corner.position, 1.0)
    // );

    out.world_position = bevy_pbr::mesh_functions::mesh_position_local_to_world(model, vec4<f32>(corner.position, 1.0));
    out.position = bevy_pbr::mesh_functions::mesh_position_world_to_clip(out.world_position);
    
#ifdef VERTEX_COLORS
    out.color = vec4(0.1, 0.1, 0.15, 1.0);
#endif

    return out;
}