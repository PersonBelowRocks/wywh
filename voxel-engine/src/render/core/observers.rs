use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::render_resource::{
    BindGroup, CachedComputePipelineId, CachedPipelineState, Pipeline,
};
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

use crate::render::{ChunkBatch, LODs, LevelOfDetail, LodMap, VisibleBatches};
use crate::topo::{controller::RenderableObserverChunks, world::ChunkPos};
use crate::util::ChunkSet;

use super::gpu_chunk::IndirectRenderDataStore;
use super::phase::{PrepassChunkPhaseItem, RenderChunkPhaseItem};
use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    shaders::BUILD_BATCH_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

/// Copies of the indirect, instance, and count buffers for each observer so they can cull individually.
// TODO: clear this out whenever theres newer data afoot
#[derive(Resource, Clone, Default, Deref, DerefMut)]
pub struct ObserverBatchBuffersStore(EntityHashMap<ObserverBatches>);

impl ObserverBatchBuffersStore {
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone)]
pub struct ObserverBatchGpuData {
    pub bind_group: Option<BindGroup>,
    pub num_chunks: u32,
    pub indirect: Buffer,
    pub count: Buffer,
}

pub type ObserverBatches = EntityHashMap<ObserverBatchGpuData>;

pub fn extract_observer_visible_batches(
    query: Extract<Query<(Entity, &VisibleBatches)>>,
    batch_query: Query<&ChunkBatch>,
    mut cmds: Commands,
) {
    for (entity, visible) in &query {
        let visible = visible
            .iter()
            .filter(|&entity| batch_query.contains(*entity))
            .cloned()
            .collect::<EntityHashSet>();

        cmds.get_or_spawn(entity)
            .insert(VisibleBatches::new(visible));
    }
}

/// Sets up chunk render phases for camera entities
pub fn extract_chunk_camera_phases(
    cameras: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
    mut prepass_phases: ResMut<ViewSortedRenderPhases<PrepassChunkPhaseItem>>,
    mut render_phases: ResMut<ViewSortedRenderPhases<RenderChunkPhaseItem>>,
    mut live_entities: Local<EntityHashSet>,
) {
    live_entities.clear();

    for (entity, camera) in &cameras {
        if !camera.is_active {
            continue;
        }

        prepass_phases.insert_or_clear(entity);
        render_phases.insert_or_clear(entity);

        live_entities.insert(entity);
    }

    prepass_phases.retain(|entity, _| live_entities.contains(entity));
    render_phases.retain(|entity, _| live_entities.contains(entity));
}
