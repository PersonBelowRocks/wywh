use core::fmt;
use std::mem;

use bevy::math::{uvec3, IVec2, UVec3};
use itertools::Itertools;

use crate::{
    data::{
        registries::{block::BlockVariantRegistry, Registry},
        voxel::rotations::BlockModelRotation,
    },
    util::cubic::Cubic,
};

use super::{chunk::Chunk, storage::error::OutOfBounds};

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum BlockVoxel {
    Full(FullBlock),
    Subdivided(SubdividedBlock),
}

impl BlockVoxel {
    pub fn new_full(block: <BlockVariantRegistry as Registry>::Id) -> Self {
        Self::Full(FullBlock {
            rotation: None,
            block,
        })
    }
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
    pub rotations: Cubic<{ Self::SUBDIVISIONS_USIZE }, Option<BlockModelRotation>>,
    pub blocks: Cubic<{ Self::SUBDIVISIONS_USIZE }, <BlockVariantRegistry as Registry>::Id>,
}

#[derive(Copy, Clone, Debug)]
pub struct Microblock {
    pub rotation: Option<BlockModelRotation>,
    pub block: <BlockVariantRegistry as Registry>::Id,
}

impl Microblock {
    pub fn as_full_block(&self) -> FullBlock {
        FullBlock {
            rotation: self.rotation,
            block: self.block,
        }
    }
}

impl fmt::Debug for SubdividedBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SubdividedBlock")
    }
}

impl SubdividedBlock {
    pub const SUBDIVISIONS: i32 = 4;
    pub const SUBDIVISIONS_USIZE: usize = Self::SUBDIVISIONS as usize;
    pub const SUBDIVS_VEC: IVec2 = IVec2::splat(Self::SUBDIVISIONS);

    /// Test if all subblocks are the same in this block (i.e., it's basically a full block)
    #[inline]
    pub fn is_equichromatic(&self) -> bool {
        self.blocks.flattened().iter().all_equal() && self.rotations.flattened().iter().all_equal()
    }

    /// Try to "merge" all the subblocks into a full block,
    /// returns `None` if the subblocks were not all identical
    #[inline]
    pub fn coalesce(&self) -> Option<FullBlock> {
        if self.is_equichromatic() {
            let block = *self.blocks.get(uvec3(0, 0, 0)).unwrap();
            let rotation = *self.rotations.get(uvec3(0, 0, 0)).unwrap();

            Some(FullBlock { rotation, block })
        } else {
            None
        }
    }

    /// Get the microblock at the given `pos`, returns `Err(OutOfBounds)` if `pos` is out of bounds
    #[inline]
    pub fn get(&self, pos: UVec3) -> Result<Microblock, OutOfBounds> {
        Ok(Microblock {
            rotation: self.rotations.get(pos).copied()?,
            block: self.blocks.get(pos).copied()?,
        })
    }

    /// Set the microblock at the given `pos`, returns `Err(OutOfBounds)` if the `pos` is out of bounds.
    /// Otherwise returns the old microblock at the position.
    #[inline]
    pub fn set(&mut self, pos: UVec3, mblock: Microblock) -> Result<Microblock, OutOfBounds> {
        let rot_slot = self.rotations.get_mut(pos)?;
        let block_slot = self.blocks.get_mut(pos)?;

        Ok(Microblock {
            rotation: mem::replace(rot_slot, mblock.rotation),
            block: mem::replace(block_slot, mblock.block),
        })
    }
}
