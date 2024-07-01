use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
    render::{
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayout, Buffer, BufferDescriptor,
            BufferInitDescriptor, BufferUsages, CachedComputePipelineId, ComputePipelineDescriptor,
            PipelineCache, ShaderSize, SpecializedComputePipeline, SpecializedComputePipelines,
        },
        renderer::RenderDevice,
        view::ViewUniforms,
        Extract,
    },
};
use bytemuck::cast_slice;

use crate::{
    topo::controller::{ChunkBatch, ChunkBatchLod, VisibleBatches},
    util::ChunkSet,
};

use super::{
    gpu_chunk::IndirectRenderDataStore,
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    observers::{ObserverBatchBuffersStore, ObserverBatchGpuData},
    shaders::{BUILD_BATCH_BUFFERS_HANDLE, OBSERVER_BATCH_FRUSTUM_CULL_HANDLE},
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

#[derive(Resource, Clone, Default)]
pub struct PreparedChunkBatches(EntityHashMap<PreparedChunkBatch>);

impl PreparedChunkBatches {
    /// Tries to insert the given batch entity and initialize its indirect buffer. Will only insert if the
    /// batch doesn't exist from before or if the existing batch is older (had a smaller `tick`).
    /// Returns true if the batch was inserted (in which case the batch should be queued for
    /// buffer building).
    pub fn try_insert(&mut self, entity: Entity, batch: &ChunkBatch, gpu: &RenderDevice) -> bool {
        let num_chunks = batch.num_chunks();
        let mut did_insert = false;

        self.0
            .entry(entity)
            .and_modify(|render_batch| {
                if render_batch.tick < batch.tick() {
                    *render_batch = PreparedChunkBatch {
                        indirect: create_primary_indirect_buffer(gpu, num_chunks),
                        num_chunks,
                        tick: batch.tick(),
                    };

                    did_insert = true;
                }
            })
            .or_insert_with(|| {
                did_insert = true;

                PreparedChunkBatch {
                    indirect: create_primary_indirect_buffer(gpu, num_chunks),
                    num_chunks,
                    tick: batch.tick(),
                }
            });

        did_insert
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn get(&self, entity: Entity) -> Option<&PreparedChunkBatch> {
        self.0.get(&entity)
    }

    pub fn contains(&self, batch_entity: Entity) -> bool {
        self.0.contains_key(&batch_entity)
    }
}

#[derive(Clone)]
pub struct PreparedChunkBatch {
    pub indirect: Buffer,
    pub num_chunks: u32,
    pub tick: u64,
}

#[derive(Resource, Clone)]
pub struct PopulateBatchBuffers {
    pub observers: EntityHashMap<EntityHashSet>,
    pub batches: EntityHashMap<BindGroup>,
}

impl PopulateBatchBuffers {
    pub fn clear(&mut self) {
        self.observers.clear();
        self.batches.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty() && self.observers.is_empty()
    }

    pub fn queue<F>(&mut self, batch: Entity, observer: Entity, bbb_bg_factory: F)
    where
        F: FnOnce() -> BindGroup,
    {
        // This dance here is to avoid cloning the bind group unless we really have to, since it's a somewhat expensive operation
        self.batches.entry(batch).or_insert_with(bbb_bg_factory);

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
    mut populate_buffers: ResMut<PopulateBatchBuffers>,
    mut render_batches: ResMut<PreparedChunkBatches>,
    mut all_observer_batch_buffers: ResMut<ObserverBatchBuffersStore>,
    store: Res<IndirectRenderDataStore>,
    view_uniforms: Res<ViewUniforms>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    q_observer_batches: Query<(Entity, &VisibleBatches)>,
    q_batches: Query<(&ChunkBatch, &ChunkBatchLod)>,
    gpu: Res<RenderDevice>,
) {
    // Clear out the previous queued population buffers
    populate_buffers.clear();

    // We want to avoid doing anything here if the view bindings aren't here (yet).
    let Some(view_uniforms_binding) = view_uniforms.uniforms.binding() else {
        warn!("Couldn't initialize and queue batch buffers and observer batch buffers because the view uniforms binding wasn't present.");
        return;
    };

    for (observer_entity, visible_batches) in &q_observer_batches {
        // All the per-observer batch buffers for this observer.
        let observer_batch_buffers = all_observer_batch_buffers
            .entry(observer_entity)
            .or_insert(EntityHashMap::default());

        for &batch_entity in visible_batches.iter() {
            // We need to initialize the buffers at the appropriate size.
            let (batch, batch_lod) = q_batches
                .get(batch_entity)
                .expect("Earlier in the extract phase we ensured that all visible batches are also actually present in the ECS world");

            let did_insert = render_batches.try_insert(batch_entity, batch, &gpu);
            let batch_lod = batch_lod.0;

            // If we inserted this batch entity for this view, then we need to build 2 bind groups:
            // The frustum cull bind bind group, which binds the indirect buffers (count, args),
            // chunk instances (basically their position), and the view (for the frustum). This
            // bind group is used in a compute shader to edit the indirect args so that the
            // chunks that are outside the frustum are not rendered.
            //
            // The buffer building bind group, which is basically a glorified copy. It takes in an
            // array of the indices to chunk metadata in the metadata array buffer, and builds indirect args
            // based on what it finds.
            if did_insert {
                let observer_indirect_buf =
                    create_observer_indirect_buffer(&gpu, batch.num_chunks());
                let observer_count_buf = create_count_buffer(&gpu);

                let lod_data = &store.lod(batch_lod);

                let cull_bind_group = gpu.create_bind_group(
                    Some("observer_batch_frustum_cull_bind_group"),
                    &default_layouts.observer_batch_cull_layout,
                    &BindGroupEntries::sequential((
                        lod_data.buffers().instances.as_entire_binding(),
                        view_uniforms_binding.clone(),
                        observer_indirect_buf.as_entire_binding(),
                        observer_count_buf.as_entire_binding(),
                    )),
                );

                observer_batch_buffers.insert(
                    batch_entity,
                    ObserverBatchGpuData {
                        cull_bind_group,
                        indirect: observer_indirect_buf,
                        count: observer_count_buf,
                        num_chunks: batch.num_chunks(),
                    },
                );

                let factory = || {
                    // An array of the indices to the chunk metadata on the GPU.
                    let chunk_metadata_indices = batch.get_metadata_indices(&lod_data);
                    let metadata_index_buffer =
                        gpu.create_buffer_with_data(&BufferInitDescriptor {
                            label: Some("BBB_chunk_metadata_indices_buffer"),
                            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                            contents: cast_slice(&chunk_metadata_indices),
                        });

                    let metadata_buffer = &lod_data.buffers().metadata;
                    let render_batch_indirect_buf = &render_batches
                        .get(batch_entity)
                        .expect("We just inserted the render batch for this entity")
                        .indirect;

                    // Build bind group
                    gpu.create_bind_group(
                        Some("BBB_bind_group"),
                        &default_layouts.build_batch_buffers_layout,
                        &BindGroupEntries::sequential((
                            metadata_buffer.as_entire_binding(),
                            metadata_index_buffer.as_entire_binding(),
                            render_batch_indirect_buf.as_entire_binding(),
                        )),
                    )
                };

                populate_buffers.queue(batch_entity, observer_entity, factory);
            }
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct BuildBatchBuffersPipelineId(pub CachedComputePipelineId);

#[derive(Resource)]
pub struct BuildBatchBuffersPipeline {
    pub shader: Handle<Shader>,
    pub bg_layout: BindGroupLayout,
}

impl FromWorld for BuildBatchBuffersPipeline {
    fn from_world(world: &mut World) -> Self {
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            shader: BUILD_BATCH_BUFFERS_HANDLE,
            bg_layout: default_layouts.build_batch_buffers_layout.clone(),
        }
    }
}

impl SpecializedComputePipeline for BuildBatchBuffersPipeline {
    type Key = ();

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let mut shader_defs = vec![];
        add_shader_constants(&mut shader_defs);

        ComputePipelineDescriptor {
            label: Some("build_batch_buffers_pipeline".into()),
            entry_point: "build_buffers".into(),
            shader: self.shader.clone(),
            push_constant_ranges: vec![],
            shader_defs,
            layout: vec![self.bg_layout.clone()],
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ObserverBatchFrustumCullPipelineId(pub CachedComputePipelineId);

#[derive(Resource)]
pub struct ObserverBatchFrustumCullPipeline {
    pub shader: Handle<Shader>,
    pub bg_layout: BindGroupLayout,
}

impl FromWorld for ObserverBatchFrustumCullPipeline {
    fn from_world(world: &mut World) -> Self {
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            shader: OBSERVER_BATCH_FRUSTUM_CULL_HANDLE,
            bg_layout: default_layouts.observer_batch_cull_layout.clone(),
        }
    }
}

impl SpecializedComputePipeline for ObserverBatchFrustumCullPipeline {
    type Key = ();

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let mut shader_defs = vec![];
        add_shader_constants(&mut shader_defs);

        ComputePipelineDescriptor {
            label: Some("observer_batch_frustum_cull_pipeline".into()),
            entry_point: "batch_frustum_cull".into(),
            shader: self.shader.clone(),
            shader_defs,
            layout: vec![self.bg_layout.clone()],
            push_constant_ranges: vec![],
        }
    }
}

pub fn create_pipelines(
    cache: Res<PipelineCache>,
    buffer_build: Res<BuildBatchBuffersPipeline>,
    batch_cull: Res<ObserverBatchFrustumCullPipeline>,
    mut buffer_builder_pipelines: ResMut<SpecializedComputePipelines<BuildBatchBuffersPipeline>>,
    mut cull_observer_batch_pipelines: ResMut<
        SpecializedComputePipelines<ObserverBatchFrustumCullPipeline>,
    >,
    mut cmds: Commands,
) {
    let id = buffer_builder_pipelines.specialize(&cache, &buffer_build, ());
    cmds.insert_resource(BuildBatchBuffersPipelineId(id));
    let id = cull_observer_batch_pipelines.specialize(&cache, &batch_cull, ());
    cmds.insert_resource(ObserverBatchFrustumCullPipelineId(id));
}
