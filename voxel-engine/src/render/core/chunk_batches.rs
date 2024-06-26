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
    render::{ChunkBatch, LevelOfDetail, VisibleBatches},
    util::ChunkSet,
};

use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    observers::{ObserverBatchBuffers, ObserverBatchBuffersStore},
    shaders::POPULATE_OBSERVER_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

#[derive(Resource, Clone, Default)]
pub struct RenderChunkBatches(EntityHashMap<RenderChunkBatch>);

impl RenderChunkBatches {
    pub fn insert(&mut self, entity: Entity, batch: &ChunkBatch) {
        let num_chunks = batch.chunks.len() as u32;

        self.0
            .entry(entity)
            .and_modify(|render_batch| {
                if render_batch.tick < batch.tick {
                    *render_batch = RenderChunkBatch::new(batch.tick, num_chunks);
                }
            })
            .or_insert_with(|| RenderChunkBatch::new(batch.tick, num_chunks));
    }

    pub fn drop_buffers(&mut self) {
        for batch in self.0.values_mut() {
            batch.buffers = None;
        }
    }

    pub fn get(&self, entity: Entity) -> Option<&RenderChunkBatch> {
        self.0.get(&entity)
    }

    pub fn set_buffers(&mut self, batch_entity: Entity, buffers: ChunkBatchBuffers) {
        self.0
            .get_mut(&batch_entity)
            .map(|batch| batch.buffers = Some(buffers));
    }

    pub fn contains(&self, batch_entity: Entity) -> bool {
        self.0.contains_key(&batch_entity)
    }

    pub fn has_buffers(&self, batch_entity: Entity) -> bool {
        self.get(batch_entity).is_some_and(|b| b.buffers.is_some())
    }
}

#[derive(Clone)]
pub struct ChunkBatchBuffers {
    pub indirect: Buffer,
    pub instance: Buffer,
}

#[derive(Clone)]
pub struct RenderChunkBatch {
    pub buffers: Option<ChunkBatchBuffers>,
    pub num_chunks: u32,
    pub tick: u64,
}

impl RenderChunkBatch {
    pub fn new(tick: u64, num_chunks: u32) -> Self {
        Self {
            tick,
            num_chunks,
            buffers: None,
        }
    }
}

#[derive(Resource, Clone)]
pub struct PopulateBatchBuffers {
    pub observers: EntityHashMap<EntityHashSet>,
    pub batches: EntityHashSet,
}

impl PopulateBatchBuffers {
    pub fn clear(&mut self) {
        self.observers.clear();
        self.batches.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty() && self.observers.is_empty()
    }

    pub fn queue(&mut self, batch: Entity, observer: Entity) {
        self.batches.insert(batch);

        self.observers
            .entry(observer)
            .and_modify(|visible| {
                visible.insert(batch);
            })
            .or_insert_with(|| EntityHashSet::from_iter([batch]));
    }
}

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
    mut cmds: Commands,
) {
    for (entity, batch) in &query {
        render_batches.insert(entity, batch);
        cmds.get_or_spawn(entity).insert(batch.clone());
    }
}

fn create_primary_indirect_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_batch_indirect_buffer"),
        size: (chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
        usage: BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}

fn create_observer_indirect_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("observer_batch_indirect_buffer"),
        size: (chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
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
        label: Some("observer_batch_chunk_count_buffer"),
        contents: &[0; 4],
        usage: BufferUsages::STORAGE,
    })
}

/// This system initializes the GPU buffers for chunk batches (instance buffer, indirect buffer, etc.) and queues
/// them for population by the buffer builder in the render graph.
pub fn initialize_and_queue_batch_buffers(
    mut populate_buffers: ResMut<PopulateBatchBuffers>,
    mut render_batches: ResMut<RenderChunkBatches>,
    mut all_observer_batch_buffers: ResMut<ObserverBatchBuffersStore>,
    observer_batch_query: Query<(Entity, &VisibleBatches)>,
    all_batches: Query<&ChunkBatch>,
    gpu: Res<RenderDevice>,
) {
    // Clear out the previous queued population buffers
    populate_buffers.clear();

    for (observer_entity, visible_batches) in &observer_batch_query {
        // All the per-observer batch buffers for this observer.
        let observer_batch_buffers = all_observer_batch_buffers
            .entry(observer_entity)
            .or_insert(EntityHashMap::default());

        for &batch_entity in visible_batches.iter() {
            // This batch is already initialized so we skip it
            if render_batches.has_buffers(batch_entity) {
                continue;
            }

            // We need to initialize the buffers at the appropriate size.
            let num_chunks = all_batches
                .get(batch_entity)
                .expect("Earlier in the extract phase we ensured that all visible batches are also actually present in the ECS world")
                .chunks.len() as u32;

            // Initialize the buffers here
            let buffers = ChunkBatchBuffers {
                indirect: create_primary_indirect_buffer(&gpu, num_chunks),
                instance: create_instance_buffer(&gpu, num_chunks),
            };

            // Set the empty buffers and queue this batch for buffer population
            render_batches.set_buffers(batch_entity, buffers);

            observer_batch_buffers.insert(
                batch_entity,
                ObserverBatchBuffers {
                    indirect: create_observer_indirect_buffer(&gpu, num_chunks),
                    count: create_count_buffer(&gpu),
                },
            );

            populate_buffers.queue(batch_entity, observer_entity);
        }
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
