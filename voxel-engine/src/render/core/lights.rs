use bevy::{pbr::LightEntity, prelude::*};

use crate::topo::controller::VisibleBatches;

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
    cmds.insert_or_spawn_batch(insert.into_iter());
}
