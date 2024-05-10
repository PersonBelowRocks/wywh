use bevy::ecs::query::QueryEntityError;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::{ecs::system::SystemParam, render::view::VisibleEntities};

use crate::topo::world::{ChunkEntity, ChunkPos};

use super::gpu_chunk::{ChunkRenderData, ChunkRenderDataStore};

#[derive(SystemParam)]
pub struct ChunkDataParams<'w, 's> {
    pub chunk_entities: Query<'w, 's, Read<ChunkPos>, With<ChunkEntity>>,
    pub chunk_data_store: Res<'w, ChunkRenderDataStore>,
}

pub fn iter_visible_chunks<'w, 's, F>(
    visible: &VisibleEntities,
    chunk_data_params: &ChunkDataParams<'w, 's>,
    mut f: F,
) where
    F: FnMut(Entity, ChunkPos),
{
    for entity in &visible.entities {
        // Extract chunk position for this entity, and skip all entities that don't match the query.
        let chunk_pos = match chunk_data_params.chunk_entities.get(*entity) {
            Ok(chunk_pos) => *chunk_pos,
            Err(QueryEntityError::QueryDoesNotMatch(_)) => continue,
            Err(QueryEntityError::NoSuchEntity(entity)) => {
                error!("Entity {entity:?} seemingly doesn't exist in render world");
                continue;
            }

            _ => panic!("Unexpected result when getting chunk position from entity"),
        };

        // Skip chunks that don't have chunk render data on the GPU
        if !chunk_data_params
            .chunk_data_store
            .map
            .get(chunk_pos)
            .is_some_and(|data| matches!(data.data, ChunkRenderData::Gpu(_)))
        {
            continue;
        }

        f(*entity, chunk_pos);
    }
}
