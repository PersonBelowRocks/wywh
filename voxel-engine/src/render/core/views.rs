use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::render_resource::BindGroup;
use bevy::{
    prelude::*,
    render::{render_resource::Buffer, Extract},
};

use crate::topo::controller::{ChunkBatch, ChunkBatchLod, VisibleBatches};

use super::phase::DeferredBatchPrepass;

#[derive(Clone)]
pub struct IndirectViewBatch {
    pub cull_bind_group: BindGroup,
    pub num_chunks: u32,
    pub indirect: Buffer,
    pub count: Buffer,
}

pub type ViewBatches = EntityHashMap<IndirectViewBatch>;

#[derive(Default, Clone)]
pub struct ViewBatchBuffersStore(EntityHashMap<ViewBatches>);

impl ViewBatchBuffersStore {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn get_batch(
        &self,
        view_entity: Entity,
        batch_entity: Entity,
    ) -> Option<&IndirectViewBatch> {
        self.0.get(&view_entity)?.get(&batch_entity)
    }

    pub fn get_batches(&self, view_entity: Entity) -> Option<&ViewBatches> {
        self.0.get(&view_entity)
    }

    pub fn get_or_insert(&mut self, view_entity: Entity) -> &mut ViewBatches {
        self.0.entry(view_entity).or_insert(ViewBatches::default())
    }
}

/// Copies of the indirect, instance, and count buffers for each observer so they can cull individually.
#[derive(Resource, Clone, Default, Deref, DerefMut)]
pub struct ObserverBatchBuffersStore(ViewBatchBuffersStore);

pub fn extract_observer_visible_batches(
    query: Extract<Query<(Entity, &VisibleBatches)>>,
    batch_query: Query<(&ChunkBatch, &ChunkBatchLod)>,
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
    mut prepass_phases: ResMut<ViewSortedRenderPhases<DeferredBatchPrepass>>,
    mut live_entities: Local<EntityHashSet>,
) {
    live_entities.clear();

    for (entity, camera) in &cameras {
        if !camera.is_active {
            continue;
        }

        prepass_phases.insert_or_clear(entity);

        live_entities.insert(entity);
    }

    prepass_phases.retain(|entity, _| live_entities.contains(entity));
}
