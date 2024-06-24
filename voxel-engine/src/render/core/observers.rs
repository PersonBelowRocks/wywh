use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::query::QueryEntityError;
use bevy::render::render_resource::{CachedPipelineState, Pipeline};
use bevy::{
    prelude::*,
    render::{
        render_resource::{
            BindGroupEntries, BindGroupLayout, Buffer, BufferDescriptor, BufferInitDescriptor,
            BufferUsages, CommandEncoderDescriptor, ComputePassDescriptor,
            ComputePipelineDescriptor, PipelineCache, ShaderSize, SpecializedComputePipeline,
            SpecializedComputePipelines,
        },
        renderer::{RenderDevice, RenderQueue},
        Extract,
    },
};
use bytemuck::cast_slice;
use itertools::Itertools;
use std::sync::atomic::Ordering;

use crate::topo::controller::ObserverId;
use crate::topo::{controller::RenderableObserverChunks, world::ChunkPos};

use super::gpu_chunk::IndirectRenderDataStore;
use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    shaders::POPULATE_OBSERVER_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

#[derive(Resource, Deref, DerefMut, Default)]
pub struct RenderWorldObservers(
    hb::HashMap<ObserverId, ExtractedObserverData, rustc_hash::FxBuildHasher>,
);

#[derive(Component, Clone, Default)]
pub struct ExtractedObserverData {
    pub in_range: Vec<ChunkPos>,
    pub buffers: Option<ObserverBuffers>,
}

#[derive(Clone)]
pub struct ObserverBuffers {
    pub indirect: Buffer,
    pub instance: Buffer,
    pub count: Buffer,
}

impl ExtractedObserverData {
    pub fn new(chunks: Vec<ChunkPos>) -> Self {
        Self {
            in_range: chunks,
            buffers: None,
        }
    }

    /// Get the indices for the metadata of this observer's in-range chunks on the GPU as described by the provided indirect chunk data.
    /// If the metadata didn't exist in the provided indirect chunk data, then its index is not part of the returned vector.
    /// The caller must handle this (or do what this function does manually) if it's an issue.
    pub fn get_metadata_indices(&self, indirect_data: &IndirectChunkData) -> Vec<u32> {
        let mut chunk_metadata_indices = Vec::<u32>::with_capacity(self.in_range.len());

        for &chunk_pos in self.in_range.iter() {
            let Some(metadata_index) = indirect_data.get_chunk_metadata_index(chunk_pos) else {
                continue;
            };

            chunk_metadata_indices.push(metadata_index);
        }

        chunk_metadata_indices
    }
}

// TODO: update removed observer entities in the render world too.
pub fn extract_observer_chunks(
    observers: Extract<Query<(&ObserverId, &RenderableObserverChunks)>>,
    mut existing: ResMut<RenderWorldObservers>,
) {
    for (&id, ob_chunks) in &observers {
        if !ob_chunks.should_extract.load(Ordering::Relaxed) {
            continue;
        }

        existing
            .entry(id)
            .and_modify(|data| {
                data.in_range.clear();
                data.in_range.extend(ob_chunks.in_range.iter());
            })
            .or_insert_with(|| ExtractedObserverData::new(ob_chunks.in_range.iter().collect_vec()));

        ob_chunks.should_extract.store(false, Ordering::Relaxed);
    }
}

#[derive(Resource)]
pub struct PopulateObserverBuffersPipeline {
    pub shader: Handle<Shader>,
    pub input_layout: BindGroupLayout,
    pub output_layout: BindGroupLayout,
}

