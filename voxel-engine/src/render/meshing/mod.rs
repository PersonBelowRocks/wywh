pub mod ecs;
pub mod error;
pub mod greedy;
pub mod immediate;
pub mod workers;

use bevy::render::mesh::Mesh;
pub use workers::MeshWorkerPool;

use crate::{
    data::registries::Registries,
    topo::{access::ChunkAccess, neighbors::Neighbors},
};

use self::error::MesherResult;

use super::{occlusion::ChunkOcclusionMap, quad::ChunkQuads};

pub trait Mesher: Clone + Send + Sync + 'static {
    fn build<'reg, 'chunk, A, Nb>(
        &self,
        access: A,
        context: Context<'reg, 'chunk, Nb>,
    ) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess<'chunk>,
        Nb: ChunkAccess<'chunk>;
}

pub struct Context<'reg, 'chunk, Nb: ChunkAccess<'chunk>> {
    pub neighbors: Neighbors<'chunk, Nb>,
    pub registries: &'reg Registries,
}

#[derive(Clone, Debug)]
pub struct MesherOutput {
    pub mesh: Mesh,
    pub occlusion: ChunkOcclusionMap,
    pub quads: ChunkQuads,
}
