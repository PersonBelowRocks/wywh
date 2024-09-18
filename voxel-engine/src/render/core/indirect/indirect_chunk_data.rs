use std::{ops::Range, u32};

use bevy::{
    prelude::*,
    render::{
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayout, Buffer, BufferDescriptor,
            BufferInitDescriptor, BufferUsages, ShaderSize, ShaderType,
        },
        renderer::{RenderDevice, RenderQueue},
    },
};
use itertools::Itertools;
use rangemap::RangeSet;

use crate::{
    render::{
        core::{
            indirect::buffer_utils::instance_bytes_from_metadata, utils::InspectChunks,
            BindGroupProvider,
        },
        lod::LevelOfDetail,
        meshing::controller::{quads_in_byte_buffer, u32s_in_byte_buffer, ChunkMeshData},
        quad::GpuQuad,
    },
    topo::world::ChunkPos,
    util::{ChunkIndexMap, ChunkMap, ChunkSet},
};

use super::buffer_utils::{to_formatted_bytes, VramArray};

#[derive(Copy, Clone, ShaderType)]
#[repr(C)]
pub struct ChunkInstanceData {
    pub pos: Vec3,
    pub first_quad: u32,
}

/// Argument buffer layout for draw_indexed_indirect commands.
/// Identical to wgpu's `DrawIndexedIndirectArgs` but this type implements
/// the traits required to use it in a `VramArray`.
#[derive(Copy, Clone, Debug, Default, ShaderType)]
#[repr(C)]
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

/// Metadata for the mesh representation of a chunk. Describes the chunk's position, and its share of the
/// indirect index and quad buffers.
#[derive(Copy, Clone, Debug, Default, ShaderType)]
#[repr(C)]
pub struct GpuChunkMetadata {
    // TODO: thorough testing of the logic here, not sure if I managed to get the order of stuff correctly
    pub instance: u32,
    pub start_index: u32,
    pub end_index: u32,
    pub start_quad: u32,
    pub end_quad: u32,
}

impl GpuChunkMetadata {
    pub fn new(instance: u32, indices: Range<u32>, quads: Range<u32>) -> Self {
        debug_assert!(indices.start < indices.end);
        debug_assert!(quads.start < quads.end);

        Self {
            instance,
            start_index: indices.start,
            end_index: indices.end,
            start_quad: quads.start,
            end_quad: quads.end,
        }
    }

    pub fn index_range(&self) -> Range<u32> {
        debug_assert!(self.start_index < self.end_index);
        self.start_index..self.end_index
    }

    pub fn quad_range(&self) -> Range<u32> {
        debug_assert!(self.start_quad < self.end_quad);
        self.start_quad..self.end_quad
    }

    pub fn indices(&self) -> u32 {
        self.end_index - self.start_index
    }

    pub fn quads(&self) -> u32 {
        self.end_quad - self.start_quad
    }
}

