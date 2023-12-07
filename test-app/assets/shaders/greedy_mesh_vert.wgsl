#import bevy_pbr::{
    mesh_functions,
    skinning,
    morph::morph,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}
#import bevy_render::instance_index::get_instance_index

struct GreedyVertexOutput {
    // This is `clip position` when the struct is used as a vertex stage output
    // and `frag coord` when used as a fragment stage input
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
#ifdef VERTEX_TANGENTS
    @location(3) world_tangent: vec4<f32>,
#endif
#ifdef VERTEX_COLORS
    @location(4) color: vec4<f32>,
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(5) @interpolate(flat) instance_index: u32,
#endif
    @location(10) @interpolate(flat) texture: vec2<f32>,
    @location(11) @interpolate(flat) texture_rot: f32,

    @location(12) @interpolate(flat) flip_uv_x: u32,
    @location(13) @interpolate(flat) flip_uv_y: u32,
}

const ROTATION_MASK: u32 = #{ROTATION_MASK}u;
const FLIP_UV_X: u32 = #{FLIP_UV_X}u;
const FLIP_UV_Y: u32 = #{FLIP_UV_Y}u;

@vertex
fn vertex(
    vertex_no_morph: Vertex, 
    @location(10) texture: vec2<f32>,
    @location(11) misc: u32,
) -> GreedyVertexOutput {
    var out: GreedyVertexOutput;

    var vertex = vertex_no_morph;
    out.texture = texture;

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

    if vertex.normal.y != 0.0 {
        out.uv = vertex.position.xz;
    }
    if vertex.normal.x != 0.0 {
        out.uv = vertex.position.zy;
    }
    if vertex.normal.z != 0.0 {
        out.uv = vertex.position.xy;
    }

#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        model,
        vertex.tangent,
        // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        get_instance_index(vertex_no_morph.instance_index)
    );
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416
    out.instance_index = get_instance_index(vertex_no_morph.instance_index);
#endif

    return out;
}
