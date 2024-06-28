use std::mem;

use bevy::{
    ecs::{
        system::{Res, ResMut, Resource},
        world::Mut,
    },
    log::{debug, warn},
    prelude::{Deref, DerefMut, FromWorld, World},
    render::{
        render_resource::{BindGroup, BindGroupEntries},
        renderer::{RenderDevice, RenderQueue},
        MainWorld,
    },
};

use crate::{
    render::{
        lod::{LODs, LevelOfDetail, LodMap},
        meshing::controller::{ChunkMeshData, ChunkMeshStatus, ExtractableChunkMeshData},
    },
    util::{ChunkMap, ChunkSet},
};

use super::{
    chunk_batches::RenderChunkBatches, indirect::IndirectChunkData,
    observers::ObserverBatchBuffersStore, DefaultBindGroupLayouts,
};

pub fn extract_chunk_mesh_data(
    mut unprepared: ResMut<AddChunkMeshes>,
    mut remove_meshes: ResMut<RemoveChunkMeshes>,
    mut main_world: ResMut<MainWorld>,
) {
    // TODO: new extract logic that considers mesh LODs
    todo!()
}

/// Untrack chunk meshes in the render world and remove their data on the GPU
pub fn remove_chunk_meshes(
    mut remove: ResMut<RemoveChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut rebuild: ResMut<UpdateIndirectLODs>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    let remove = mem::replace(&mut remove.0, LodMap::default());

    for (lod, chunks) in remove.into_iter() {
        // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
        // is quite sensitive to running with empty vectors and maps.
        if chunks.is_empty() {
            return;
        }

        indirect_data.lod_mut(lod).remove_chunks(gpu, queue, chunks);
        // This LOD had its indirect data updated so we note it down to update the dependants of it later
        rebuild.insert_lod(lod);
    }
}

/// Upload unprepared chunk meshes to the GPU and track them in the render world
pub fn upload_chunk_meshes(
    mut add: ResMut<AddChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut update: ResMut<UpdateIndirectLODs>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    let add = mem::replace(&mut add.0, LodMap::default());

    for (lod, meshes) in add.into_iter() {
        // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
        // is quite sensitive to running with empty vectors and maps.
        if meshes.is_empty() {
            continue;
        }

        indirect_data.lod_mut(lod).upload_chunks(gpu, queue, meshes);
        // This LOD had its indirect data updated so we note it down to update the dependants of it later
        update.insert_lod(lod);
    }
}

pub fn update_indirect_chunk_data_dependants(
    mut update: ResMut<UpdateIndirectLODs>,
    mut batches: ResMut<RenderChunkBatches>,
    mut observer_batches: ResMut<ObserverBatchBuffersStore>,
) {
    for lod in update.contained_lods() {
        // TODO: need to split this up into per-LOD stuff as well
        batches.clear();
        observer_batches.clear();
    }

    // We just processed the updated LODs so we clear the update tracker
    update.0 = LODs::empty();
}

/// A store of unprepared chunk meshes
#[derive(Resource, Default, Deref, DerefMut)]
pub struct AddChunkMeshes(pub LodMap<ChunkMap<ChunkMeshData>>);

/// A store of chunks that should be removed from the render world
#[derive(Resource, Default, Deref, DerefMut)]
pub struct RemoveChunkMeshes(pub LodMap<ChunkSet>);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct UpdateIndirectLODs(pub LODs);

#[derive(Resource)]
pub struct IndirectRenderDataStore {
    lods: LodMap<IndirectChunkData>,
}

impl FromWorld for IndirectRenderDataStore {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();
        let default_bg_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            lods: LodMap::from_fn(|_lod| {
                Some(IndirectChunkData::new(
                    gpu,
                    default_bg_layouts.icd_quad_bg_layout.clone(),
                ))
            }),
        }
    }
}

impl IndirectRenderDataStore {
    pub fn lod(&self, lod: LevelOfDetail) -> &IndirectChunkData {
        self.lods
            .get(lod)
            .expect("This LOD map should not have any empty values")
    }

    pub fn lod_mut(&mut self, lod: LevelOfDetail) -> &mut IndirectChunkData {
        self.lods
            .get_mut(lod)
            .expect("This LOD map should not have any empty values")
    }
}
