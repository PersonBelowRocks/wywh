use std::{
    fmt,
    hash::BuildHasher,
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
};

use bevy::{math::UVec3, prelude::IVec3};

use crate::data::{
    registries::{block::BlockVariantRegistry, Registry},
    voxel::rotations::BlockModelRotation,
};

use super::{
    access::{ChunkBounds, ReadAccess, WriteAccess},
    block::{BlockVoxel, FullBlock, Microblock, SubdividedBlock},
    chunk::{Chunk, ChunkPos, VoxelVariantData},
    error::{ChunkAccessError, ChunkManagerError},
    storage::{
        containers::data_storage::{SiccAccess, SiccReadAccess},
        error::OutOfBounds,
    },
};

#[derive(Clone)]
pub struct ChunkRef {
    pub(crate) chunk: Weak<Chunk>,
    pub(crate) changed: Weak<AtomicBool>,
    pub(crate) pos: ChunkPos,
}

impl ChunkRef {
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn treat_as_changed(&self) -> Result<(), ChunkManagerError> {
        let changed = self.changed.upgrade().ok_or(ChunkManagerError::Unloaded)?;
        changed.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[allow(clippy::let_and_return)] // We need do to this little crime so the borrowchecker doesn't yell at us
    pub fn with_access<F, U>(&self, f: F) -> Result<U, ChunkManagerError>
    where
        F: for<'a> FnOnce(ChunkRefVxlAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkManagerError::Unloaded)?;
        self.treat_as_changed()?;

        let variant_access = chunk.variants.access();

        let x = Ok(f(ChunkRefVxlAccess {
            block_variants: variant_access,
        }));
        x
    }

    #[allow(clippy::let_and_return)]
    pub fn with_read_access<F, U>(&self, f: F) -> Result<U, ChunkManagerError>
    where
        F: for<'a> FnOnce(ChunkRefVxlReadAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkManagerError::Unloaded)?;

        let block_variant_access = chunk.variants.read_access();

        let x = Ok(f(ChunkRefVxlReadAccess {
            block_variants: block_variant_access,
        }));
        x
    }
}

pub struct ChunkRefVxlReadAccess<'a, S: BuildHasher = ahash::RandomState> {
    pub(crate) block_variants: SiccReadAccess<'a, BlockVoxel, S>,
}

pub type CrVra<'a> = ChunkRefVxlReadAccess<'a>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CvoBlock<'a> {
    Full(FullBlock),
    Subdivided(&'a SubdividedBlock),
}

impl<'a> CvoBlock<'a> {
    pub fn get_microblock(&self, pos: UVec3) -> Result<Microblock, OutOfBounds> {
        if SubdividedBlock::contains(pos) {
            Ok(match self {
                Self::Full(block) => Microblock {
                    rotation: block.rotation,
                    id: block.id,
                },
                Self::Subdivided(block) => block.get(pos)?,
            })
        } else {
            Err(OutOfBounds)
        }
    }
}

pub enum MutCvoBlock<'a> {
    Full(&'a mut FullBlock),
    Subdivided(&'a mut SubdividedBlock),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChunkVoxelOutput<'a> {
    pub block: CvoBlock<'a>,
}

impl<'a> ChunkVoxelOutput<'a> {
    pub fn new(block: &'a BlockVoxel) -> Self {
        match block {
            BlockVoxel::Full(block) => Self {
                block: CvoBlock::Full(*block),
            },
            BlockVoxel::Subdivided(subdiv) => Self {
                block: CvoBlock::Subdivided(subdiv),
            },
        }
    }
}

#[derive(Clone, Debug, dm::Constructor)]
pub struct ChunkVoxelInput {
    pub block: BlockVoxel,
}

pub struct MutChunkVxlOutput<'a> {
    pub block: MutCvoBlock<'a>,
}

pub struct ChunkRefVxlAccess<'a, S: BuildHasher = ahash::RandomState> {
    pub(crate) block_variants: SiccAccess<'a, BlockVoxel, S>,
}

impl<'a, S: BuildHasher + Clone> ChunkRefVxlAccess<'a, S> {
    pub(crate) fn get_mutable_output(
        &mut self,
        pos: IVec3,
    ) -> Result<MutChunkVxlOutput<'_>, ChunkAccessError> {
        let block = self
            .block_variants
            .get_mut(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        let output = match block {
            BlockVoxel::Full(full) => MutCvoBlock::Full(full),
            BlockVoxel::Subdivided(subdiv) => MutCvoBlock::Subdivided(subdiv),
        };

        Ok(MutChunkVxlOutput { block: output })
    }

    pub fn coalesce_microblocks(&mut self) -> usize {
        let mut coalesced = 0;

        for value in self.block_variants.values_mut() {
            if let BlockVoxel::Subdivided(subdiv) = value {
                if let Some(full) = subdiv.coalesce() {
                    coalesced += 1;
                    *value = BlockVoxel::Full(full);
                }
            }
        }

        coalesced
    }

    pub fn optimize_internal_storage(&mut self) -> usize {
        self.block_variants.optimize_storage()
    }
}

impl<'a, S: BuildHasher> WriteAccess for ChunkRefVxlAccess<'a, S> {
    type WriteErr = ChunkAccessError;
    type WriteType = ChunkVoxelInput;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.block_variants.set(pos, Some(data.block))?;

        Ok(())
    }
}

impl<'a, S: BuildHasher> ReadAccess for ChunkRefVxlAccess<'a, S> {
    type ReadErr = ChunkAccessError;
    type ReadType<'b> = ChunkVoxelOutput<'b> where Self: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        let block = self
            .block_variants
            .get(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        Ok(ChunkVoxelOutput::new(block))
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefVxlAccess<'a, S> {}

impl<'a, S: BuildHasher> ReadAccess for ChunkRefVxlReadAccess<'a, S> {
    type ReadErr = ChunkAccessError;
    type ReadType<'b> = ChunkVoxelOutput<'b> where Self: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        let block = self
            .block_variants
            .get(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        Ok(ChunkVoxelOutput::new(block))
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefVxlReadAccess<'a, S> {}
