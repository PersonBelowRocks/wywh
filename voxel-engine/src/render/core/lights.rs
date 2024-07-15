use bevy::{
    pbr::{
        LightEntity, LightMeta, MeshPipelineKey, Shadow, ShadowBinKey, ViewLightEntities,
        ViewLightsUniformOffset,
    },
    prelude::*,
    render::{
        render_phase::{BinnedRenderPhaseType, DrawFunctions, ViewBinnedRenderPhases},
        render_resource::{BindGroupEntries, PipelineCache, SpecializedRenderPipelines},
        renderer::RenderDevice,
    },
};

use crate::topo::controller::{ChunkBatch, ChunkBatchLod, VisibleBatches};

use super::{
    chunk_batches::{
        create_batch_count_buffer, create_batch_indirect_buffer, QueuedBatchBufBuildJobs,
    },
    commands::DrawDeferredBatch,
    gpu_chunk::IndirectRenderDataStore,
    gpu_registries::RegistryBindGroup,
    pipelines::{ChunkPipelineKey, DeferredIndirectChunkPipeline},
    views::{IndirectViewBatch, IndirectViewBatchCullData, ViewBatchBuffersStore},
    DefaultBindGroupLayouts,
};

#[derive(Component, Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct LightParent;

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
    mut last_parents: Local<usize>,
    mut cmds: Commands,
) {
    let mut insert = Vec::with_capacity(*last_size);
    let mut parents = Vec::with_capacity(*last_parents);

    for (entity, light) in &q_light_entities {
        let parent = get_parent_light(light);
        let Some(visible_batches) = q_visible_batches.get(parent).cloned().ok() else {
            continue;
        };

        insert.push((entity, visible_batches));
        parents.push((parent, LightParent));
    }

    *last_size = insert.len();
    *last_parents = parents.len();
    cmds.insert_or_spawn_batch(insert);
    cmds.insert_or_spawn_batch(parents);
}

pub fn initialize_and_queue_light_batch_buffers(
    mut populate_buffers: ResMut<QueuedBatchBufBuildJobs>,
    mut view_batch_buf_store: ResMut<ViewBatchBuffersStore>,
    store: Res<IndirectRenderDataStore>,
    light_meta: Res<LightMeta>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    q_views: Query<(&ViewLightEntities, &ViewLightsUniformOffset)>,
    q_light_entities: Query<&VisibleBatches, With<LightEntity>>,
    q_batches: Query<(&ChunkBatch, &ChunkBatchLod)>,
    gpu: Res<RenderDevice>,
) {
    let Some(view_light_uniforms_binding) = light_meta.view_gpu_lights.binding() else {
        warn!("Couldn't initialize and queue batch buffers and observer batch buffers because the view light uniforms binding wasn't present.");
        return;
    };

    for (view_light_entities, view_light_offset) in &q_views {
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

                let view_indirect_buf = create_batch_indirect_buffer(&gpu, batch.num_chunks());
                let view_count_buf = create_batch_count_buffer(&gpu);

                let cull_bind_group = gpu.create_bind_group(
                    Some("view_light_batch_frustum_cull_bind_group"),
                    &default_layouts.batch_cull_bind_group_layout,
                    &BindGroupEntries::sequential((
                        lod_data.buffers().instances.as_entire_binding(),
                        view_light_uniforms_binding.clone(),
                        view_indirect_buf.as_entire_binding(),
                        view_count_buf.as_entire_binding(),
                    )),
                );

                view_batch_buffers.insert(
                    batch_entity,
                    IndirectViewBatch {
                        num_chunks: batch.num_chunks(),
                        indirect: view_indirect_buf.clone(),
                        cull_data: Some(IndirectViewBatchCullData {
                            bind_group: cull_bind_group,
                            count: view_count_buf,
                            uniform_offset: view_light_offset.offset,
                        }),
                    },
                );

                populate_buffers.queue(view_indirect_buf, batch_lod, chunk_metadata_indices);
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
    pipeline: Res<DeferredIndirectChunkPipeline>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<DeferredIndirectChunkPipeline>>,
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
                ChunkPipelineKey { inner: light_key },
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
