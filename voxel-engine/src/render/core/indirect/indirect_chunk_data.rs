use std::{ops::Range, u32};

use bevy::{
    prelude::*,
    render::{
        render_resource::{Buffer, BufferDescriptor, BufferUsages, ShaderSize, ShaderType},
        renderer::{RenderDevice, RenderQueue},
    },
};
use itertools::Itertools;
use once_cell::unsync::Lazy;
use rangemap::RangeSet;

use crate::{
    render::{meshing::controller::ChunkMeshData, quad::GpuQuad},
    util::{ChunkIndexMap, ChunkMap, ChunkSet},
};

use super::buffer_utils::{to_formatted_bytes, VramArray};

#[derive(Copy, Clone, ShaderType)]
pub struct ChunkInstanceData {
    pub pos: Vec3,
    pub first_quad: u32,
}

/// Argument buffer layout for draw_indexed_indirect commands.
/// Identical to wgpu's `DrawIndexedIndirectArgs` but this type implements
/// the traits required to use it in a `VramArray`.
#[derive(Copy, Clone, Debug, Default, ShaderType)]
pub struct IndexedIndirectArgs {
    /// The number of indices to draw.
    pub index_count: u32,
    /// The number of instances to draw.
    pub instance_count: u32,
    /// The first index within the index buffer.
    pub first_index: u32,
    /// The value added to the vertex index before indexing into the vertex buffer.
    pub base_vertex: i32,
    /// The instance ID of the first instance to draw.
    ///
    /// Has to be 0, unless [`Features::INDIRECT_FIRST_INSTANCE`](crate::Features::INDIRECT_FIRST_INSTANCE) is enabled.
    pub first_instance: u32,
}

fn writable_buffer_desc(label: &'static str, usages: BufferUsages) -> BufferDescriptor<'static> {
    BufferDescriptor {
        label: Some(label),
        size: 0,
        usage: BufferUsages::COPY_DST | usages,
        mapped_at_creation: false,
    }
}

pub const INDEX_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| writable_buffer_desc("chunk_multidraw_index_buffer", BufferUsages::INDEX));

pub const QUAD_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| writable_buffer_desc("chunk_multidraw_quad_buffer", BufferUsages::STORAGE));

pub const INSTANCE_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| writable_buffer_desc("chunk_multidraw_instance_buffer", BufferUsages::VERTEX));

pub const INDIRECT_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| writable_buffer_desc("chunk_multidraw_indirect_buffer", BufferUsages::INDIRECT));

#[derive(Clone)]
pub struct MultidrawBuffers {
    pub index: VramArray<u32>,
    pub quad: VramArray<GpuQuad>,
    pub instance: Buffer,
    pub indirect: Buffer,
}

