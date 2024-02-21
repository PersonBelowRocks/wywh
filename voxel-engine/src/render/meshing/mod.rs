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
    fn build<A, Nb>(
        &self,
        access: A,
        context: Context<Nb>,
    ) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess,
        Nb: ChunkAccess;
}

pub struct Context<'a, A: ChunkAccess> {
    pub neighbors: Neighbors<A>,
    pub registries: &'a Registries,
}

#[derive(Clone, Debug)]
pub struct MesherOutput {
    pub mesh: Mesh,
    pub occlusion: ChunkOcclusionMap,
    pub quads: ChunkQuads,
}
