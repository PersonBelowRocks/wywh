#define_import_path vxl::types

struct FaceTexture {
    flags: u32,
    color_tex_idx: u32,
    normal_tex_idx: u32,
}

struct ChunkQuad {
    texture_id: u32,
    bitfields: ChunkQuadBitfields,
    min: vec2<f32>,
    max: vec2<f32>,
    magnitude: i32,
}

struct ChunkQuadBitfields {
    value: u32
}