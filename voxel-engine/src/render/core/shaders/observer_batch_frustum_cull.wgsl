#import vxl::types::{
    IndexedIndirectArgs,
    ChunkInstanceData,
}
#import vxl::utils::is_valid_indirect_args
#import bevy_render::view::View
#import bevy_render::maths

const CHUNK_HALF_SIZE: f32 = 16.0 / 2.0;

fn view_frustum_intersects_chunk_sphere(
    chunk_pos: vec3f
) -> bool {

    for (var i = 0; i < 5; i += 1) {
        let plane_normal = view.frustum[i];

        // Check the frustum plane.
        if (!maths::sphere_intersects_plane_half_space(
                plane_normal, vec4(chunk_pos, 1.0), CHUNK_HALF_SIZE)) {
            return false;
        }
    }

    return true;
}

@group(0) @binding(0) var<storage, read> instances: array<ChunkInstanceData>;
@group(0) @binding(1) var<uniform> view: View;
@group(0) @binding(2) var<storage, read_write> indirect_args: array<IndexedIndirectArgs>;
@group(0) @binding(3) var<storage, read_write> count: atomic<u32>;

@compute @workgroup_size(1, 1, 64)
fn batch_frustum_cull(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let index = id.z;
    // Return early if the index is out of bounds
    if index >= arrayLength(&indirect_args) {
        return;
    }

    var args = indirect_args[index];
    args.instance_count = 0u;

    // Reset the instance count
    indirect_args[index] = args;
    let instance_index = args.first_instance;

    if instance_index >= arrayLength(&instances) || !is_valid_indirect_args(args) {
        return;
    }

    args.instance_count = 1u;
    indirect_args[index] = args;

    atomicAdd(&count, 1u);

    // TODO: enable this again
    // let chunk_pos = instances[instance_index].position;
    // if view_frustum_intersects_chunk_sphere(chunk_pos + vec3f(CHUNK_HALF_SIZE)) {
    //     indirect_args[index].instance_count = 1u;
    //     atomicAdd(&count, 1u);
    // }
}