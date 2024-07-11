use bevy::{
    ecs::system::{Res, ResMut, Resource},
    log::info,
    prelude::{Deref, DerefMut, FromWorld, World},
    render::{
        renderer::{RenderDevice, RenderQueue},
        MainWorld,
    },
};

use crate::{
    render::{
        lod::{FilledLodMap, LODs, LevelOfDetail},
        meshing::controller::{ChunkMeshData, ExtractableChunkMeshData},
    },
    util::{ChunkMap, ChunkSet},
};

use super::{
    indirect::IndirectChunkData, utils::InspectChunks, views::ViewBatchBuffersStore,
    DefaultBindGroupLayouts,
};

pub fn extract_chunk_mesh_data(
    mut add_meshes: ResMut<AddChunkMeshes>,
    mut remove_meshes: ResMut<RemoveChunkMeshes>,
    mut main_world: ResMut<MainWorld>,
) {
    main_world.resource_scope::<ExtractableChunkMeshData, _>(|_, mut meshes| {
        // The main world regulates how often we're allowed to extract these, so if we're not currently allowed
        // to extract we return early and check again next frame.
        if !meshes.should_extract() {
            return;
        }

        for lod in LevelOfDetail::LODS {
            let add = &mut add_meshes[lod];
            let remove = &mut remove_meshes[lod];

            for (chunk, mesh) in meshes.additions(lod) {
                add.set(chunk, mesh.clone());
            }

            for chunk in meshes.removals(lod) {
                remove.set(chunk);
            }
        }

        meshes.mark_as_extracted(LODs::all());
    });
}

/// Untrack chunk meshes in the render world and remove their data on the GPU
pub fn remove_chunk_meshes(
    mut remove: ResMut<RemoveChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut update: ResMut<UpdateIndirectLODs>,
    inspections: Res<InspectChunks>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    for lod in LevelOfDetail::LODS {
        let icd = indirect_data.lod_mut(lod);
        let remove_lod = &mut remove[lod];

        // Nothing to remove so we skip.
        if remove_lod.is_empty() {
            continue;
        }

        for chunk in inspections.iter().filter(|&c| remove_lod.contains(c)) {
            match icd.get_chunk_metadata(chunk) {
                None => {
                    info!("REMOVING chunk {chunk} from LOD {lod:?}, it doesn't have any metadata.")
                }
                Some(metadata) => {
                    info!("REMOVING chunk {chunk} from LOD {lod:?} with metadata: {metadata:#?}")
                }
            }
        }

        // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
        // is quite sensitive to running with empty vectors and maps.
        if icd.is_empty() {
            continue;
        }

        icd.remove_chunks(gpu, queue, &remove_lod, Some(inspections.as_ref()));
        // This LOD had its indirect data updated so we note it down to update the dependants of it later
        update.insert_lod(lod);
        // Clear the removal queue
        remove_lod.clear();
    }
}

/// Upload unprepared chunk meshes to the GPU and track them in the render world
pub fn upload_chunk_meshes(
    mut add: ResMut<AddChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut update: ResMut<UpdateIndirectLODs>,
    inspections: Res<InspectChunks>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    for lod in LevelOfDetail::LODS {
        let icd = indirect_data.lod_mut(lod);
        let meshes = &mut add[lod];

        // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
        // is quite sensitive to running with empty vectors and maps.
        if meshes.is_empty() {
            continue;
        }

        for chunk in inspections.iter().filter(|&c| meshes.contains(c)) {
            match icd.get_chunk_metadata(chunk) {
                None => info!(
                    "ADDING chunk {chunk} at LOD {lod:?}, it doesn't have any existing metadata."
                ),
                Some(metadata) => info!(
                    "ADDING chunk {chunk} at LOD {lod:?} with existing metadata: {metadata:#?}"
                ),
            }
        }

        icd.upload_chunks(gpu, queue, meshes.clone(), Some(inspections.as_ref()));
        // This LOD had its indirect data updated so we note it down to update the dependants of it later
        update.insert_lod(lod);
        // Clear the addition queue
        meshes.clear();
    }
}

pub fn update_indirect_mesh_data_dependants(
    mut update: ResMut<UpdateIndirectLODs>,
    mut batches: ResMut<ViewBatchBuffersStore>,
) {
    for _lod in update.contained_lods() {
        // TODO: need to split this up into per-LOD stuff as well
        batches.clear();
    }

    // We just processed the updated LODs so we clear the update tracker
    update.0 = LODs::empty();
}

/// A store of unprepared chunk meshes
#[derive(Resource, Default, Deref, DerefMut)]
pub struct AddChunkMeshes(pub FilledLodMap<ChunkMap<ChunkMeshData>>);

/// A store of chunks that should be removed from the render world
#[derive(Resource, Default, Deref, DerefMut)]
pub struct RemoveChunkMeshes(pub FilledLodMap<ChunkSet>);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct UpdateIndirectLODs(pub LODs);

#[derive(Resource)]
pub struct IndirectRenderDataStore {
    lods: FilledLodMap<IndirectChunkData>,
}

impl FromWorld for IndirectRenderDataStore {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();
        let default_bg_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            lods: FilledLodMap::from_fn(|lod| {
                IndirectChunkData::new(lod, gpu, default_bg_layouts.icd_quad_bg_layout.clone())
            }),
        }
    }
}

impl IndirectRenderDataStore {
    pub fn lod(&self, lod: LevelOfDetail) -> &IndirectChunkData {
        &self.lods[lod]
    }

    pub fn lod_mut(&mut self, lod: LevelOfDetail) -> &mut IndirectChunkData {
        &mut self.lods[lod]
    }
}
