#import bevy_pbr::{
    mesh_functions,
    skinning,
    morph::morph,
    forward_io::{VertexOutput},
    view_transformations::position_world_to_clip,
}
#import bevy_render::instance_index::get_instance_index

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
#ifdef VERTEX_NORMALS
    @location(1) normal: vec3<f32>,
#endif
};

#import "shaders/greedy_mesh_utils.wgsl"::GreedyVertexOutput

const ROTATION_MASK: u32 = #{ROTATION_MASK}u;
const FLIP_UV_X: u32 = #{FLIP_UV_X}u;
const FLIP_UV_Y: u32 = #{FLIP_UV_Y}u;

@vertex
fn vertex(
    vertex_no_morph: Vertex,
    @location(10) texture_id: u32,
    @location(11) misc: u32,
) -> GreedyVertexOutput {
    var out: GreedyVertexOutput;

    var vertex = vertex_no_morph;
    out.texture_id = texture_id;

    let rotation = misc ^ ROTATION_MASK;

    out.texture_rot = radians(90.0 * f32(rotation));

    out.flip_uv_x = (misc & FLIP_UV_X);
    out.flip_uv_y = (misc & FLIP_UV_Y);

    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416 .
    var model = mesh_functions::get_model_matrix(vertex_no_morph.instance_index);

#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        get_instance_index(vertex_no_morph.instance_index)
    );
#endif

#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(model, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#endif

#ifdef VERTEX_NORMALS
    var tangent: vec3<f32>;
    if vertex.normal.y != 0.0 {
        out.uv = vertex.position.xz;
        tangent = vec3(1.0, 0.0, 0.0);
    }
    if vertex.normal.x != 0.0 {
        out.uv = vertex.position.zy;
        tangent = vec3(0.0, 0.0, 1.0);
    }
    if vertex.normal.z != 0.0 {
        out.uv = vertex.position.xy;
        tangent = vec3(1.0, 0.0 ,0.0);
    }

    if out.flip_uv_x != 0u {
        tangent = -tangent;
    }

    let a = out.texture_rot;
    var M: mat3x3<f32>;
    if vertex.normal.y != 0.0 {
        M = mat3x3(
            cos(a), 0.0, -sin(a),
            0.0,    1.0,     0.0,
            sin(a), 0.0,  cos(a),
        );
    }
    if vertex.normal.x != 0.0 {
        M = mat3x3(
            1.0,    0.0,     0.0,
            0.0, cos(a), -sin(a),
            0.0, sin(a),  cos(a),
        );
    }
    if vertex.normal.z != 0.0 {
        M = mat3x3(
            cos(a), -sin(a), 0.0,
            sin(a),  cos(a), 0.0,
            0.0,        0.0, 1.0,
        );
    }

    tangent = M * tangent;

    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        model,
        vec4(tangent.xyz, 0.0),
        // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        get_instance_index(vertex_no_morph.instance_index)
    );
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416
    out.instance_index = get_instance_index(vertex_no_morph.instance_index);
#endif

    return out;
}