fn writable_buffer_desc(label: &'static str, usages: BufferUsages) -> BufferDescriptor<'static> {
    BufferDescriptor {
        label: Some(label),
        size: 0,
        usage: BufferUsages::COPY_DST | usages,
        mapped_at_creation: false,
    }
}

pub static ICD_INDEX_BUFFER_LABEL: &'static str = "ICD_index_buffer";
pub static ICD_QUAD_BUFFER_LABEL: &'static str = "ICD_quad_buffer";
pub static ICD_CHUNK_METADATA_BUFFER_LABEL: &'static str = "ICD_chunk_metadata_buffer";
pub static ICD_CHUNK_INSTANCE_BUFFER_LABEL: &'static str = "ICD_chunk_instance_buffer";

#[derive(Clone)]
pub struct RawIndirectChunkData {
    pub index: VramArray<u32>,
    pub quad: VramArray<GpuQuad>,
    pub metadata: Buffer,
    pub instances: Buffer,
}

impl RawIndirectChunkData {
    pub fn new(gpu: &RenderDevice) -> Self {
        Self {
            index: VramArray::new(ICD_INDEX_BUFFER_LABEL, BufferUsages::INDEX, gpu),
            quad: VramArray::new(ICD_QUAD_BUFFER_LABEL, BufferUsages::STORAGE, gpu),
            metadata: gpu.create_buffer(&BufferDescriptor {
                label: Some(ICD_CHUNK_METADATA_BUFFER_LABEL),
                size: 0,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            instances: gpu.create_buffer(&BufferDescriptor {
                label: Some(ICD_CHUNK_INSTANCE_BUFFER_LABEL),
                size: 0,
                usage: BufferUsages::VERTEX | BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}

#[derive(Clone)]
pub struct IcdBindGroups {
    quad_bg_layout: BindGroupLayout,
    quad_bg: Option<BindGroup>,
    preprocess_metadata_bg_layout: BindGroupLayout,
    preprocess_metadata_bg: Option<BindGroup>,
}

pub struct IcdCommit<'a> {
    add: ChunkMap<ChunkMeshData>,
    remove: ChunkSet,
    inspect: Option<&'a ChunkSet>,
}

impl<'a> IcdCommit<'a> {
    pub fn new() -> Self {
        Self {
            add: Default::default(),
            remove: Default::default(),
            inspect: None,
        }
    }

    pub fn set_inspections(&mut self, inspect: &'a InspectChunks) -> &mut Self {
        self.inspect = Some(inspect);
        self
    }

    pub fn add(&mut self, meshes: ChunkMap<ChunkMeshData>) -> &mut Self {
        self.add.extend(meshes.into_iter());
        self
    }

    pub fn remove(&mut self, remove: ChunkSet) -> &mut Self {
        self.remove.extend(remove.into_iter());
        self
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
#[derive(Clone)]
pub struct IndirectChunkData {
    lod: LevelOfDetail,
    raw: RawIndirectChunkData,
    bind_groups: IcdBindGroups,
    metadata: ChunkIndexMap<GpuChunkMetadata>,
}

impl IndirectChunkData {
    pub fn new(gpu: &RenderDevice, bg_provider: &BindGroupProvider, lod: LevelOfDetail) -> Self {
        Self {
            lod,
            raw: RawIndirectChunkData::new(gpu),
            bind_groups: IcdBindGroups {
                quad_bg_layout: bg_provider.icd_quad_bg_layout.clone(),
                quad_bg: None,
                preprocess_metadata_bg_layout: bg_provider
                    .preprocess_mesh_metadata_bg_layout
                    .clone(),
                preprocess_metadata_bg: None,
            },
            metadata: ChunkIndexMap::default(),
        }
    }

    /// Whether or not this indirect chunk data is in a state where it can be used for rendering.
    /// In order to be ready the quad bind group must be created, and there must be some chunk metadata present.
    pub fn is_ready(&self) -> bool {
        self.quad_bind_group().is_some() && !self.is_empty()
    }

    /// Whether or not this data is empty, i.e., contains no chunks. If the data is empty then the instance buffer and metadata buffer are also
    /// empty and thus can't be used in bind groups.
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    pub fn quad_bind_group(&self) -> Option<&BindGroup> {
        self.bind_groups.quad_bg.as_ref()
    }

    pub fn preprocess_metadata_bind_group(&self) -> Option<&BindGroup> {
        self.bind_groups.preprocess_metadata_bg.as_ref()
    }

    pub fn index_buffer(&self) -> &Buffer {
        self.buffers().index.buffer()
    }

    pub fn quad_buffer(&self) -> &Buffer {
        self.buffers().quad.buffer()
    }

    pub fn instance_buffer(&self) -> &Buffer {
        &self.buffers().instances
    }

    pub fn metadata_buffer(&self) -> &Buffer {
        &self.buffers().metadata
    }

    pub fn buffers(&self) -> &RawIndirectChunkData {
        &self.raw
    }

    fn update_metadata(
        &mut self,
        gpu: &RenderDevice,
        new_metadata: ChunkIndexMap<GpuChunkMetadata>,
    ) {
        debug_assert!(chunk_bounds_correctly_formatted(&new_metadata));

        let metadata_bytes = to_formatted_bytes(&new_metadata.values().collect_vec());
        let instance_bytes = instance_bytes_from_metadata(&new_metadata);

        self.raw.metadata = gpu.create_buffer_with_data(&BufferInitDescriptor {
            label: Some(ICD_CHUNK_METADATA_BUFFER_LABEL),
            usage: BufferUsages::COPY_DST | BufferUsages::STORAGE,
            contents: &metadata_bytes,
        });

        self.raw.instances = gpu.create_buffer_with_data(&BufferInitDescriptor {
            label: Some(ICD_CHUNK_INSTANCE_BUFFER_LABEL),
            usage: BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
            contents: &instance_bytes,
        });

        self.metadata = new_metadata;
    }

    fn update_bind_groups(&mut self, gpu: &RenderDevice) {
        // Remove our old bind groups
        self.bind_groups.quad_bg = None;
        self.bind_groups.preprocess_metadata_bg = None;

        // We only make a new quad bind group if we have any quads
        let quad_vram_array = &self.buffers().quad;
        if quad_vram_array.vram_bytes() > 0 {
            let quad_buffer = quad_vram_array.buffer();

            let bg = gpu.create_bind_group(
                "ICD_quad_bind_group",
                &self.bind_groups.quad_bg_layout,
                &BindGroupEntries::single(quad_buffer.as_entire_buffer_binding()),
            );

            self.bind_groups.quad_bg = Some(bg);
        }

        // We only make a new preprocess metadata bind group if we have any metadata
        if !self.metadata.is_empty() {
            let metadata_buffer = self.metadata_buffer();
            let instance_buffer = self.instance_buffer();

            let bg = gpu.create_bind_group(
                "ICD_preprocess_metadata_bind_group",
                &self.bind_groups.preprocess_metadata_bg_layout,
                &BindGroupEntries::sequential((
                    metadata_buffer.as_entire_binding(),
                    instance_buffer.as_entire_binding(),
                )),
            );

            self.bind_groups.preprocess_metadata_bg = Some(bg);
        }
    }

    pub fn commit(&mut self, gpu: &RenderDevice, queue: &RenderQueue, commit: IcdCommit) {
        // TODO: merge these two operations into one so that we don't duplicate work
        self.remove_chunks(gpu, queue, &commit.remove, commit.inspect);
        self.upload_chunks(gpu, queue, commit.add, commit.inspect);
    }

    pub fn upload_chunks(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        chunks_to_upload: ChunkMap<ChunkMeshData>,
        inspections: Option<&ChunkSet>,
    ) {
        if chunks_to_upload.is_empty() {
            return;
        }

        let mut upload_metadata = ChunkIndexMap::<GpuChunkMetadata>::with_capacity_and_hasher(
            chunks_to_upload.len(),
            Default::default(),
        );

        // A set of chunks that this multidraw data had beforehand, but that were present in the provided
        // chunk mesh data that we're supposed to upload. These chunks can be considered "updated" and we want to replace
        // the existing mesh data with the provided (new) data. To do this we must remove the data from our existing
        // buffers and tie the existing chunk with the new data that we have uploaded.
        let mut updated_chunks = ChunkSet::default();

        let mut upload_index_bytes = Vec::<u8>::new();
        let mut upload_quad_bytes = Vec::<u8>::new();

        for (i, (chunk, mesh)) in chunks_to_upload.iter().enumerate() {
            let inspect = inspections.is_some_and(|insp| insp.contains(chunk));

            if inspect {
                info!(
                    "Uploading chunk mesh for {chunk} to the GPU at LOD {:?}.",
                    self.lod
                );
                info!("Index: {i}");
            }

            if self.metadata.contains_key(&chunk) {
                if inspect {
                    info!("Chunk {chunk} has pre-existing metadata so we will remove it before we upload it.");
                }

                updated_chunks.set(chunk);
            }

            let indices_len: u32 = u32s_in_byte_buffer(upload_index_bytes.len() as u64);
            let quads_len: u32 = quads_in_byte_buffer(upload_quad_bytes.len() as u64);

            let metadata = GpuChunkMetadata::new(
                // We'll let this be our instance number for now. These will all be tacked on to the end of our existing metadata so we just
                // increase this number accordingly when we get that far.
                i as u32,
                indices_len..(indices_len + mesh.indices()),
                quads_len..(quads_len + mesh.quads()),
            );

            if inspect {
                info!("Metadata for chunk {chunk}: {metadata:#?}");
            }

            debug_assert!(metadata.start_index < metadata.end_index);
            debug_assert!(metadata.start_quad < metadata.end_quad);

            debug_assert_eq!(
                u32s_in_byte_buffer(upload_index_bytes.len() as u64),
                metadata.start_index
            );
            debug_assert_eq!(
                quads_in_byte_buffer(upload_quad_bytes.len() as u64),
                metadata.start_quad
            );

            // After extending our data here the length of our data should match up
            // with the end bounds in the ChunkBufferBounds value that we made earlier.
            upload_index_bytes.extend(&mesh.index_buffer_data);
            upload_quad_bytes.extend(&mesh.quad_buffer_data);

            debug_assert_eq!(
                u32s_in_byte_buffer(upload_index_bytes.len() as u64),
                metadata.end_index
            );
            debug_assert_eq!(
                quads_in_byte_buffer(upload_quad_bytes.len() as u64),
                metadata.end_quad
            );

            upload_metadata.insert(chunk, metadata);
        }

        // Note down the data ranges for the chunks we want to remove
        let mut remove_indices = RangeSet::<u32>::new();
        let mut remove_quads = RangeSet::<u32>::new();

        for chunk in updated_chunks.iter() {
            let inspect = inspections.is_some_and(|insp| insp.contains(chunk));

            let metadata = self.metadata.get(&chunk).expect(
                "we already checked that this chunk was present in our metadata in the earlier loop",
            );

            if inspect {
                info!("Old metadata for chunk {chunk}: {metadata:#?}");
                info!("Removing indices and quads for chunk {chunk}.")
            }

            remove_indices.insert(metadata.index_range());
            remove_quads.insert(metadata.quad_range());
        }

        // Update the data ranges for the chunks we retained (aka. didn't remove)
        let mut retained_bounds = ChunkIndexMap::<GpuChunkMetadata>::with_capacity_and_hasher(
            self.metadata.len(),
            Default::default(),
        );

        let ordered_retained_chunks = self
            .metadata
            .iter()
            .filter(|&(chunk, _)| !updated_chunks.contains(*chunk));

        let mut current_index: u32 = 0;
        let mut current_quad: u32 = 0;

        for (i, (chunk, metadata)) in ordered_retained_chunks.enumerate() {
            let inspect = inspections.is_some_and(|insp| insp.contains(*chunk));

            #[cfg(debug_assertions)]
            {
                let contains_indices = remove_indices.overlaps(&metadata.index_range());
                let contains_quads = remove_quads.overlaps(&metadata.quad_range());

                // Both must be true, otherwise we're removing quads but not indices, or vice versa.
                // Doing so would severely mess up the format of the data, and should never happen.
                debug_assert!(!(contains_indices ^ contains_quads));

                // If this chunk's buffer bounds were marked for removal, then the chunk must also
                // we marked as an updated chunk.
                if updated_chunks.contains(*chunk) {
                    debug_assert!(contains_indices && contains_quads);
                } else {
                    debug_assert!(!contains_indices && !contains_quads);
                }
            }

            let num_indices = metadata.indices();
            let num_quads = metadata.quads();

            let new_metadata = GpuChunkMetadata::new(
                i as u32,
                current_index..(num_indices + current_index),
                current_quad..(num_quads + current_quad),
            );

            if inspect {
                info!(
                    "Retaining chunk {chunk} with new metadata as part of uploading: {metadata:#?}"
                );
            }

            retained_bounds.insert(*chunk, new_metadata);

            current_index += num_indices;
            current_quad += num_quads;
        }

        // Okay now we actually queue up the copying commands for the GPU.
        // After this the bounds in 'retained_bounds' should map correctly into the buffers on the GPU.
        self.raw.index.remove(gpu, queue, &remove_indices);
        self.raw.quad.remove(gpu, queue, &remove_quads);

        // Now we shift the bounds of the chunks we're going to upload so that they're placed after our retained chunks.
        let max_retained_index = current_index;
        let max_retained_quad = current_quad;
        let max_instance = retained_bounds.len() as u32;

        retained_bounds.extend(upload_metadata.into_iter().map(|(cpos, mut metadata)| {
            let inspect = inspections.is_some_and(|insp| insp.contains(cpos));

            if inspect {
                info!("OLD retained bounds for chunk {cpos}: {metadata:#?}");
            }

            metadata.start_index += max_retained_index;
            metadata.end_index += max_retained_index;

            metadata.start_quad += max_retained_quad;
            metadata.end_quad += max_retained_quad;

            metadata.instance += max_instance;

            if inspect {
                info!("NEW retained bounds for chunk {cpos}: {metadata:#?}");
            }

            (cpos, metadata)
        }));

        let new_bounds = retained_bounds;

        self.raw.index.append_raw(
            queue,
            gpu,
            &upload_index_bytes,
            u32s_in_byte_buffer(upload_index_bytes.len() as u64),
        );
        self.raw.quad.append_raw(
            queue,
            gpu,
            &upload_quad_bytes,
            quads_in_byte_buffer(upload_quad_bytes.len() as u64),
        );

        self.update_metadata(gpu, new_bounds);
        self.update_bind_groups(gpu);
    }

    pub fn remove_chunks(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        chunks: &ChunkSet,
        inspections: Option<&ChunkSet>,
    ) {
        if chunks.is_empty() {
            return;
        }

        let mut remove_indices = RangeSet::<u32>::new();
        let mut remove_quads = RangeSet::<u32>::new();
        let mut chunks_to_retain = ChunkIndexMap::<GpuChunkMetadata>::with_capacity_and_hasher(
            self.metadata.len(),
            Default::default(),
        );

        let mut current_index: u32 = 0;
        let mut current_quad: u32 = 0;
        let mut current_instance: u32 = 0;

        for (chunk_pos, metadata) in self.metadata.iter() {
            let inspect = inspections.is_some_and(|insp| insp.contains(*chunk_pos));

            if chunks.contains(*chunk_pos) {
                if inspect {
                    info!("Removing chunk {chunk_pos} at LOD {:?}", self.lod);
                }

                remove_indices.insert(metadata.index_range());
                remove_quads.insert(metadata.quad_range());
            } else {
                let new_metadata = GpuChunkMetadata::new(
                    current_instance,
                    current_index..(current_index + metadata.indices()),
                    current_quad..(current_quad + metadata.quads()),
                );

                if inspect {
                    info!("Retaining chunk {chunk_pos} at LOD {:?} with updated metadata as part of removal: {new_metadata:#?}", self.lod);
                }

                // Plenty of sanity checks here to make sure that the start of a range is always smaller than the end.
                debug_assert!(metadata.start_index < metadata.end_index);
                debug_assert!(metadata.start_quad < metadata.end_quad);
                debug_assert!(new_metadata.start_index < new_metadata.end_index);
                debug_assert!(new_metadata.start_quad < new_metadata.end_quad);

                chunks_to_retain.insert(*chunk_pos, new_metadata);

                current_index += metadata.indices();
                current_quad += metadata.quads();
                current_instance += 1;
            }
        }

        self.raw.index.remove(gpu, queue, &remove_indices);
        self.raw.quad.remove(gpu, queue, &remove_quads);

        self.update_metadata(gpu, chunks_to_retain);
        self.update_bind_groups(gpu);
    }

    #[inline]
    pub fn num_chunks(&self) -> usize {
        self.metadata.len()
    }

    #[inline]
    pub fn get_chunk_metadata(&self, chunk: ChunkPos) -> Option<GpuChunkMetadata> {
        self.metadata.get(&chunk).cloned()
    }

    #[inline]
    pub fn get_chunk_metadata_index(&self, chunk: ChunkPos) -> Option<u32> {
        self.metadata.get_index_of(&chunk).map(|v| v as u32)
    }
}

/// Tests if a chunk map of a bunch of buffer bounds is correctly formatted.
pub(crate) fn chunk_bounds_correctly_formatted(bounds: &ChunkIndexMap<GpuChunkMetadata>) -> bool {
    // If there's less than 2 different bounds then it's not really possible to format them incorrectly.
    if bounds.len() < 2 {
        return true;
    }

    // The first chunk must be instance 0, and all its bounds must start at 0.
    let first = bounds.get_index(0).unwrap().1;
    let first_chunk_is_correct = 0 == first.index_range().start && 0 == first.quad_range().start;

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
            p_bounds.index_range().start < n_bounds.index_range().start
            && p_bounds.quad_range().start < n_bounds.quad_range().start
            // The chunks must share the data buffers contiguously.
            // i.e., the previous chunk's share must end where the next chunk's starts
            && p_bounds.index_range().end == n_bounds.index_range().start
            && p_bounds.quad_range().end == n_bounds.quad_range().start;

        if !is_correct {
            error!("Relationship between these 2 chunks violates chunk GPU metadata format rules!");
            dbg!(p_bounds);
            dbg!(n_bounds);

            return false;
        }
    }

    true
}
