// pub mod ecs;
pub mod controller;
pub mod error;
pub mod greedy;
pub mod immediate;

use crate::{data::registries::Registries, topo::neighbors::Neighbors};

use super::lod::LevelOfDetail;

pub struct Context<'reg, 'chunk> {
    pub lod: LevelOfDetail,
    pub neighbors: Neighbors<'chunk>,
    pub registries: &'reg Registries,
}
