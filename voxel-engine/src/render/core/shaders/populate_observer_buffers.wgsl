#import vxl::types::{
    IndexedIndirectArgs,
    ChunkInstanceData,
    GpuChunkMetadata,
    empty_instance_data,
    instance_data_from_metadata,
    empty_indexed_indirect_args,
    indexed_args_from_metadata_and_instance,
}

@group(0) @binding(0) var<storage, read> all_metadata: array<GpuChunkMetadata>;
@group(0) @binding(1) var<storage, read> metadata_indices: array<u32>;

@group(0) @binding(2) var<storage, read_write> indirect_args: array<IndexedIndirectArgs>;

@compute @workgroup_size(64)
fn build_buffers(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let index = id.z;
    indirect_args[index] = empty_indexed_indirect_args();

    if arrayLength(&metadata_indices) <= index {
        return;
    }

    let metadata_index = metadata_indices[index];
    let metadata = all_metadata[metadata_index];

    indirect_args[index] = indexed_args_from_metadata_and_instance(metadata, metadata_index);
}