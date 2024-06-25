use bevy::render::render_resource::{CachedComputePipelineId, CachedPipelineState, Pipeline};
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
use std::sync::atomic::{AtomicBool, Ordering};

use crate::render::{LODs, LevelOfDetail, LodMap};
use crate::topo::controller::ObserverId;
use crate::topo::{controller::RenderableObserverChunks, world::ChunkPos};
use crate::util::ChunkSet;

use super::gpu_chunk::IndirectRenderDataStore;
use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    shaders::POPULATE_OBSERVER_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

#[derive(Resource, Deref, DerefMut, Default)]
pub struct RenderWorldObservers(
    hb::HashMap<ObserverId, LodMap<Option<ExtractedChunkBatch>>, rustc_hash::FxBuildHasher>,
);

impl RenderWorldObservers {
    /// Insert a batch of chunks into the given observer at the given LOD. Will initialize the observer
    /// if it doesn't already exist.
    pub fn insert_batch(&mut self, observer: ObserverId, lod: LevelOfDetail, chunks: &ChunkSet) {
        self.0
            .entry(observer)
            .and_modify(|batches| match &mut batches[lod] {
                Some(ref mut batch) => batch.chunks = chunks.clone(),
                None => {
                    batches[lod] = Some(ExtractedChunkBatch::new(chunks.clone()));
                }
            })
            .or_insert_with(|| {
                let mut new = LodMap::<Option<ExtractedChunkBatch>>::default();
                new[lod] = Some(ExtractedChunkBatch::new(chunks.clone()));
                new
            });
    }

    /// Drop the GPU buffers associated with the observers.
    pub fn drop_buffers(&mut self) {
        for batches in self.0.values_mut() {
            for (_, batch) in batches.iter_mut() {
                let Some(batch) = batch else { continue };
                batch.buffers = None;
            }
        }
    }
}

#[derive(Component, Default)]
pub struct ExtractedChunkBatch {
    pub chunks: ChunkSet,
    pub buffers: Option<ObserverBuffers>,
}

pub struct ObserverBuffers {
    pub indirect: Buffer,
    pub instance: Buffer,
    pub count: Buffer,
    pub ready: AtomicBool,
}

impl ExtractedChunkBatch {
    pub fn new(chunks: ChunkSet) -> Self {
        Self {
            chunks: chunks,
            buffers: None,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.buffers
            .as_ref()
            .is_some_and(|buffers| buffers.ready.load(Ordering::Relaxed))
    }

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

// TODO: update removed observer entities in the render world too.
pub fn extract_observer_chunks(
    observers: Extract<Query<(Entity, &ObserverId, &RenderableObserverChunks)>>,
    mut existing: ResMut<RenderWorldObservers>,
    mut cmds: Commands,
) {
    for (entity, &id, ob_chunks) in &observers {
        let lods = LODs::from_map(&ob_chunks.in_range);

        cmds.get_or_spawn(entity)
            .insert(id)
            .with_children(|builder| {
                for lod in lods.contained_lods() {
                    builder.spawn(ChunkBatch { lod });
                }
            });

        if !ob_chunks.should_extract.load(Ordering::Relaxed) {
            continue;
        }

        for (lod, chunks) in ob_chunks.in_range() {
            existing.insert_batch(id, lod, chunks);
        }

        ob_chunks.should_extract.store(false, Ordering::Relaxed);
    }
}

#[derive(Component, Copy, Clone)]
pub struct ChunkBatch {
    pub lod: LevelOfDetail,
}

#[derive(Resource, Clone, Debug)]
pub struct PopulateObserverBuffersPipelineId(pub CachedComputePipelineId);

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

/// TODO: move to node in render graph
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
    todo!()
}
