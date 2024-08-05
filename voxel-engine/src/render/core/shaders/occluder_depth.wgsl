#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;

/// Convert a world space position to clip space
fn position_world_to_clip(world_pos: vec3<f32>) -> vec4<f32> {
    let clip_pos = view.clip_from_world * vec4(world_pos, 1.0);
    return clip_pos;
}

@vertex
fn occluder_depth_vertex(
    @location(0) chunk: vec3i,
    @location(1) position: vec3f,
) -> @builtin(position) vec4f {
    let chunk_min = vec3f(chunk) * 16.0;

    let world_position = chunk_min + position;
    return position_world_to_clip(world_position);
}