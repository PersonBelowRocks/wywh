struct FaceTexture {
    flags: u32,
    color_tex_pos: vec2<f32>,
    normal_tex_pos: vec2<f32>,
}

struct ChunkQuad {
    texture_id: u32,
    bitfields: ChunkQuadBitfields,
    min: vec2<f32>,
    max: vec2<f32>,
    layer: f32,
}

struct ChunkQuadBitfields {
    value: u32
}