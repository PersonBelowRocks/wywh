use bevy::{
    ecs::query::QueryEntityError,
    pbr::{
        CascadesVisibleEntities, CubemapVisibleEntities, ExtractedDirectionalLight,
        ExtractedPointLight, LightEntity, MeshPipelineKey, RenderMeshInstances, Shadow,
        ViewLightEntities,
    },
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_asset::RenderAssets,
        render_phase::{DrawFunctions, RenderPhase},
        render_resource::{
            PipelineCache, SpecializedMeshPipelines, SpecializedRenderPipeline,
            SpecializedRenderPipelines,
        },
        view::VisibleEntities,
    },
};

use crate::topo::world::{ChunkEntity, ChunkPos};

use super::{
    gpu_chunk::{ChunkRenderData, ChunkRenderDataStore},
    prepass::{ChunkPrepassPipeline, DrawVoxelChunkPrepass},
    render::ChunkPipelineKey,
    utils::{iter_visible_chunks, ChunkDataParams},
};

// largely taken from
// https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/render/light.rs#L1590
pub fn queue_shadows(
    shadow_draw_functions: Res<DrawFunctions<Shadow>>,
    prepass_pipeline: Res<ChunkPrepassPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<ChunkPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    view_lights: Query<(Entity, &ViewLightEntities)>,
    mut view_light_shadow_phases: Query<(&LightEntity, &mut RenderPhase<Shadow>)>,
    point_light_entities: Query<&CubemapVisibleEntities, With<ExtractedPointLight>>,
    directional_light_entities: Query<&CascadesVisibleEntities, With<ExtractedDirectionalLight>>,
    spot_light_entities: Query<&VisibleEntities, With<ExtractedPointLight>>,
    chunks: ChunkDataParams,
) {
    for (entity, view_lights) in &view_lights {
        let shadow_function = shadow_draw_functions.read().id::<DrawVoxelChunkPrepass>();
        for view_light_entity in view_lights.lights.iter().copied() {
            let (light_entity, mut phase) =
                view_light_shadow_phases.get_mut(view_light_entity).unwrap();

            let is_directional_light = matches!(light_entity, LightEntity::Directional { .. });
            let visible_entities = match light_entity {
                LightEntity::Directional {
                    light_entity,
                    cascade_index,
                } => directional_light_entities
                    .get(*light_entity)
                    .expect("Failed to get directional light visible entities")
                    .entities
                    .get(&entity)
                    .expect("Failed to get directional light visible entities for view")
                    .get(*cascade_index)
                    .expect("Failed to get directional light visible entities for cascade"),
                LightEntity::Point {
                    light_entity,
                    face_index,
                } => point_light_entities
                    .get(*light_entity)
                    .expect("Failed to get point light visible entities")
                    .get(*face_index),
                LightEntity::Spot { light_entity } => spot_light_entities
                    .get(*light_entity)
                    .expect("Failed to get spot light visible entities"),
            };

            iter_visible_chunks(visible_entities, &chunks, |entity, chunk_pos| {
                let mut mesh_key =
                    MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList)
                        | MeshPipelineKey::DEPTH_PREPASS;

                if is_directional_light {
                    mesh_key |= MeshPipelineKey::DEPTH_CLAMP_ORTHO;
                }

                let pipeline_id = pipelines.specialize(
                    &pipeline_cache,
                    &prepass_pipeline,
                    ChunkPipelineKey { mesh_key },
                );

                phase.add(Shadow {
                    draw_function: shadow_function,
                    pipeline: pipeline_id,
                    entity: entity,
                    distance: 0.0, // TODO: (bevy todo) sort front-to-back
                    batch_range: 0..1,
                    dynamic_offset: None,
                });
            });
        }
    }
}
