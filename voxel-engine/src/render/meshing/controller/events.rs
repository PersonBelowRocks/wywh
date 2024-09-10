use crate::{
    render::lod::{LODs, LevelOfDetail},
    topo::world::{chunk_populator::events::PriorityCalcStrategy, ChunkPos},
};
use bevy::prelude::*;

use super::{ChunkMeshData, ChunkMeshStatus};

/// Event sent when a mesh for a chunk should be built.
#[derive(Clone, Event, Debug)]
pub struct BuildMeshEvent {
    pub chunk_pos: ChunkPos,
    pub urgency: MeshJobUrgency,
    pub lod: LevelOfDetail,
    /// The tick that this job is from. The resulting chunk mesh will overwrite the existing chunk
    /// mesh if it's from an earlier tick. This is to ensure that the most up-to-date data is shown
    /// visually. The tick value should just be whatever the current tick is at the time that this event is sent.
    pub tick: u64,
}

/// Describes where/when the mesh job should be ran.
#[derive(Clone, Debug)]
pub enum MeshJobUrgency {
    /// Immediate mesh building, will wait for the mesh to be built before the next frame.
    P0,
    /// Offload the mesh building job to a background task pool with the given priority.
    /// The *higher* the priority the *sooner* the job will be ran.
    /// Meshes in this pool are built on a best-effort basis,
    /// and there is no guarantee when they will be ready.
    P1(u32),
}

/// Event sent when a chunk mesh at some LOD is done being built.
#[derive(Clone, Event, Debug)]
pub struct MeshFinishedEvent {
    pub chunk_pos: ChunkPos,
    pub lod: LevelOfDetail,
    pub mesh: ChunkMeshData,
    pub tick: u64,
}

/// Event sent to recalculate the priorities of pending mesh building tasks based on the provided strategy.
#[derive(Clone, Event, Debug)]
pub struct RecalculateMeshBuildingEventPrioritiesEvent {
    pub strategy: PriorityCalcStrategy,
}

/// Event sent to remove chunk meshes at LODs from the render world and the mesh builder job queue.
#[derive(Clone, Event, Debug)]
pub struct RemoveChunkMeshEvent {
    pub chunk_pos: ChunkPos,
    pub lods: LODs,
    pub tick: u64,
}
