use bevy::math::uvec3;
use itertools::Itertools;

use crate::{
    data::{
        registries::{variant::BlockVariantRegistry, Registry},
        voxel::rotations::BlockModelRotation,
    },
    util::cubic::Cubic,
};

pub const SUBDIVISIONS: u32 = 4;
pub const SUBDIVISIONS_USIZE: usize = SUBDIVISIONS as usize;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum BlockVoxel {
    Full(FullBlock),
    Subdivided(SubdividedBlock),
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct FullBlock {
    pub rotation: Option<BlockModelRotation>,
    pub block: <BlockVariantRegistry as Registry>::Id,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct SubdividedBlock {
    pub rotation: Option<BlockModelRotation>,
    pub data: Cubic<{ SUBDIVISIONS_USIZE }, <BlockVariantRegistry as Registry>::Id>,
}

impl SubdividedBlock {
    pub fn coalesce(&self) -> Option<FullBlock> {
        if self.data.flattened().iter().all_equal() {
            let block = *self.data.get(uvec3(0, 0, 0)).unwrap();

            Some(FullBlock {
                rotation: self.rotation,
                block,
            })
        } else {
            None
        }
    }
}