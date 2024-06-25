use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
    render::{
        render_resource::{
            BindGroupLayout, Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages,
            CachedComputePipelineId, ComputePipelineDescriptor, PipelineCache, ShaderSize,
            SpecializedComputePipeline, SpecializedComputePipelines,
        },
        renderer::RenderDevice,
        Extract,
    },
};

use crate::{
    render::{ChunkBatch, LevelOfDetail},
    util::ChunkSet,
};

use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    shaders::POPULATE_OBSERVER_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

#[derive(Resource, Clone, Default)]
pub struct RenderChunkBatches(EntityHashMap<RenderChunkBatch>);

impl RenderChunkBatches {
    pub fn insert(&mut self, entity: Entity, batch: &ChunkBatch) {
        self.0
            .entry(entity)
            .and_modify(|render_batch| {
                if render_batch.tick < batch.tick {
                    *render_batch = RenderChunkBatch::from_tick(batch.tick);
                }
            })
            .or_insert_with(|| RenderChunkBatch::from_tick(batch.tick));
    }

    pub fn drop_buffers(&mut self) {
        for batch in self.0.values_mut() {
            batch.buffers = None;
        }
    }

    pub fn get(&self, entity: Entity) -> Option<&RenderChunkBatch> {
        self.0.get(&entity)
    }
}

#[derive(Clone)]
pub struct ChunkBatchBuffers {
    pub indirect: Buffer,
    pub instance: Buffer,
    pub count: Buffer,
}

#[derive(Clone)]
pub struct RenderChunkBatch {
    pub buffers: Option<ChunkBatchBuffers>,
    pub tick: u64,
}

impl RenderChunkBatch {
    pub fn from_tick(tick: u64) -> Self {
        Self {
            tick,
            buffers: None,
        }
    }
}

#[derive(Resource, Clone, Deref, DerefMut)]
pub struct PopulateBatchBuffers(EntityHashSet);

impl ChunkBatch {
    /// Get the indices for the metadata of this observer's in-range chunks on the GPU as described by the provided indirect chunk data.
    /// If the metadata didn't exist in the provided indirect chunk data, then its index is not part of the returned vector.
    /// The caller must handle this (or do what this function does manually) if it's an issue.
    pub fn get_metadata_indices(&self, indirect_data: &IndirectChunkData) -> Vec<u32> {
        let mut chunk_metadata_indices = Vec::<u32>::with_capacity(self.chunks.len());

        for chunk_pos in self.chunks.iter() {
            let Some(metadata_index) = indirect_data.get_chunk_metadata_index(chunk_pos) else {
                continue;
            };

            chunk_metadata_indices.push(metadata_index);
        }

        chunk_metadata_indices
    }
}

// TODO: handle chunk batch removals
pub fn extract_chunk_batches(
    query: Extract<Query<(Entity, &ChunkBatch)>>,
    mut render_batches: ResMut<RenderChunkBatches>,
) {
    for (entity, batch) in &query {
        render_batches.insert(entity, batch);
    }
}

fn create_indirect_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_batch_indirect_buffer"),
        size: (chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT,
        mapped_at_creation: false,
    })
}

fn create_instance_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_batch_instance_buffer"),
        size: (chunks as u64) * u64::from(ChunkInstanceData::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

fn create_count_buffer(gpu: &RenderDevice) -> Buffer {
    gpu.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("chunk_batch_num_chunks"),
        contents: &[0; 4],
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT,
    })
}

/// This system initializes the GPU buffers for chunk batches (instance buffer, indirect buffer, etc.) and queues
/// them for population by the buffer builder in the render graph.
pub fn initialize_and_queue_batch_buffers(
    mut populate_buffers: ResMut<PopulateBatchBuffers>,
    mut render_batches: ResMut<RenderChunkBatches>,
    batch_query: Query<&ChunkBatch>,
    gpu: Res<RenderDevice>,
) {
    // Clear out the previous queued population buffers
    populate_buffers.clear();

    for (&entity, batch) in render_batches.0.iter_mut() {
        // This batch is already initialized so we skip it
        if batch.buffers.is_some() {
            continue;
        }

        // We need to initialize the buffers at the appropriate size.
        let num_chunks = batch_query.get(entity).unwrap().chunks.len() as u32;

        // Initialize the buffers here
        let buffers = ChunkBatchBuffers {
            indirect: create_indirect_buffer(&gpu, num_chunks),
            instance: create_instance_buffer(&gpu, num_chunks),
            count: create_count_buffer(&gpu),
        };

        batch.buffers = Some(buffers);

        // Queue this batch for buffer population
        populate_buffers.insert(entity);
    }
}

#[derive(Resource, Clone, Debug)]
pub struct PopulateBatchBuffersPipelineId(pub CachedComputePipelineId);

#[derive(Resource)]
pub struct PopulateBatchBuffersPipeline {
    pub shader: Handle<Shader>,
    pub input_layout: BindGroupLayout,
    pub output_layout: BindGroupLayout,
}

impl FromWorld for PopulateBatchBuffersPipeline {
    fn from_world(world: &mut World) -> Self {
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            shader: POPULATE_OBSERVER_BUFFERS_HANDLE,
            input_layout: default_layouts.observer_buffers_input_layout.clone(),
            output_layout: default_layouts.observer_buffers_output_layout.clone(),
        }
    }
}

impl SpecializedComputePipeline for PopulateBatchBuffersPipeline {
    type Key = ();

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let mut shader_defs = vec![];
        add_shader_constants(&mut shader_defs);

        ComputePipelineDescriptor {
            label: Some("populate_observer_buffers_pipeline".into()),
            entry_point: "populate_buffers".into(),
            shader: self.shader.clone(),
            push_constant_ranges: vec![],
            shader_defs,
            layout: vec![self.input_layout.clone(), self.output_layout.clone()],
        }
    }
}

pub fn create_buffer_population_pipeline(
    cache: Res<PipelineCache>,
    pipeline: Res<PopulateBatchBuffersPipeline>,
    mut pipelines: SpecializedComputePipelines<PopulateBatchBuffersPipeline>,
    mut cmds: Commands,
) {
    let id = pipelines.specialize(&cache, &pipeline, ());
    cmds.insert_resource(PopulateBatchBuffersPipelineId(id));
}
