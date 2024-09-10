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
    diagnostics::{DiagRecStatus, DiagnosticsTx, ENGINE_DIAGNOSTICS},
    render::{
        lod::{FilledLodMap, LODs, LevelOfDetail},
        meshing::controller::{ChunkMeshData, ChunkMeshExtractBridge},
    },
    util::{ChunkMap, ChunkSet},
};

use super::{
    indirect::{IcdCommit, IndirectChunkData},
    utils::InspectChunks,
    views::ViewBatchBuffersStore,
    BindGroupProvider,
};

pub fn extract_chunk_mesh_data(
    mut add_meshes: ResMut<AddChunkMeshes>,
    mut remove_meshes: ResMut<RemoveChunkMeshes>,
    mut main_world: ResMut<MainWorld>,
    diagnostics_tx: Res<DiagnosticsTx>,
) {
    main_world.resource_scope::<ChunkMeshExtractBridge, _>(|_, mut meshes| {
        // The main world regulates how often we're allowed to extract these, so if we're not currently allowed
        // to extract we return early and check again next frame.
        if !meshes.should_extract() {
            return;
        }

        diagnostics_tx.measure(&ENGINE_DIAGNOSTICS.mesh_extract_time, |status| {
            *status = DiagRecStatus::Ignore;

            for lod in LevelOfDetail::LODS {
                let add = &mut add_meshes[lod];
                let remove = &mut remove_meshes[lod];

                if !meshes.is_empty(lod) {
                    *status = DiagRecStatus::Record;
                }

                for (chunk, mesh) in meshes.additions(lod) {
                    add.set(chunk, mesh.clone());
                }

                for chunk in meshes.removals(lod) {
                    remove.set(chunk);
                }
            }

            meshes.mark_as_extracted(LODs::all());
        });
    });
}

pub fn update_gpu_mesh_data(
    mut add: ResMut<AddChunkMeshes>,
    mut remove: ResMut<RemoveChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut update: ResMut<UpdateIndirectLODs>,
    diagnostics_tx: Res<DiagnosticsTx>,
    inspections: Res<InspectChunks>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    diagnostics_tx.measure(&ENGINE_DIAGNOSTICS.gpu_update_time, |diag_status| {
        *diag_status = DiagRecStatus::Ignore;

        for lod in LevelOfDetail::LODS {
            let icd = indirect_data.lod_mut(lod);
            let additions = &mut add[lod];
            let removals = &mut remove[lod];

            // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
            // is quite sensitive to running with empty vectors and maps.
            if additions.is_empty() && removals.is_empty() {
                continue;
            }

            // We're uploading something this time so let's record the time
            *diag_status = DiagRecStatus::Record;

            let mut commit = IcdCommit::new();
            commit.set_inspections(&inspections);

            for chunk in inspections.iter().filter(|&c| additions.contains(c)) {
                match icd.get_chunk_metadata(chunk) {
                    None => info!(
                        "ADDING chunk {chunk} at LOD {lod:?}, it doesn't have any existing metadata."
                    ),
                    Some(metadata) => info!(
                        "ADDING chunk {chunk} at LOD {lod:?} with existing metadata: {metadata:#?}"
                    ),
                }
            }

            commit.add(additions.clone());

            for chunk in inspections.iter().filter(|&c| removals.contains(c)) {
                match icd.get_chunk_metadata(chunk) {
                    None => {
                        info!("REMOVING chunk {chunk} from LOD {lod:?}, it doesn't have any metadata.")
                    }
                    Some(metadata) => {
                        info!("REMOVING chunk {chunk} from LOD {lod:?} with metadata: {metadata:#?}")
                    }
                }
            }

            commit.remove(removals.clone());

            icd.commit(gpu, queue, commit);
            // This LOD had its indirect data updated so we note it down to update the dependants of it later
            update.insert_lod(lod);
            // Clear the addition queue
            additions.clear();
            removals.clear();
        }
    })
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
        let bg_provider = world.resource::<BindGroupProvider>();

        Self {
            lods: FilledLodMap::from_fn(|lod| IndirectChunkData::new(gpu, bg_provider, lod)),
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
