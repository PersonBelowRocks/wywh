use bevy::{
    pbr::{
        CascadesVisibleEntities, CubemapVisibleEntities, ExtractedDirectionalLight,
        ExtractedPointLight, LightEntity, MeshPipelineKey, RenderMeshInstances, Shadow,
        ViewLightEntities,
    },
    prelude::*,
    render::{
        render_asset::RenderAssets,
        render_phase::{DrawFunctions, RenderPhase},
        render_resource::{PipelineCache, SpecializedMeshPipelines},
        view::VisibleEntities,
    },
};

use super::{
    gpu_chunk::{ChunkRenderData, ChunkRenderDataStore},
    prepass::{ChunkPrepassPipeline, DrawVoxelChunkPrepass},
    render::ChunkPipelineKey,
};

// largely taken from
// https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/render/light.rs#L1590
pub fn queue_shadows(
    chunk_data_store: Res<ChunkRenderDataStore>,
    shadow_draw_functions: Res<DrawFunctions<Shadow>>,
    prepass_pipeline: Res<ChunkPrepassPipeline>,
    render_meshes: Res<RenderAssets<Mesh>>,
    mut render_mesh_instances: ResMut<RenderMeshInstances>,
    mut pipelines: ResMut<SpecializedMeshPipelines<ChunkPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    view_lights: Query<(Entity, &ViewLightEntities)>,
    mut view_light_shadow_phases: Query<(&LightEntity, &mut RenderPhase<Shadow>)>,
    point_light_entities: Query<&CubemapVisibleEntities, With<ExtractedPointLight>>,
    directional_light_entities: Query<&CascadesVisibleEntities, With<ExtractedDirectionalLight>>,
    spot_light_entities: Query<&VisibleEntities, With<ExtractedPointLight>>,
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

            for entity in &visible_entities.entities {
                // skip all entities that dont have chunk render data
                if !chunk_data_store
                    .map
                    .get(entity)
                    .is_some_and(|data| matches!(data, ChunkRenderData::BindGroup(_)))
                {
                    continue;
                }

                let Some(mesh_instance) = render_mesh_instances.get_mut(entity) else {
                    continue;
                };
                if !mesh_instance.shadow_caster {
                    continue;
                }
                let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
                    continue;
                };

                let mut mesh_key =
                    MeshPipelineKey::from_primitive_topology(mesh.primitive_topology)
                        | MeshPipelineKey::DEPTH_PREPASS;

                if is_directional_light {
                    mesh_key |= MeshPipelineKey::DEPTH_CLAMP_ORTHO;
                }

                let pipeline_id = pipelines.specialize(
                    &pipeline_cache,
                    &prepass_pipeline,
                    ChunkPipelineKey { mesh_key },
                    &mesh.layout,
                );

                let pipeline_id = match pipeline_id {
                    Ok(id) => id,
                    Err(err) => {
                        error!("{}", err);
                        continue;
                    }
                };

                phase.add(Shadow {
                    draw_function: shadow_function,
                    pipeline: pipeline_id,
                    entity: *entity,
                    distance: 0.0, // TODO: (bevy todo) sort front-to-back
                    batch_range: 0..1,
                    dynamic_offset: None,
                });
            }
        }
    }
}
