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
