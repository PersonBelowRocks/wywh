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

struct GpuChunkMetadata {
    position: vec3f,
    start_index: u32,
    end_index: u32,
    start_quad: u32,
    end_quad: u32,
}

struct IndexedIndirectArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

fn indexed_args_from_metadata_and_instance(metadata: GpuChunkMetadata, instance: u32) -> IndexedIndirectArgs {
    var args: IndexedIndirectArgs;
    args.index_count = metadata.end_index - metadata.start_index;
    args.instance_count = 0;
    args.first_index = metadata.start_index;
    args.first_instance = instance;
    args.base_vertex = 0;
    return args;
}

fn empty_indexed_indirect_args() -> IndexedIndirectArgs {
    var args: IndexedIndirectArgs;
    args.index_count = 0u;
    args.instance_count = 0u;
    args.first_index = 0u;
    args.base_vertex = 0;
    args.first_instance = 0u;
    return args;
}

struct ChunkInstanceData {
    position: vec3f,
    base_quad: u32,
}

fn empty_instance_data() -> ChunkInstanceData {
    var data: ChunkInstanceData;
    data.position = vec3f(0.0);
    data.base_quad = 0u;
    return data;
}

fn instance_data_from_metadata(metadata: GpuChunkMetadata) -> ChunkInstanceData {
    var chunk_instance_data: ChunkInstanceData;
    chunk_instance_data.position = metadata.position;
    chunk_instance_data.base_quad = metadata.start_quad;
    return chunk_instance_data;
}