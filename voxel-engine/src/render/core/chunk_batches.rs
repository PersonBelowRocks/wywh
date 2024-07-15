use bevy::{
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_resource::{
            BindGroup, BindGroupEntries, Buffer, BufferDescriptor, BufferInitDescriptor,
            BufferUsages, ShaderSize,
        },
        renderer::RenderDevice,
        view::{ViewUniformOffset, ViewUniforms},
        Extract,
    },
};
use bytemuck::cast_slice;

use crate::{
    render::lod::LevelOfDetail,
    topo::controller::{ChunkBatch, ChunkBatchLod, VisibleBatches},
};

use super::{
    gpu_chunk::IndirectRenderDataStore,
    indirect::{IndexedIndirectArgs, IndirectChunkData},
    views::{IndirectViewBatch, ViewBatchBuffersStore},
    BindGroupProvider,
};

impl ChunkBatch {
    /// Get the indices for the metadata of this observer's in-range chunks on the GPU as described by the provided indirect chunk data.
    /// If the metadata didn't exist in the provided indirect chunk data, then its index is not part of the returned vector.
    /// The caller must handle this (or do what this function does manually) if it's an issue.
    pub fn get_metadata_indices(&self, indirect_data: &IndirectChunkData) -> Vec<u32> {
        let mut chunk_metadata_indices = Vec::<u32>::with_capacity(self.chunks().len());

        for chunk_pos in self.chunks().iter() {
            let Some(metadata_index) = indirect_data.get_chunk_metadata_index(chunk_pos) else {
                continue;
            };

            chunk_metadata_indices.push(metadata_index);
        }

        chunk_metadata_indices
    }
}

pub fn create_batch_indirect_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("batch_indirect_buffer"),
        size: (chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub fn create_batch_count_buffer(gpu: &RenderDevice) -> Buffer {
    gpu.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("batch_count_buffer"),
        contents: &[0; 4],
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
    })
}

/// Extracts all entites with both a `ChunkBatch` and `ChunkBatchLod` component. These entities are
/// "renderable" chunk batches, they have all the data required to be rendered. Other batches are ignored.
pub fn extract_batches_with_lods(
    batches: Extract<Query<(Entity, &ChunkBatch, &ChunkBatchLod)>>,
    mut cmds: Commands,
    mut previous_size: Local<usize>,
) {
    let mut extract = Vec::with_capacity(*previous_size);

    for (entity, batch, batch_lod) in &batches {
        extract.push((entity, (batch.clone(), batch_lod.clone())));
    }

    *previous_size = extract.len();
    cmds.insert_or_spawn_batch(extract.into_iter());
}

/// This system initializes the GPU buffers for chunk batches (instance buffer, indirect buffer, etc.) and queues
/// them for population by the buffer builder in the render graph.
pub fn initialize_and_queue_batch_buffers(
    mut view_batch_buf_store: ResMut<ViewBatchBuffersStore>,
    store: Res<IndirectRenderDataStore>,
    view_uniforms: Res<ViewUniforms>,
    default_layouts: Res<BindGroupProvider>,
    q_views: Query<(Entity, &VisibleBatches, &ViewUniformOffset), With<ExtractedCamera>>,
    q_batches: Query<(&ChunkBatch, &ChunkBatchLod)>,
    gpu: Res<RenderDevice>,
) {
    // We want to avoid doing anything here if the view bindings aren't here (yet).
    let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
        warn!("Couldn't initialize and queue batch buffers and observer batch buffers because the view uniforms binding wasn't present.");
        return;
    };

    for (view_entity, visible_batches, view_offset) in &q_views {
        // All the per-observer batch buffers for this observer.
        let view_batch_buffers = view_batch_buf_store.get_or_insert(view_entity);

        for &batch_entity in visible_batches.iter() {
            // We need to initialize the buffers at the appropriate size.
            let (batch, batch_lod) = q_batches
                .get(batch_entity)
                .expect("Earlier in the extract phase we ensured that all visible batches are also actually present in the ECS world");

            let batch_lod = batch_lod.0;
            let lod_data = store.lod(batch_lod);

            // We can't have empty buffers in our bind group, so if the indirect data for this LOD
            // is empty we skip and get back to it later once it's ready.
            if lod_data.is_empty() {
                continue;
            }

            // This view already has buffers for this batch, so we don't need to build them.
            if view_batch_buffers.contains_key(&batch_entity) {
                continue;
            }

            // At this point we know that the LOD data is not empty, and that this view needs
            // buffers for this batch, so we'll (try to) initialize the buffers and queue the build job.
            let chunk_metadata_indices = batch.get_metadata_indices(&lod_data);

            // This batch didn't have any metadata for this LOD so we skip it.
            if chunk_metadata_indices.is_empty() {
                continue;
            }

            view_batch_buffers.insert(
                batch_entity,
                IndirectViewBatch {
                    num_chunks: batch.num_chunks(),
                    metadata_index_buffer: gpu.create_buffer_with_data(&BufferInitDescriptor {
                        label: Some("view_metadata_index_buffer"),
                        contents: cast_slice(&chunk_metadata_indices),
                        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    }),
                    indirect_buffer: create_batch_indirect_buffer(&gpu, batch.num_chunks()),
                    count_buffer: create_batch_count_buffer(&gpu),
                },
            );
        }
    }
}
