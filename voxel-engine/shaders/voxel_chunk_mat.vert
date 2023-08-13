#version 450

// Highly compacted data about the voxel corner.
layout(location = 0) in uint voxelData;

layout(location = 0) out vec3 normal;

layout(set = 0, binding = 0) uniform CameraViewProj {
    mat4 ViewProj;
    mat4 View;
    mat4 InverseView;
    mat4 Projection;
    vec3 WorldPosition;
    float width;
    float height;
};

layout(set = 2, binding = 0) uniform Mesh {
    mat4 Model;
    mat4 InverseTransposeModel;
    uint flags;
};

const uint UINT_BITS = 32;

const uint FACE_BITS = 3;

// Extract what face this corner is on.
uint extractFace(uint raw) {
    return (raw & 0xE0000000) >> (UINT_BITS - FACE_BITS);
}

const uint VXL_POS_CMPNT_SIZE = 4;

const uint IDX_X = 3;
const uint IDX_Y = IDX_X+VXL_POS_CMPNT_SIZE;
const uint IDX_Z = IDX_Y+VXL_POS_CMPNT_SIZE;

// Extract the position of the voxel this corner is on.
uvec3 extractVoxelPos(uint raw) {
    uint x = (raw & 0x1E000000) >> (UINT_BITS - (IDX_X+VXL_POS_CMPNT_SIZE));
    uint y = (raw & 0x01E00000) >> (UINT_BITS - (IDX_Y+VXL_POS_CMPNT_SIZE));
    uint z = (raw & 0x001E0000) >> (UINT_BITS - (IDX_Z+VXL_POS_CMPNT_SIZE));

    return uvec3(x, y, z);
}

const uint TEX_POS_CMPNT_SIZE = 6;

const uint IDX_TEXTURE_X = 15;
const uint IDX_TEXTURE_Y = IDX_TEXTURE_X+TEX_POS_CMPNT_SIZE;

// Extract which texture the face this corner belongs to uses.
// We need to use this to calculate the UVs.
uvec2 extractTexturePos(uint raw) {
    uint x = (raw & 0x1F800) >> (UINT_BITS - (IDX_TEXTURE_X+TEX_POS_CMPNT_SIZE));
    uint y = (raw & 0x007E0) >> (UINT_BITS - (IDX_TEXTURE_Y+TEX_POS_CMPNT_SIZE));
    
    return uvec2(x, y);
}

const uint IDX_CORNER = 27;
const uint CORNER_SIZE = 2;

// Extract the ID of this corner, can be 4 different values.
uint extractCornerId(uint raw) {
    return (raw & 0x18) >> (UINT_BITS - (IDX_CORNER+CORNER_SIZE));
}

vec2 cornerIdToOffset(uint cornerId) {

    /* 
    0---1
    |   |
    2---3
    */

    switch (cornerId) {
        case(0): return vec2(-0.5,  0.5);
        case(1): return vec2( 0.5,  0.5);
        case(2): return vec2(-0.5, -0.5);
        case(3): return vec2( 0.5, -0.5);
        default: return vec2(0.0);
    };
}

vec2 extractCorner(uint raw) {
    return cornerIdToOffset(extractCornerId(raw));
}

mat3 faceToTransform(uint face) {
    // TODO: verify that these transforms are correct
    switch (face) {
        case(0): return mat3(1);
        case(1): return mat3(
            1,  0,  0,
            0, -1,  0,
            0,  0, -1
        );
        case(2): return mat3(
            1,  0,  0,
            0,  0,  1,
            0, -1,  0
        );
        case(3): return mat3(
            0,  0, -1,
            1,  0,  0,
            0, -1,  0
        );
        case(4): return mat3(
           -1,  0,  0,
            0,  0, -1,
            0, -1,  0
        );
        case(5): return mat3(
            0,  0,  1,
           -1,  0,  0,
            0, -1,  0
        );
    }
}

vec3 normalFromFace(uint face) {
    switch (face) {
        case(0): return vec3(0,  1,  0); // top
        case(1): return vec3(0, -1,  0); // bottom
        case(2): return vec3(1,  0,  0); // north
        case(3): return vec3(0,  0,  1); // east
        case(4): return vec3(-1, 0,  0); // south
        case(5): return vec3(0,  0, -1); // west
    }
}

struct VoxelCorner {
    vec3 position;
    vec3 normal;
    vec2 uv;
};

VoxelCorner unpackData(uint raw) {
    uint face = extractFace(raw);

    uvec3 voxelPos = extractVoxelPos(raw);
    // This is the center of the voxel cube
    vec3 centeredPos = voxelPos + vec3(0.5);

    uvec2 texturePos = extractTexturePos(raw);
    // We initially just get the corner of a 2D square
    vec2 corner = extractCorner(raw);

    // Think about this as "moving" the 2D square up by 0.5, giving us the coordinates
    // of the "top" face of the voxel
    vec3 corner3D = vec3(corner.x, 0.5, corner.y);
    // Rotate the corner to be positioned on the correct face. This does nothing if
    // we're on the top face.
    vec3 rotatedCorner = faceToTransform(face) * corner3D;
    // Now we calculate where in the chunk this corner would be by using our voxel
    // position from earlier.
    vec3 finalCorner = centeredPos + rotatedCorner;

    return VoxelCorner(
        finalCorner,
        normalFromFace(face),
        vec2(0,0) // TODO: texture coordinate system + texture atlas
    );
}

void main() {
    VoxelCorner corner = unpackData(voxelData);

    normal = corner.normal;
    gl_Position = ViewProj * Model * vec4(corner.position, 1.0);
}