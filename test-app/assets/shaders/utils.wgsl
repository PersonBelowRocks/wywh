#import "shaders/vxl_types.wgsl"::ChunkQuad

#import "shaders/constants.wgsl"::ROTATION_MASK
#import "shaders/constants.wgsl"::ROTATION_SHIFT
#import "shaders/constants.wgsl"::FACE_MASK
#import "shaders/constants.wgsl"::FACE_SHIFT
#import "shaders/constants.wgsl"::FLIP_UV_X_SHIFT
#import "shaders/constants.wgsl"::FLIP_UV_Y_SHIFT

fn extract_position(quad: ChunkQuad, quad_vertex_index: u32) -> vec3<f32> {
    var pos_2d: vec2<f32>;
    switch quad_vertex_index {
        case 0u: {
            pos_2d = vec2(quad.min.x, quad.max.y);
        }
        case 1u: {
            pos_2d = vec2(quad.max.x, quad.max.y);
        }
        case 2u: {
            pos_2d = vec2(quad.min.x, quad.min.y);
        }
        case 3u: {
            pos_2d = vec2(quad.max.x, quad.min.y);
        }
        default: {
            return vec3(0.0, 0.0, 0.0);
        }
    }

    let face = extract_face(quad);
    return project_to_3d(pos_2d, axis_from_face(face), quad.layer);
}

fn project_to_3d(pos: vec2<f32>, axis: u32, mag: f32) -> vec3<f32> {
    switch axis {
        case AXIS_X: {
            return vec3(mag, pos.y, pos.x);
        }
        case AXIS_Y: {
            return vec3(pos.x, mag, pos.y);
        }
        case AXIS_Z: {
            return vec3(pos.x, pos.y, mag);
        }
        default: {
            return vec3(0.0, 0.0, 0.0);
        }
    }
}

fn project_to_2d(pos: vec3<f32>, axis: u32) -> vec2<f32> {
    switch axis {
        case AXIS_X: {
            return vec2(pos.z, pos.y);
        }
        case AXIS_Y: {
            return vec2(pos.x, pos.z);
        }
        case AXIS_Z: {
            return vec2(pos.x, pos.y);
        }
        default: {
            return vec2(0.0, 0.0);
        }
    }
}

fn extract_face(quad: ChunkQuad) -> u32 {
    return (quad.bitfields.value & FACE_MASK) >> FACE_SHIFT;
}

fn extract_normal(quad: ChunkQuad) -> vec3<f32> {
    let face = extract_face(quad);
    return normal_from_face(face);
}

const FACE_TOP: u32 = 0u;
const FACE_BOTTOM: u32 = 1u;
const FACE_NORTH: u32 = 2u;
const FACE_EAST: u32 = 3u;
const FACE_SOUTH: u32 = 4u;
const FACE_WEST: u32 = 5u;

fn normal_from_face(face: u32) -> vec3<f32> {
    switch face {
        case FACE_TOP: {
            return vec3(0.0, 1.0, 0.0);
        }
        case FACE_BOTTOM: {
            return vec3(0.0, -1.0, 0.0);
        }
        case FACE_NORTH: {
            return vec3(1.0, 0.0, 0.0);
        }
        case FACE_EAST: {
            return vec3(0.0, 0.0, 1.0);
        }
        case FACE_SOUTH: {
            return vec3(-1.0, 0.0, 0.0);
        }
        case FACE_WEST: {
            return vec3(0.0, 0.0, -1.0);
        }
        default: {
            return vec3(0.0, 0.0, 0.0);
        }
    }
}

const AXIS_X: u32 = 0u;
const AXIS_Y: u32 = 1u;
const AXIS_Z: u32 = 2u;

fn axis_from_face(face: u32) -> u32 {
    switch face {
        case FACE_NORTH, FACE_SOUTH: {
            return AXIS_X;
        }
        case FACE_TOP, FACE_BOTTOM: {
            return AXIS_Y;
        }
        case FACE_EAST, FACE_WEST: {
            return AXIS_Z;
        }
        default: {
            // return Y as default, its really easy to visually identify vertical faces
            // so this is (hopefully) useful for debugging
            return AXIS_Y;
        }
    }
}