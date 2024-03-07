use core::fmt;

use bevy::math::uvec3;
use itertools::Itertools;

use crate::{
    data::{
        registries::{block::BlockVariantRegistry, Registry},
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

impl fmt::Debug for BlockVoxel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct FullBlock {
    pub rotation: Option<BlockModelRotation>,
    pub block: <BlockVariantRegistry as Registry>::Id,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct SubdividedBlock {
    pub rotations: Cubic<{ SUBDIVISIONS_USIZE }, Option<BlockModelRotation>>,
    pub data: Cubic<{ SUBDIVISIONS_USIZE }, <BlockVariantRegistry as Registry>::Id>,
}

impl SubdividedBlock {
    /// Test if all subblocks are the same in this block (i.e., it's basically a full block)
    #[inline]
    pub fn is_equichromatic(&self) -> bool {
        self.data.flattened().iter().all_equal() && self.rotations.flattened().iter().all_equal()
    }

    /// Try to "merge" all the subblocks into a full block,
    /// returns `None` if the subblocks were not all identical
    #[inline]
    pub fn coalesce(&self) -> Option<FullBlock> {
        if self.is_equichromatic() {
            let block = *self.data.get(uvec3(0, 0, 0)).unwrap();
            let rotation = *self.rotations.get(uvec3(0, 0, 0)).unwrap();

            Some(FullBlock { rotation, block })
        } else {
            None
        }
    }
}
