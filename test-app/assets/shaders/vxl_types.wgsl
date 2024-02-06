struct FaceTexture {
    flags: u32,
    color_tex_pos: vec2<f32>,
    normal_tex_pos: vec2<f32>,
}

struct ChunkQuad {
    texture_id: u32,
    rotation: u32,
    min: vec3<f32>,
    max: vec3<f32>
}