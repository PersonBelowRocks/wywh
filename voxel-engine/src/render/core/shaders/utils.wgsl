#define_import_path vxl::utils

#import vxl::types::ChunkQuad

#import vxl::constants::{
    ROTATION_MASK,
    ROTATION_SHIFT,
    FACE_MASK,
    FACE_SHIFT,
    FLIP_UV_X_BIT,
    FLIP_UV_Y_BIT,
}

// from https://community.khronos.org/t/mipmap-level-calculation-using-dfdx-dfdy/67480/2
fn calculate_mip_level(uv: vec2f) -> f32 {
    let dx_vtc = dpdx(uv);
    let dy_vtc = dpdy(uv);

    let delta_max_sqr: f32 = max(dot(dx_vtc, dx_vtc), dot(dy_vtc, dy_vtc));

    return 0.5 * log2(delta_max_sqr);
}

// texture_rot must be below 4
fn create_rotation_matrix(texture_rot: u32) -> mat2x2f {
    let r = radians(90.0 * f32(texture_rot));

    return mat2x2(
        cos(r), -sin(r),
        sin(r),  cos(r),
    );
}

fn tex_rotation_matrix_around_axis(texture_rot: u32, axis: u32) -> mat3x3f {
    let r = radians(90.0 * f32(texture_rot));

    switch axis {
        case AXIS_X: {
            return mat3x3(
                1.0,    0.0,     0.0,
                0.0, cos(r), -sin(r),
                0.0, sin(r),  cos(r),
            );
        }
        case AXIS_Y: {
            return mat3x3(
                cos(r), 0.0, -sin(r),
                0.0   , 1.0,     0.0,
                sin(r), 0.0,  cos(r),
            );
        }
        case AXIS_Z: {
            return mat3x3(
                cos(r), -sin(r), 0.0,
                sin(r),  cos(r), 0.0,
                0.0   ,     0.0, 1.0,
            );
        }
        default: {
            return mat3x3(
                0.0, 0.0, 0.0,
                0.0, 0.0, 0.0,
                0.0, 0.0, 0.0,
            );
        }
    }
}

fn uv_coords_from_fs_pos_and_params(
    fs_pos: vec2f,
    rot: mat2x2<f32>,
    face: u32,
    flip_x: bool,
    flip_y: bool
) -> vec2f {
    var raw_uv = fract(fs_pos);

    // flip UV coordinate V (y) component by default, as the UV origin is in the top left of textures but the facespace
    // origin is in the bottom left
    raw_uv.y = 1.0 - raw_uv.y;

    if face == FACE_NORTH || face == FACE_WEST || face == FACE_TOP {
        raw_uv.x = 1.0 - raw_uv.x;
    }

    if face == FACE_WEST || face == FACE_EAST {
        raw_uv = vec2(1.0) - raw_uv;
    }

    if flip_x {
        raw_uv.x = 1.0 - raw_uv.x;
    }

    if flip_y {
        raw_uv.y = 1.0 - raw_uv.y;
    }

    // we need to center the UVs to rotate it around the origin
    let centered_uv = raw_uv - vec2f(0.5);
    let rotated_uv = rot * centered_uv;

    // reverse the centering before we return
    return rotated_uv + vec2f(0.5);
}

fn flipped_uv_x(quad: ChunkQuad) -> bool {
    return (quad.bitfields.value & FLIP_UV_X_BIT) != 0u;
}

fn flipped_uv_y(quad: ChunkQuad) -> bool {
    return (quad.bitfields.value & FLIP_UV_Y_BIT) != 0u;
}

fn extract_texture_rot(quad: ChunkQuad) -> u32 {
    return (quad.bitfields.value & ROTATION_MASK) >> ROTATION_SHIFT;
}

