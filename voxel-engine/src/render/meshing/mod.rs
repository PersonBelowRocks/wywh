// pub mod ecs;
pub mod controller;
pub mod error;
pub mod greedy;
pub mod immediate;

use bevy::render::mesh::Mesh;

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
    pub indices: Vec<u32>,
    pub quads: ChunkQuads,
}
