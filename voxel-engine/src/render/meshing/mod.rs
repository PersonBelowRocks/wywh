pub mod ecs;
pub mod error;
pub mod greedy;
pub mod immediate;
pub mod workers;

use bevy::render::mesh::Mesh;
pub use workers::MeshWorkerPool;

use crate::{
    data::registries::Registries,
    topo::{neighbors::Neighbors, world::Crra},
};

use self::error::MesherResult;

use super::{occlusion::ChunkOcclusionMap, quad::ChunkQuads};

pub trait Mesher: Clone + Send + Sync + 'static {
    fn build<'reg, 'chunk>(
        &self,
        access: Crra<'chunk>,
        context: Context<'reg, 'chunk>,
    ) -> MesherResult;
}

pub struct Context<'reg, 'chunk> {
    pub neighbors: Neighbors<'chunk>,
    pub registries: &'reg Registries,
}

#[derive(Clone, Debug)]
pub struct MesherOutput {
    pub mesh: Mesh,
    pub occlusion: ChunkOcclusionMap,
    pub quads: ChunkQuads,
}
