use bevy::prelude::*;

use crate::{
    render::lod::LevelOfDetail,
    topo::{
        controller::{LastPosition, VoxelWorldTick},
        neighbors::NeighborSelection,
        world::{chunk_populator::events::ChunkPopulated, ChunkPos},
        ChunkJobQueue, ObserverSettings,
    },
};

use super::{
    events::BuildChunkMeshEvent, ChunkMeshExtractBridge, ChunkMeshStatusManager,
    TimedChunkMeshStatus,
};

/// A helper type to find which LOD (if any) a chunk mesh should have.
#[derive(Clone, Debug)]
pub struct LodFinder(Vec<ChunkPos>);

impl LodFinder {
    pub fn new(positions: impl IntoIterator<Item = ChunkPos>) -> Self {
        Self(positions.into_iter().collect())
    }

    pub fn get_lod(&self, chunk_pos: ChunkPos) -> Option<LevelOfDetail> {
        todo!()
    }

    pub fn get_priority(&self, chunk_pos: ChunkPos) -> Option<u32> {
        todo!()
    }
}

#[derive(Resource, Clone, Deref)]
pub struct MeshWorkerChannel(flume::Sender<BuildChunkMeshEvent>);

fn is_surrounded_by_populated_chunks(
    chunk_pos: ChunkPos,
    statuses: &ChunkMeshStatusManager,
) -> bool {
    for neighbor_offset in NeighborSelection::all().selected() {
        let neighbor_pos = ChunkPos::from(neighbor_offset + chunk_pos.as_ivec3());

        let neighbor_statuses = statuses.get_statuses(neighbor_pos);
        if neighbor_statuses.is_empty() {
            return false;
        }
    }

    return true;
}

/// Create an LOD finder from the chunk positions of the given observers.
fn lod_finder_from_observer_query(
    observers: &Query<(&LastPosition, &ObserverSettings)>,
) -> LodFinder {
    let chunks = observers.iter().map(|(last_pos, _)| last_pos.chunk_pos);
    LodFinder::new(chunks)
}

pub fn schedule_populated_chunks(
    mut queue: Local<ChunkJobQueue<BuildChunkMeshEvent>>,
    //////////
    observers: Query<(&LastPosition, &ObserverSettings)>,
    mut events: EventReader<ChunkPopulated>,
    mut bridge: ResMut<ChunkMeshExtractBridge>,
    job_tx: Res<MeshWorkerChannel>,
    tick: Res<VoxelWorldTick>,
) {
    let status = TimedChunkMeshStatus::unfulfilled(tick.get());
    let lod_finder = lod_finder_from_observer_query(&observers);

    // Queue newly populated chunks that are surrounded by previously populated chunks.
    for event in events.read() {
        let Some(lod) = lod_finder.get_lod(event.chunk_pos) else {
            continue;
        };

        bridge.set_status(event.chunk_pos, LevelOfDetail::X16Subdiv, status);

        let status_manager = bridge.chunk_mesh_status_manager().as_ref();
        if is_surrounded_by_populated_chunks(event.chunk_pos, status_manager) {
            todo!()
            // queue.push(event.chunk_pos, lod_finder.get_priority(event.chunk_pos).unwrap());
        }
    }

    // Queue all the chunks that observers are located at.
    for (last_pos, _) in &observers {
        todo!();
        // let status_manager = bridge.chunk_mesh_status_manager().as_ref();

        // let is_surrounded = is_surrounded_by_populated_chunks(last_pos.chunk_pos, status_manager);
        // let is_queued = queue.get(&last_pos.chunk_pos).is_some();

        // if is_surrounded && !is_queued {
        //     queue.push(last_pos.chunk_pos, 0);
        // }
    }

    let available_space = job_tx.capacity().unwrap() - job_tx.len();
    for _ in 0..available_space {
        let Some(job) = queue.pop() else {
            break;
        };
    }
}
