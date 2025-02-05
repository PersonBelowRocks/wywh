use crate::render::lod::{LODs, LevelOfDetail, LodMap};
use crate::render::meshing::controller::ChunkMeshData;
use crate::render::meshing::visibility_logic::ChunkConnectivitySupergraph;
use crate::topo::controller::VoxelWorldTick;
use crate::topo::world::chunk_manager::ChunkNotification;
use crate::topo::world::ChunkPos;
use crate::util::{ChunkMap, ChunkSet};
use bevy::prelude::{EventReader, Res, ResMut, Resource};
use bevy::tasks::futures_lite::StreamExt;
use octo::voxelmap::VoxelMap;
use std::sync::Arc;

/// The state of a chunk mesh, and the tick that the build event was sent at.
#[derive(Copy, Clone, Debug)]
pub struct ChunkMeshTimestate {
    /// The tick that the build event for this mesh was sent on. This is not the same
    /// as the age of the chunk mesh, but it is always older than, or the same as, the chunk mesh's age.
    /// We keep track of this age so that the most up-to-date chunk mesh is used, and we want to ignore
    /// requests to remove chunk meshes if those requests are older than the chunk mesh.
    pub tick: u64,
    /// The status of the chunk mesh.
    pub status: ChunkMeshState,
}

impl ChunkMeshTimestate {
    /// Create a `ChunkMeshStatus::Absent` for the given tick.
    pub fn absent(tick: u64) -> Self {
        Self {
            tick,
            status: ChunkMeshState::Absent,
        }
    }
}

/// Describes the state that a chunk mesh is in. This reflects the behaviour elsewhere in the engine
/// about how the chunk mesh should be treated.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChunkMeshState {
    /// An updated chunk mesh is absent, but there might be an outdated one present. This status is
    /// assigned to a chunk mesh when the underlying chunk is changed in some way. Chunks will lose their
    /// absent status once a mesh has been built (or the chunk is determined to be empty).
    Absent,
    /// The chunk mesh is built, but is empty (i.e., has no geometry).
    /// This can happen if the chunk is only void or if the chunk is encased by solid blocks and
    /// thus all faces are culled. Empty chunk meshes will not be extracted
    /// but can overwrite a [filled][`ChunkMeshStatus::Filled`] mesh if the empty one is younger, or vice-versa.
    Empty,
    /// A filled chunk mesh has geometry and is ready to be [extracted][`ChunkMeshStatus::Extracted`] to the render world.
    Filled,
    /// The chunk mesh has been extracted into the render world, where we no longer have any say in how it's treated.
    /// Only filled meshes can be extracted. Extraction happens through the addition & removal buffers in [`ChunkMeshExtractBridge`].
    Extracted,
}

/// Tracks and manages the state of chunk meshes and provides visibility checks.
#[derive(Resource)]
pub struct ChunkMeshManager {
    state_in_lods: LodMap<VoxelMap<ChunkMeshTimestate>>,
    ccsg: ChunkConnectivitySupergraph,
}

impl ChunkMeshManager {
    pub fn new() -> Self {
        Self {
            state_in_lods: LodMap::from_fn(|_| Some(VoxelMap::new())),
            ccsg: ChunkConnectivitySupergraph::new(),
        }
    }

    /// Get the state and tick of a chunk mesh at the given LOD.
    /// Returns `None` if this chunk mesh does not exist at the given LOD.
    ///
    /// See [`ChunkMeshTimestate`] for more information.
    #[inline]
    pub fn timestate(&self, lod: LevelOfDetail, chunk_pos: ChunkPos) -> Option<ChunkMeshTimestate> {
        self.state_in_lods[lod].get(chunk_pos).as_deref().copied()
    }

    /// Returns `true` if this mesh state manager has a state for the given chunk at the given LOD.
    #[inline]
    pub fn contains(&self, lod: LevelOfDetail, chunk_pos: ChunkPos) -> bool {
        self.state_in_lods[lod].contains(chunk_pos)
    }

    /// Get the states of this chunk at different LODs.
    pub fn get_states(&self, chunk_pos: ChunkPos) -> LodMap<ChunkMeshTimestate> {
        self.state_in_lods
            .iter()
            .filter_map(|(lod, chunks)| {
                chunks
                    .get(chunk_pos)
                    .as_deref()
                    .copied()
                    .map(|status| (lod, status))
            })
            .collect::<LodMap<_>>()
    }
}

/// Acts as a sort of bridge between the main world and render world for chunk meshes.
/// The render world will change its state depending on how this resource changes.
#[derive(Resource)]
pub struct ChunkMeshExtractBridge {
    add: LodMap<ChunkMap<ChunkMeshData>>,
    remove: LodMap<ChunkSet>,
    /// Indicates if we should extract chunks to the render world (or remove chunks from the render world).
    /// Usually used to regulate extraction a bit so that we can extract chunks in bulk instead of extracting them immediately
    /// as they become available. This helps reduce some lag when meshing lots of chunks.
    // TODO: maybe we should split this up into a per-lod thing.
    should_extract: bool,
}