impl FromWorld for PopulateObserverBuffersPipeline {
    fn from_world(world: &mut World) -> Self {
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            shader: POPULATE_OBSERVER_BUFFERS_HANDLE,
            input_layout: default_layouts.observer_buffers_input_layout.clone(),
            output_layout: default_layouts.observer_buffers_output_layout.clone(),
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub struct PopulateObserverBuffersPipelineKey;

impl SpecializedComputePipeline for PopulateObserverBuffersPipeline {
    type Key = PopulateObserverBuffersPipelineKey;

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

fn create_indirect_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_observer_indirect_buffer"),
        size: (chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT,
        mapped_at_creation: false,
    })
}

fn create_instance_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_observer_instance_buffer"),
        size: (chunks as u64) * u64::from(ChunkInstanceData::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

fn create_count_buffer(gpu: &RenderDevice) -> Buffer {
    gpu.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("chunk_observer_num_chunks"),
        contents: &[0; 4],
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT,
    })
}

pub fn populate_observer_multi_draw_buffers(
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    indirect_data: Res<IndirectRenderDataStore>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut pipelines: ResMut<SpecializedComputePipelines<PopulateObserverBuffersPipeline>>,
    pipeline: Res<PopulateObserverBuffersPipeline>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    mut observers: ResMut<RenderWorldObservers>,
) {
    // If there's no chunks present, then skip the rest of the logic here and return early since there's nothing we can really do
    if indirect_data.chunks.num_chunks() == 0 {
        return;
    }

    let pipeline_id = pipelines.specialize(
        &pipeline_cache,
        &pipeline,
        PopulateObserverBuffersPipelineKey,
    );

    // TODO: we shouldn't do this every time this system is run, instead we should create the pipeline once and store the ID once we know its valid
    pipeline_cache.process_queue();

    let compute_pipeline = match pipeline_cache.get_compute_pipeline_state(pipeline_id) {
        CachedPipelineState::Ok(Pipeline::ComputePipeline(pipeline)) => pipeline,
        CachedPipelineState::Err(error) => {
            error!("Error creating observer buffer population pipeline: {error}");
            panic!();
        }
        CachedPipelineState::Queued | CachedPipelineState::Creating(_) => return,
        _ => unreachable!(),
    };

    for ob_chunks in observers.values_mut() {
        // Skip observer data with no in-range chunks to avoid making 0 length buffers
        if ob_chunks.in_range.is_empty() {
            continue;
        }

        let num_chunks = ob_chunks.in_range.len();

        let chunk_metadata_indices = ob_chunks.get_metadata_indices(&indirect_data.chunks);

        let metadata_index_buffer = gpu.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("observer_chunks_metadata_indices_buffer"),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            contents: cast_slice(&chunk_metadata_indices),
        });

        let metadata_buffer = &indirect_data.chunks.buffers().metadata;

        let instance_buffer = create_instance_buffer(&gpu, num_chunks as _);
        let indirect_buffer = create_indirect_buffer(&gpu, num_chunks as _);
        let count_buffer = create_count_buffer(&gpu);

        // Build bind groups
        let input_bg = gpu.create_bind_group(
            Some("observer_population_input_bind_group"),
            &default_layouts.observer_buffers_input_layout,
            &BindGroupEntries::sequential((
                metadata_buffer.as_entire_binding(),
                metadata_index_buffer.as_entire_binding(),
            )),
        );

        let output_bg = gpu.create_bind_group(
            Some("observer_population_output_bind_group"),
            &default_layouts.observer_buffers_output_layout,
            &BindGroupEntries::sequential((
                instance_buffer.as_entire_binding(),
                indirect_buffer.as_entire_binding(),
                count_buffer.as_entire_binding(),
            )),
        );

        // Encode compute pass
        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("populate_observer_buffers_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("populate_observer_buffers_compute_pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&compute_pipeline);

            pass.set_bind_group(0, &input_bg, &[]);
            pass.set_bind_group(1, &output_bg, &[]);

            let num_chunks = ob_chunks.in_range.len() as u32;
            pass.dispatch_workgroups(1, 1, num_chunks);
        }

        queue.submit([encoder.finish()]);

        ob_chunks.buffers = Some(ObserverBuffers {
            indirect: indirect_buffer,
            instance: instance_buffer,
            count: count_buffer,
        });
    }
}
