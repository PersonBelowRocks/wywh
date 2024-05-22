// pub mod ecs;
pub mod controller;
pub mod error;
pub mod greedy;
pub mod immediate;

use crate::{data::registries::Registries, topo::neighbors::Neighbors};

pub struct Context<'reg, 'chunk> {
    pub neighbors: Neighbors<'chunk>,
    pub registries: &'reg Registries,
}
