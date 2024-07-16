#import vxl::types::{
    IndexedIndirectArgs,
    ChunkInstanceData,
    GpuChunkMetadata,
}
#import vxl::utils::is_valid_indirect_args
#import bevy_render::view::View
#import bevy_render::maths

const CHUNK_HALF_SIZE: f32 = 16.0 / 2.0;
const CHUNK_SPHERE_RADIUS: f32 = (16.0 * sqrt(3.0)) * 0.5;

fn view_frustum_intersects_chunk_sphere(
    center: vec3f
) -> bool {

    for (var i = 0; i < 5; i += 1) {
        let plane_normal = view.frustum[i];

        let d = dot(plane_normal.xyz, center) + plane_normal.w;
        if d < -CHUNK_SPHERE_RADIUS {
            return false;
        }
    }

    return true;
}

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

@compute @workgroup_size(1, 1, #{WORKGROUP_SIZE})
fn preprocess_batch(
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

    if view_frustum_intersects_chunk_sphere(instance.position + vec3f(CHUNK_HALF_SIZE)) {
        let args = indirect_args_from_metadata(metadata);
        let arg_index = atomicAdd(&count, 1u);
        indirect_args[arg_index] = args;
    }
}