impl MultidrawBuffers {
    pub fn new(gpu: &RenderDevice) -> Self {
        Self {
            index: VramArray::new("chunk_multidraw_index_buffer", BufferUsages::INDEX, gpu),
            quad: VramArray::new("chunk_multidraw_quad_buffer", BufferUsages::STORAGE, gpu),
            instance: gpu.create_buffer(&INSTANCE_BUFFER_DESC),
            indirect: gpu.create_buffer(&INDIRECT_BUFFER_DESC),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChunkBufferBounds {
    pub indices: Range<u64>,
    pub quads: Range<u64>,
}

impl ChunkBufferBounds {
    pub fn num_indices(&self) -> u64 {
        self.indices.end - self.indices.start
    }

    pub fn num_quads(&self) -> u64 {
        self.quads.end - self.quads.start
    }
}

#[inline]
fn indirect_args_from_bounds_and_index(
    bounds: &ChunkBufferBounds,
    idx: usize,
) -> IndexedIndirectArgs {
    IndexedIndirectArgs {
        first_instance: idx as u32,
        instance_count: 1,
        first_index: bounds.indices.start as u32,
        index_count: bounds.num_indices() as u32,
        // We're only using an index buffer and an instance buffer, so we'll never end up using this
        base_vertex: 0,
    }
}

/// Stores chunk rendering data in contiguous buffers on the GPU and associated chunk positions with shares of these buffers.
/// Meant to be used to set up data correctly for indirect multidraw rendering.
///
/// There are 4 main buffers that [`ChunkMultidrawData`] manages:
/// - Index buffer, contains the indices for all the chunks
/// - Quad buffer, contains the quads for all the chunks
/// - Instance buffer, contains the chunk position of each chunk as well as the starting quad
/// - Indirect arg buffer, contains the arguments required for indexed indirect multidraw commands ([`wgpu::util::DrawIndexedIndirectArgs`])
///
/// The instance buffer and indirect arg buffer are quite simple because one value in these buffers corresponds to one chunk.
/// But the index buffers and quad buffers are a bit more complicated. One chunk "owns" a range of values in these buffers,
/// so when that chunk is updated or removed, we need to remove the associated data from these buffers. But additionally
/// we need to update the indirect arg buffer and instance buffer to reflect the change.
///
/// The data looks something like this on the GPU:
/// ```
/// indices: ##########################################################################
/// quads:   ##########################################################################
/// ```
/// Now consider how this data is split up between different chunks:
/// ```
/// instance:         0                         1           2           3                         4
/// indices:   [######][########################][##########][##########][########################]
/// quads:     [################][#########][########][###############][##########################]
/// instance:                   0          1         2                3                           4
/// ```
/// Note that this is not to scale, chunks will always have more indices than quads (each quad needs 4 different indices 6 times).
///
/// Each instance is a different chunk, so there's a 1=1 relationship between instance and chunk.
///
/// Also importantly the instance number increases as the quad and index increases.
/// i.e., if chunk A's instance number is higher than chunk B's instance number, then chunk A's share of the index and quad buffer
/// comes AFTER chunk B's share. All shares are therefore ordered by their owner's instance number.
///
/// On the CPU side we maintain a chunk hashmap that looks a bit like this:
/// ```
/// buffer_bounds = {
///     chunk_0: {
///         instance: 0
///         indices: x..y,
///         quads: z..w
///     },
///     chunk_1: {
///         instance: 1
///         indices: x..y,
///         quads: z..w
///     },
///     chunk_2: {
///         instance: 2
///         indices: x..y,
///         quads: z..w
///     },
///     chunk_3: {
///         instance: 3
///         indices: x..y,
///         quads: z..w
///     },
///     chunk_4: {
///         instance: 4
///         indices: x..y,
///         quads: z..w
///     }
/// }
/// ```
///
/// You might notice that this hashmap looks an awful lot like the indexed indirect arg type that we need for indirect draw calls.
/// This similarity is deliberate, and it simplifies the process of editing the data on the GPU by a lot.
/// Every time we remove chunks, we do roughly the following (all on CPU):
/// - Find all the chunks requested for removal that are present in our `buffer_bounds` (chunks to remove)
/// - Find all chunks that are NOT requested for removal (retained chunks)
/// - Sort the retained chunks by their instance number.
/// - Iterate through the sorted retained chunks
/// - Keep track of the current index, quad, and instance
/// - For each retained chunk, update its bounds to start at the current index/quad,
/// and end at the current index/quad + the amount of indices/quads this chunk owns.
/// Also set the chunk's instance to the current instance number.
/// - Increase the current instance by 1, and the current index/quad by the number of indices/quads this chunk owns.
/// - Create new index and quad buffers on the GPU, and copy all the retained data (i.e., all data excluding the stuff we wanted removed)
/// from our old data buffers into these new buffers. Importantly, this copy preserves the order.
/// - Now we just use these new data buffers instead, and we update our hashmap we use to keep track of which chunk owns what.
/// - And finally we convert the ownership data in our CPU hashmap to indirect args and instances in the instance buffer.
///
/// Uploading chunks to the GPU requires us to do almost this entire removal process but for chunks who's data we want to
/// overwrite. After everything has been removed, we just tack on the extra data for our uploaded chunks on the end of our existing data.
#[derive(Resource, Clone)]
pub struct IndirectChunkData {
    buffers: MultidrawBuffers,
    bounds: ChunkIndexMap<ChunkBufferBounds>,
}

impl IndirectChunkData {
    #[allow(dead_code)]
    pub fn new(gpu: &RenderDevice) -> Self {
        Self {
            buffers: MultidrawBuffers::new(gpu),
            bounds: ChunkIndexMap::default(),
        }
    }

    pub fn buffers(&self) -> &MultidrawBuffers {
        &self.buffers
    }

    fn set_instances(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        instances: &[ChunkInstanceData],
    ) {
        let buffer_size = instances.len() as u64 * u64::from(ChunkInstanceData::SHADER_SIZE);

        let instance_buffer = gpu.create_buffer(&BufferDescriptor {
            label: INSTANCE_BUFFER_DESC.label,
            size: buffer_size,
            usage: INSTANCE_BUFFER_DESC.usage,
            mapped_at_creation: false,
        });

        if instances.len() > 0 {
            let data = to_formatted_bytes(&instances);
            debug_assert_eq!(buffer_size, data.len() as u64);
            queue.write_buffer(&instance_buffer, 0, &data);
        }

        self.buffers.instance = instance_buffer;
    }

    fn set_indirect_args(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        indirect_args: &[IndexedIndirectArgs],
    ) {
        let buffer_size = indirect_args.len() as u64 * u64::from(IndexedIndirectArgs::SHADER_SIZE);

        let indirect_buffer = gpu.create_buffer(&BufferDescriptor {
            label: INDIRECT_BUFFER_DESC.label,
            size: buffer_size,
            usage: INDIRECT_BUFFER_DESC.usage,
            mapped_at_creation: false,
        });

        if indirect_args.len() > 0 {
            let data = to_formatted_bytes(&indirect_args);
            debug_assert_eq!(buffer_size, data.len() as u64);
            queue.write_buffer(&indirect_buffer, 0, &data);
        }

        self.buffers.indirect = indirect_buffer;
    }

    fn update_bounds(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        new_bounds: ChunkIndexMap<ChunkBufferBounds>,
    ) {
        debug_assert!(chunk_bounds_correctly_formatted(&new_bounds));

        let chunk_instances = new_bounds
            .iter()
            .map(|(chunk, bounds)| ChunkInstanceData {
                pos: chunk.worldspace_min().as_vec3(),
                first_quad: bounds.quads.start as u32,
            })
            .collect_vec();

        if !chunk_instances.is_empty() {
            debug_assert_eq!(0, chunk_instances[0].first_quad);
        }

        let indirect_args = new_bounds
            .iter()
            .enumerate()
            .map(|(idx, (_, bounds))| indirect_args_from_bounds_and_index(bounds, idx))
            .collect_vec();

        debug!("Setting instance buffer for multidraw data.");
        self.set_instances(gpu, queue, &chunk_instances);
        debug!("Setting indirect arg buffer for multidraw data.");
        self.set_indirect_args(gpu, queue, &indirect_args);

        debug!("Successfully set instance buffer and indirect arg buffer for multidraw data.");

        self.bounds = new_bounds;
    }

    pub fn upload_chunks(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        chunks_to_upload: ChunkMap<ChunkMeshData>,
    ) {
        if chunks_to_upload.is_empty() {
            return;
        }

        let mut upload_bounds = ChunkIndexMap::<ChunkBufferBounds>::with_capacity_and_hasher(
            chunks_to_upload.len(),
            Default::default(),
        );

        // A set of chunks that this multidraw data had beforehand, but that were present in the provided
        // chunk mesh data that we're supposed to upload. These chunks can be considered "updated" and we want to replace
        // the existing mesh data with the provided (new) data. To do this we must remove the data from our existing
        // buffers and tie the existing chunk with the new data that we have uploaded.
        let mut updated_chunks = ChunkSet::default();
        let mut upload_indices = Vec::<u32>::new();
        let mut upload_quads = Vec::<GpuQuad>::new();

        for (chunk, mesh) in chunks_to_upload.iter() {
            if self.bounds.contains_key(&chunk) {
                updated_chunks.set(chunk);
            }

            let indices_len = upload_indices.len() as u64;
            let quads_len = upload_quads.len() as u64;

            let bounds = ChunkBufferBounds {
                indices: indices_len..(indices_len + mesh.index_buffer.len() as u64),
                quads: quads_len..(quads_len + mesh.quad_buffer.len() as u64),
            };

            debug_assert!(bounds.indices.start < bounds.indices.end);
            debug_assert!(bounds.quads.start < bounds.quads.end);

            debug_assert_eq!(upload_indices.len() as u64, bounds.indices.start);
            debug_assert_eq!(upload_quads.len() as u64, bounds.quads.start);

            // After extending our data here the length of our data should match up
            // with the end bounds in the ChunkBufferBounds value that we made earlier.
            upload_indices.extend(&mesh.index_buffer);
            upload_quads.extend(&mesh.quad_buffer);

            debug_assert_eq!(upload_indices.len() as u64, bounds.indices.end);
            debug_assert_eq!(upload_quads.len() as u64, bounds.quads.end);

            upload_bounds.insert(chunk, bounds);
        }

        // Note down the data ranges for the chunks we want to remove
        let mut remove_indices = RangeSet::<u64>::new();
        let mut remove_quads = RangeSet::<u64>::new();

        for chunk in updated_chunks.iter() {
            let bounds = self.bounds.get(&chunk).expect(
                "we already checked that this chunk was present in our bounds in the earlier loop",
            );

            remove_indices.insert(bounds.indices.clone());
            remove_quads.insert(bounds.quads.clone());
        }

        // Update the data ranges for the chunks we retained (aka. didn't remove)
        let mut retained_bounds = ChunkIndexMap::<ChunkBufferBounds>::with_capacity_and_hasher(
            self.bounds.len(),
            Default::default(),
        );

        let ordered_retained_chunks = self
            .bounds
            .iter()
            .filter(|&(chunk, _)| !updated_chunks.contains(*chunk));

        let mut current_index: u64 = 0;
        let mut current_quad: u64 = 0;

        for (chunk, bounds) in ordered_retained_chunks {
            #[cfg(debug_assertions)]
            {
                let contains_indices = remove_indices.overlaps(&bounds.indices);
                let contains_quads = remove_quads.overlaps(&bounds.quads);

                // Both must be true, otherwise we're removing quads but not indices, or vice versa.
                // Doing so would massively mess up the format of the data, and should never happen.
                debug_assert!(!(contains_indices ^ contains_quads));

                // If this chunk's buffer bounds were marked for removal, then the chunk must also
                // we marked as an updated chunk.
                if updated_chunks.contains(*chunk) {
                    debug_assert!(contains_indices && contains_quads);
                } else {
                    debug_assert!(!contains_indices && !contains_quads);
                }
            }

            let num_indices = bounds.num_indices();
            let num_quads = bounds.num_quads();

            retained_bounds.insert(
                *chunk,
                ChunkBufferBounds {
                    indices: current_index..(num_indices + current_index),
                    quads: current_quad..(num_quads + current_quad),
                },
            );

            current_index += num_indices;
            current_quad += num_quads;
        }

        // Okay now we actually queue up the copying commands for the GPU.
        // After this the bounds in 'retained_bounds' should map correctly into the buffers on the GPU.
        self.buffers.index.remove(gpu, queue, &remove_indices);
        self.buffers.quad.remove(gpu, queue, &remove_quads);

        // Now we shift the bounds and instance numbers of the chunks we're going to upload so that they're placed after our retained chunks.
        let max_retained_index = current_index;
        let max_retained_quad = current_quad;

        retained_bounds.extend(upload_bounds.into_iter().map(|(cpos, mut bounds)| {
            bounds.indices.start += max_retained_index;
            bounds.indices.end += max_retained_index;

            bounds.quads.start += max_retained_quad;
            bounds.quads.end += max_retained_quad;

            (cpos, bounds)
        }));

        let new_bounds = retained_bounds;

        debug!("Uploading indices to the GPU.");
        self.buffers.index.append(queue, gpu, &upload_indices);
        debug!("Uploading quads to the GPU.");
        self.buffers.quad.append(queue, gpu, &upload_quads);
        debug!("Successfully uploaded indices and quads to the GPU.");

        self.update_bounds(gpu, queue, new_bounds);
    }

    pub fn remove_chunks(&mut self, gpu: &RenderDevice, queue: &RenderQueue, chunks: ChunkSet) {
        if chunks.is_empty() {
            return;
        }

        let mut remove_indices = RangeSet::<u64>::new();
        let mut remove_quads = RangeSet::<u64>::new();
        let mut chunks_to_retain = ChunkIndexMap::<ChunkBufferBounds>::with_capacity_and_hasher(
            self.bounds.len(),
            Default::default(),
        );

        let mut current_index: u64 = 0;
        let mut current_quad: u64 = 0;

        for (chunk_pos, bounds) in self.bounds.iter() {
            if chunks.contains(*chunk_pos) {
                remove_indices.insert(bounds.indices.clone());
                remove_quads.insert(bounds.quads.clone());
            } else {
                let new_bounds = ChunkBufferBounds {
                    indices: (current_index..(current_index + bounds.num_indices())),
                    quads: (current_quad..(current_quad + bounds.num_quads())),
                };

                // Plenty of sanity checks here to make sure that the start of a range is always smaller than the end.
                debug_assert!(bounds.indices.start < bounds.indices.end);
                debug_assert!(bounds.quads.start < bounds.quads.end);
                debug_assert!(new_bounds.indices.start < new_bounds.indices.end);
                debug_assert!(new_bounds.quads.start < new_bounds.quads.end);

                chunks_to_retain.insert(*chunk_pos, new_bounds);

                current_index += bounds.num_indices();
                current_quad += bounds.num_quads();
            }
        }

        self.buffers.index.remove(gpu, queue, &remove_indices);
        self.buffers.quad.remove(gpu, queue, &remove_quads);

        self.update_bounds(gpu, queue, chunks_to_retain);
    }

    pub fn num_chunks(&self) -> usize {
        self.bounds.len()
    }
}

/// Tests if a chunk map of a bunch of buffer bounds is correctly formatted.
pub(crate) fn chunk_bounds_correctly_formatted(bounds: &ChunkIndexMap<ChunkBufferBounds>) -> bool {
    // If there's less than 2 different bounds then it's not really possible to format them incorrectly.
    if bounds.len() < 2 {
        return true;
    }

    // The first chunk must be instance 0, and all its bounds must start at 0.
    let first = bounds.get_index(0).unwrap().1;
    let first_chunk_is_correct = 0 == first.indices.start && 0 == first.quads.start;

    if !first_chunk_is_correct {
        error!("First chunk bounds in the provided bounds are incorrect!");
        dbg!(first);

        return false;
    }

    let values_in_order = bounds.iter().collect_vec();

    for w in values_in_order.windows(2) {
        // Previous chunk
        let (_p_cpos, p_bounds) = w[0];
        // Next chunk
        let (_n_cpos, n_bounds) = w[1];

        let is_correct =
            // The previous bounds must start before the next bounds start.
            p_bounds.indices.start < n_bounds.indices.start
            && p_bounds.quads.start < n_bounds.quads.start
            // The chunks must share the data buffers contiguously.
            // i.e., the previous chunk's share must end where the next chunk's starts
            && p_bounds.indices.end == n_bounds.indices.start
            && p_bounds.quads.end == n_bounds.quads.start;

        if !is_correct {
            error!("Relationship between these 2 chunks violates buffer bounds format rules!");
            dbg!(p_bounds);
            dbg!(n_bounds);

            return false;
        }
    }

    true
}
