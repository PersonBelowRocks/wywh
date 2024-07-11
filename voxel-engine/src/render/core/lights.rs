use bevy::{
    pbr::{LightEntity, MeshPipelineKey, Shadow, ShadowBinKey, ViewLightEntities},
    prelude::*,
    render::{
        render_phase::{BinnedRenderPhaseType, DrawFunctions, ViewBinnedRenderPhases},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
    },
};

use crate::topo::controller::{ChunkBatchLod, VisibleBatches};

use super::{
    commands::DrawDeferredBatch,
    gpu_chunk::IndirectRenderDataStore,
    gpu_registries::RegistryBindGroup,
    pipelines::{ChunkPipelineKey, DeferredIndirectChunkPipeline},
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

            // FIXME: this fails, dunno why
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