fn extract_position(quad: ChunkQuad, quad_vertex_index: u32) -> vec3<f32> {
    var pos_2d: vec2<f32>;
    let face = extract_face(quad);
    
    // 0---1
    // |   |
    // 2---3

    // quad: 0, 1, 2, 2, 1, 3
    switch face {
        case FACE_EAST, FACE_SOUTH, FACE_BOTTOM: {
            let positions = array(
                vec2(quad.max.x, quad.min.y),
                vec2(quad.max.x, quad.max.y),
                vec2(quad.min.x, quad.min.y),
                vec2(quad.min.x, quad.max.y),
            );

            // absolutely hilarious workaround to this ridiculous bug:
            // https://github.com/gfx-rs/wgpu/issues/4337
            switch quad_vertex_index {
                case 0u: {
                    pos_2d = positions[0];
                }
                case 1u: {
                    pos_2d = positions[1];
                }
                case 2u: {
                    pos_2d = positions[2];
                }
                case 3u: {
                    pos_2d = positions[3];
                }
                default: {
                    return vec3(0.0, 0.0, 0.0);
                }
            }
        }
        case FACE_WEST, FACE_NORTH, FACE_TOP: {
            let positions = array(
                vec2(quad.min.x, quad.max.y),
                vec2(quad.max.x, quad.max.y),
                vec2(quad.min.x, quad.min.y),
                vec2(quad.max.x, quad.min.y),
            );

            // see above
            switch quad_vertex_index {
                case 0u: {
                    pos_2d = positions[0];
                }
                case 1u: {
                    pos_2d = positions[1];
                }
                case 2u: {
                    pos_2d = positions[2];
                }
                case 3u: {
                    pos_2d = positions[3];
                }
                default: {
                    return vec3(0.0, 0.0, 0.0);
                }
            }
        }

        default: {
            return vec3(0.0, 0.0, 0.0);
        }
    }

    return project_to_3d(pos_2d, axis_from_face(face), f32(quad.magnitude) * 0.25);
}

fn ivec_project_to_3d(pos: vec2<i32>, axis: u32, mag: i32) -> vec3<i32> {
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
            return vec3(0, 0, 0);
        }
    }
}

fn get_magnitude(pos: vec3<f32>, axis: u32) -> f32 {
    switch axis {
        case AXIS_X: {
            return pos.x;
        }
        case AXIS_Y: {
            return pos.y;
        }
        case AXIS_Z: {
            return pos.z;
        }
        default: {
            return 100.0;
        }
    }
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

fn tangent_from_face(face: u32) -> vec3<f32> {
    switch face {
        case FACE_TOP: {
            return vec3(1.0, 0.0, 0.0);
        }
        case FACE_BOTTOM: {
            return vec3(1.0, 0.0, 0.0);
        }
        case FACE_NORTH: {
            return vec3(0.0, 0.0, 1.0);
        }
        case FACE_EAST: {
            return vec3(1.0, 0.0, 0.0);
        }
        case FACE_SOUTH: {
            return vec3(0.0, 0.0, 1.0);
        }
        case FACE_WEST: {
            return vec3(1.0, 0.0, 0.0);
        }
        default: {
            return vec3(0.0, 0.0, 0.0);
        }
    }
}

fn face_from_normal(normal: vec3<i32>) -> u32 {
    if all(normal == vec3<i32>(0, 1, 0)) {
        return FACE_TOP;
    }
    if all(normal == vec3<i32>(0, -1, 0)) {
        return FACE_BOTTOM;
    }
    if all(normal == vec3<i32>(1, 0, 0)) {
        return FACE_NORTH;
    }
    if all(normal == vec3<i32>(0, 0, 1)) {
        return FACE_EAST;
    }
    if all(normal == vec3<i32>(-1, 0, 0)) {
        return FACE_SOUTH;
    }
    if all(normal == vec3<i32>(0, 0, -1)) {
        return FACE_WEST;
    }

    return 100u;
}

fn face_signum(face: u32) -> i32 {
    switch face {
        case FACE_TOP, FACE_NORTH, FACE_EAST: {
            return 1;
        }
        case FACE_BOTTOM, FACE_SOUTH, FACE_WEST: {
            return -1;
        }
        default: {
            return 0;
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

fn opposite_face(face: u32) -> u32 {
    switch face {
        case FACE_NORTH: {
            return FACE_SOUTH;
        }
        case FACE_SOUTH: {
            return FACE_NORTH;
        }
        case FACE_EAST: {
            return FACE_WEST;
        }
        case FACE_WEST: {
            return FACE_EAST;
        }
        case FACE_TOP: {
            return FACE_BOTTOM;
        }
        case FACE_BOTTOM: {
            return FACE_TOP;
        }
        default: {
            return 100u;
        }
    }
}

fn index_from_3d_pos(pos: vec3<u32>, max: u32) -> u32 {
    return (pos.z * max * max) + (pos.y * max) + pos.x;
}