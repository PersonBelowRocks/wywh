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

use super::{occlusion::ChunkOcclusionMap, quad::GpuQuad};

pub struct Context<'reg, 'chunk> {
    pub neighbors: Neighbors<'chunk>,
    pub registries: &'reg Registries,
}
