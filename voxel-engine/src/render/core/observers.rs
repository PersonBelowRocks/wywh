use bevy::ecs::entity::EntityHashSet;
use bevy::render::render_phase::ViewSortedRenderPhases;
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
use crate::topo::{controller::RenderableObserverChunks, world::ChunkPos};
use crate::util::ChunkSet;

use super::gpu_chunk::IndirectRenderDataStore;
use super::phase::{PrepassChunkPhaseItem, RenderChunkPhaseItem};
use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    shaders::POPULATE_OBSERVER_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

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