impl Default for ChunkMeshExtractBridge {
    fn default() -> Self {
        Self {
            should_extract: false,

            add: LodMap::from_fn(|_| Some(ChunkMap::default())),
            remove: LodMap::from_fn(|_| Some(ChunkSet::default())),
        }
    }
}

impl ChunkMeshExtractBridge {
    fn set_status(&mut self, chunk_pos: ChunkPos, lod: LevelOfDetail, status: ChunkMeshTimestate) {
        self.statuses.lods[lod].insert(chunk_pos, status);
    }

    /// Get the [`ChunkMeshStatusManager`] associated with this bridge.
    pub fn chunk_mesh_status_manager(&self) -> &Arc<ChunkMeshStatusManager> {
        &self.statuses
    }

    /// Get the status of this chunk at different LODs.
    pub fn get_statuses(&self, chunk_pos: ChunkPos) -> LodMap<ChunkMeshTimestate> {
        self.statuses.get_statuses(chunk_pos)
    }

    /// "Flush" the queued mesh data. This marks it as ready for extraction so it will be extracted
    /// next time the extract schedule runs.
    pub fn flush(&mut self) {
        self.should_extract = true;
    }

    /// The number of chunks queued for extraction at this LOD.
    pub fn queued_additions(&self, lod: LevelOfDetail) -> usize {
        self.add[lod].len()
    }

    /// The number of chunks queued for removal from the render world at this LOD.
    pub fn queued_removals(&self, lod: LevelOfDetail) -> usize {
        self.remove[lod].len()
    }

    pub fn is_empty(&self, lod: LevelOfDetail) -> bool {
        self.queued_additions(lod) == 0 || self.queued_removals(lod) == 0
    }

    pub fn should_extract(&self) -> bool {
        self.should_extract
    }

    /// Try to queue a chunk mesh of a given age and LOD for extraction. Will do nothing if there's
    /// a newer version either already queued or extracted.
    pub fn add_chunk_mesh(
        &mut self,
        chunk_pos: ChunkPos,
        lod: LevelOfDetail,
        tick: u64,
        mesh_data: ChunkMeshData,
    ) {
        let mut has_filled = false;

        // If we already have a newer chunk mesh, then we return early since we should never extract an
        // older version of a chunk mesh.
        if let Some(existing_status) = self.statuses.timed_status(lod, chunk_pos) {
            if existing_status.tick > tick {
                return;
            }

            has_filled = matches!(
                existing_status.status,
                ChunkMeshState::Filled | ChunkMeshState::Extracted
            );
        }

        let status = mesh_data.status();

        match status {
            // If the mesh is empty, queue it for removal so that the previous mesh (if it exists) is removed.
            ChunkMeshState::Empty if has_filled => {
                self.remove[lod].set(chunk_pos);
            }
            // Only queue the mesh for extraction if it's filled.
            ChunkMeshState::Filled => {
                self.add[lod].set(chunk_pos, mesh_data);
            }
            _ => (),
        }

        // Even if we don't queue the mesh for extraction we still need to note down its status.
        self.set_status(chunk_pos, lod, ChunkMeshTimestate { tick, status });
    }

    /// Queue a chunk at a given LOD for removal from the render world.
    pub fn remove_chunk(&mut self, chunk_pos: ChunkPos, lod: LevelOfDetail, tick: u64) {
        if let Some(existing) = self.statuses.timed_status(lod, chunk_pos) {
            if existing.tick > tick {
                return;
            }
        }

        self.statuses.lods[lod].remove(&chunk_pos);
        self.add[lod].remove(chunk_pos);
        self.remove[lod].set(chunk_pos);
    }

    pub fn additions(
        &self,
        lod: LevelOfDetail,
    ) -> impl Iterator<Item = (ChunkPos, &ChunkMeshData)> + '_ {
        self.add[lod].iter()
    }

    pub fn removals(&self, lod: LevelOfDetail) -> impl Iterator<Item = ChunkPos> + '_ {
        self.remove[lod].iter()
    }

    /// Clear the removal and addition queues and mark the added chunks in the queue as being extracted.
    /// Also resets the 'Self::should_extract()' status.
    /// Should be called in the extract stage in the render world after copying data to communicate the
    /// status of the meshes to the main world.
    pub fn mark_as_extracted(&mut self, lods: LODs) {
        self.should_extract = false;

        for lod in lods.contained_lods() {
            self.remove[lod].clear();

            let additions = &mut self.add[lod];
            for (chunk_pos, _) in additions.drain() {
                self.statuses.lods[lod]
                    .get_mut(&chunk_pos)
                    .expect("All chunk positions queued for addition should have a status")
                    .status = ChunkMeshState::Extracted;
            }
        }
    }
}

pub fn invalidate_updated_chunks(
    mut notifications: EventReader<ChunkNotification>,
    mut mesh_manager: ResMut<ChunkMeshManager>,
    tick: Res<VoxelWorldTick>,
) {
    let current_tick = tick.get();

    for &notification in notifications.read() {
        mesh_manager
            .ccsg
            .remove_connectivity_graph(notification.chunk_pos);

        for lod in LevelOfDetail::LODS {
            mesh_manager.state_in_lods[lod].insert(
                notification.chunk_pos,
                ChunkMeshTimestate::absent(current_tick),
            );
        }
    }
}
