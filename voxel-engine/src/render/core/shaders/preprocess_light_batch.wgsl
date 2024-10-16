#import vxl::types::{
    IndexedIndirectArgs,
    ChunkInstanceData,
    GpuChunkMetadata,
}
#import vxl::utils::is_valid_indirect_args
#import bevy_render::view::View
#import bevy_render::maths

fn indirect_args_from_metadata(metadata: GpuChunkMetadata) -> IndexedIndirectArgs {
    var args: IndexedIndirectArgs;
    args.index_count = metadata.end_index - metadata.start_index;
    args.instance_count = 1u;
    args.first_index = metadata.start_index;
    args.first_instance = metadata.instance;
    args.base_vertex = 0;
    return args;
}

// Indirect mesh data
@group(0) @binding(0) var<storage, read> all_metadata: array<GpuChunkMetadata>;
@group(0) @binding(1) var<storage, read> all_instances: array<ChunkInstanceData>;

// The view we're preprocessing the batch for
@group(1) @binding(0) var<uniform> view: View;

// The batch data
@group(2) @binding(0) var<storage, read> metadata_indices: array<u32>;
@group(2) @binding(1) var<storage, read_write> indirect_args: array<IndexedIndirectArgs>;
// Gets reset to 0 every pass
@group(2) @binding(2) var<storage, read_write> count: atomic<u32>;

/// Retrieve the perspective camera near clipping plane
fn perspective_camera_near() -> f32 {
    return view.clip_from_view[3][2];
}

/// Convert linear view z to ndc depth. 
/// Note: View z input should be negative for values in front of the camera as -z is forward
fn view_z_to_depth_ndc(view_z: f32) -> f32 {
    // TODO: add shader defs to the pipeline so that these blocks are compiled when needed
#ifdef VIEW_PROJECTION_PERSPECTIVE
    return -perspective_camera_near() / view_z;
#else ifdef VIEW_PROJECTION_ORTHOGRAPHIC
    return view.clip_from_view[3][2] + view_z * view.clip_from_view[2][2];
#else
    let ndc_pos = view.clip_from_view * vec4(0.0, 0.0, view_z, 1.0);
    return ndc_pos.z / ndc_pos.w;
#endif
}

/// Convert a world space position to clip space
fn position_world_to_clip(world_pos: vec3<f32>) -> vec4<f32> {
    let clip_pos = view.clip_from_world * vec4(world_pos, 1.0);
    return clip_pos;
}

// Chunk half extent
const C: f32 = 16.0 / 2.0;

@compute @workgroup_size(1, 1, #{WORKGROUP_SIZE})
fn preprocess_light_batch(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let index = id.z;
    // Need to check before we index into the arrays to avoid weird runtime behaviour
    if arrayLength(&metadata_indices) <= index {
        return;
    }

    // We don't really need to do a check here since we have pretty good control 
    // over this from the CPU side
    let metadata_index = metadata_indices[index];
    let metadata = all_metadata[metadata_index];
    let instance = all_instances[metadata.instance];

    let args = indirect_args_from_metadata(metadata);
    let arg_index = atomicAdd(&count, 1u);
    indirect_args[arg_index] = args;
}