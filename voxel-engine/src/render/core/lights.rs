use bevy::{
    pbr::{LightEntity, MeshPipelineKey, Shadow, ShadowBinKey, ViewLightEntities},
    prelude::*,
    render::{
        render_phase::{BinnedRenderPhaseType, DrawFunctions, ViewBinnedRenderPhases},
        render_resource::{
            BufferInitDescriptor, BufferUsages, PipelineCache, SpecializedRenderPipelines,
        },
        renderer::RenderDevice,
    },
};
use bytemuck::cast_slice;

use crate::topo::controller::{ChunkBatch, ChunkBatchLod, VisibleBatches};

use super::{
    chunk_batches::{create_batch_count_buffer, create_batch_indirect_buffer},
    commands::DrawDeferredBatch,
    gpu_chunk::IndirectRenderDataStore,
    gpu_registries::RegistryBindGroup,
    pipelines::{ChunkPipelineKey, ChunkRenderPipeline},
    views::{IndirectViewBatch, ViewBatchBuffersStore},
};

pub fn get_parent_light(light: &LightEntity) -> Entity {
    match light {
        LightEntity::Spot { light_entity } => *light_entity,
        LightEntity::Directional {
            light_entity,
            cascade_index: _,
        } => *light_entity,
        LightEntity::Point {
            light_entity,
            face_index: _,
        } => *light_entity,
    }
}

pub fn debug_light_entity(light: &LightEntity) -> String {
    match light {
        LightEntity::Spot { light_entity } => format!("spot light: {light_entity}"),
        LightEntity::Directional {
            light_entity,
            cascade_index,
        } => format!("directional light: {light_entity} cascade_index: {cascade_index}"),
        LightEntity::Point {
            light_entity,
            face_index,
        } => format!("directional light: {light_entity} face_index: {face_index}"),
    }
}

pub fn inherit_parent_light_batches(
    q_light_entities: Query<(Entity, &LightEntity)>,
    q_visible_batches: Query<&VisibleBatches>,
    mut last_size: Local<usize>,
    mut cmds: Commands,
) {
    let mut insert = Vec::with_capacity(*last_size);

    for (entity, light) in &q_light_entities {
        let parent = get_parent_light(light);
        let Some(visible_batches) = q_visible_batches.get(parent).cloned().ok() else {
            continue;
        };

        insert.push((entity, visible_batches));
    }

    *last_size = insert.len();
    cmds.insert_or_spawn_batch(insert);
}

pub fn initialize_and_queue_light_batch_buffers(
    mut view_batch_buf_store: ResMut<ViewBatchBuffersStore>,
    store: Res<IndirectRenderDataStore>,
    q_views: Query<&ViewLightEntities>,
    q_light_entities: Query<&VisibleBatches, With<LightEntity>>,
    q_batches: Query<(&ChunkBatch, &ChunkBatchLod)>,
    gpu: Res<RenderDevice>,
) {
    for view_light_entities in &q_views {
        for &view_light_entity in &view_light_entities.lights {
            let Some(light_visible_batches) = q_light_entities.get(view_light_entity).ok() else {
                continue;
            };

            let view_batch_buffers = view_batch_buf_store.get_or_insert(view_light_entity);

            for &batch_entity in light_visible_batches.iter() {
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

                // This observer already has buffers for this batch, so we don't need to build them.
                if view_batch_buffers.contains_key(&batch_entity) {
                    continue;
                }

                // At this point we know that the LOD data is not empty, and that this observer needs
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
}

/// Queue shadows for chunks.
pub fn queue_chunk_shadows(
    //////////////////////////////////////////////////////////////////////////
    q_batches: Query<&ChunkBatchLod>,
    q_view_lights: Query<(Entity, &ViewLightEntities)>,
    mut q_view_light_entities: Query<(&LightEntity, &VisibleBatches)>,
    mut phases: ResMut<ViewBinnedRenderPhases<Shadow>>,

    //////////////////////////////////////////////////////////////////////////
    functions: Res<DrawFunctions<Shadow>>,
    pipeline: Res<ChunkRenderPipeline>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<ChunkRenderPipeline>>,
    //////////////////////////////////////////////////////////////////////////
    registry_bg: Option<Res<RegistryBindGroup>>,
    mesh_data: Res<IndirectRenderDataStore>,
) {
    // While we don't use the registry bind group in this system, we do use it in our draw function.
    // It's also possible for this system to run before the registry bind group is prepared, leading to
    // an error down the line in the draw function. To avoid this we only queue our indirect chunks if the
    // registry bind group is prepared.
    if registry_bg.is_none() {
        return;
    }

    let draw_shadow = functions.read().id::<DrawDeferredBatch>();

    for (_entity, view_lights) in &q_view_lights {
        for &view_light_entity in view_lights.lights.iter() {
            let Some(phase) = phases.get_mut(&view_light_entity) else {
                continue;
            };

            let Some((light, visible_batches)) =
                q_view_light_entities.get_mut(view_light_entity).ok()
            else {
                continue;
            };

            let is_directional_light = matches!(light, LightEntity::Directional { .. });

            let mut light_key = MeshPipelineKey::DEPTH_PREPASS;
            light_key.set(MeshPipelineKey::DEPTH_CLAMP_ORTHO, is_directional_light);

            let pipeline_id = pipelines.specialize(
                &pipeline_cache,
                &pipeline,
                ChunkPipelineKey {
                    inner: light_key,
                    shadow_pass: true,
                },
            );

            for &batch_entity in visible_batches.iter() {
                let lod = q_batches.get(batch_entity).unwrap().0;

                if !mesh_data.lod(lod).is_ready() {
                    continue;
                }

                phase.add(
                    ShadowBinKey {
                        draw_function: draw_shadow,
                        pipeline: pipeline_id,
                        asset_id: AssetId::<Mesh>::default().untyped(),
                    },
                    batch_entity,
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
    }
}
