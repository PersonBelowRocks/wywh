use bevy::{
    pbr::{
        CascadesVisibleEntities, CubemapVisibleEntities, ExtractedDirectionalLight,
        ExtractedPointLight, LightEntity, MeshPipelineKey, Shadow, ViewLightEntities,
    },
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, RenderPhase},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
        view::VisibleEntities,
    },
};

use crate::render::core::{gpu_chunk::IndirectRenderDataStore, gpu_registries::RegistryBindGroup};

use super::{IndirectChunkPipelineKey, IndirectChunkPrepassPipeline, IndirectChunksPrepass};

// largely taken from
// https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/render/light.rs#L1590
pub fn shadow_queue_indirect_chunks(
    registry_bg: Option<Res<RegistryBindGroup>>,
    indirect_data: Res<IndirectRenderDataStore>,
    shadow_draw_functions: Res<DrawFunctions<Shadow>>,
    prepass_pipeline: Res<IndirectChunkPrepassPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<IndirectChunkPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    view_lights: Query<(Entity, &ViewLightEntities)>,
    mut view_light_shadow_phases: Query<(&LightEntity, &mut RenderPhase<Shadow>)>,
    point_light_entities: Query<&CubemapVisibleEntities, With<ExtractedPointLight>>,
    directional_light_entities: Query<&CascadesVisibleEntities, With<ExtractedDirectionalLight>>,
    spot_light_entities: Query<&VisibleEntities, With<ExtractedPointLight>>,
) {
    // While we don't use the registry bind group in this system, we do use it in our draw function.
    // It's also possible for this system to run before the registry bind group is prepared, leading to
    // an error down the line in the draw function. To avoid this we only queue our indirect chunks if the
    // registry bind group is prepared.
    // We also only want to run the draw function if our indirect data is ready to be rendered.
    if registry_bg.is_none() || !indirect_data.ready {
        return;
    }

    let shadow_function = shadow_draw_functions.read().id::<IndirectChunksPrepass>();

    for (entity, view_lights) in &view_lights {
        for view_light_entity in view_lights.lights.iter().copied() {
            let (light_entity, mut phase) =
                view_light_shadow_phases.get_mut(view_light_entity).unwrap();

            let is_directional_light = matches!(light_entity, LightEntity::Directional { .. });
            let _visible_entities = match light_entity {
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

            let mut key = MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList)
                | MeshPipelineKey::DEPTH_PREPASS;

            if is_directional_light {
                key |= MeshPipelineKey::DEPTH_CLAMP_ORTHO;
            }

            let pipeline_id = pipelines.specialize(
                &pipeline_cache,
                &prepass_pipeline,
                IndirectChunkPipelineKey { inner: key },
            );

            phase.add(Shadow {
                draw_function: shadow_function,
                pipeline: pipeline_id,
                entity: entity,
                distance: 0.0, // TODO: (bevy todo) sort front-to-back
                batch_range: 0..1,
                dynamic_offset: None,
            });
        }
    }
}
