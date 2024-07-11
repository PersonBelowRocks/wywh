use bevy::{
    ecs::entity::EntityHashMap,
    pbr::{
        CascadesVisibleEntities, CubemapVisibleEntities, ExtractedClusterConfig,
        ExtractedDirectionalLight, ExtractedPointLight,
    },
    prelude::*,
    render::{
        primitives::{CubemapFrusta, Frustum},
        view::{ExtractedView, RenderLayers, VisibleEntities},
    },
};

use crate::topo::controller::VisibleBatches;

use super::views::ViewBatchBuffersStore;

#[derive(Default, Resource, Deref, DerefMut)]
pub struct DirectionalLightBatchBuffers(ViewBatchBuffersStore);

#[derive(Default, Resource, Deref, DerefMut)]
pub struct CubemapBatchBuffers(ViewBatchBuffersStore);

#[derive(Default, Resource, Deref, DerefMut)]
pub struct PointLightBatchBuffers(ViewBatchBuffersStore);

pub fn prepare_light_batches(
    q_views: Query<(
        Entity,
        &ExtractedView,
        &ExtractedClusterConfig,
        Option<&RenderLayers>,
    )>,
    q_directional_lights: Query<(Entity, &VisibleBatches, &ExtractedDirectionalLight)>,
    q_point_lights: Query<(
        Entity,
        &VisibleBatches,
        &ExtractedPointLight,
        AnyOf<(&CubemapFrusta, &Frustum)>,
    )>,
    mut dirlight_batches: ResMut<DirectionalLightBatchBuffers>,
) {
    for (entity, visible, light) in &q_directional_lights {}
}
