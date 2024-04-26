use std::{
    hash::BuildHasher,
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
};

use bevy::{math::UVec3, prelude::IVec3};

use crate::topo::{
    access::{ChunkBounds, ReadAccess, WriteAccess},
    block::{BlockVoxel, FullBlock, Microblock, SubdividedBlock},
    error::ChunkAccessError,
    storage::{
        containers::data_storage::{SiccAccess, SiccReadAccess},
        error::OutOfBounds,
    },
};

use super::{
    chunk::{Chunk, ChunkPos},
    ChunkManagerError,
};

/// Chunk reference read access
pub type Crra<'a> = ChunkRefReadAccess<'a>;
/// Chunk reference (write) access
pub type Crwa<'a> = ChunkRefAccess<'a>;

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
        F: for<'a> FnOnce(ChunkRefAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkManagerError::Unloaded)?;
        self.treat_as_changed()?;

        let variant_access = chunk.variants.access();

        let x = Ok(f(ChunkRefAccess {
            block_variants: variant_access,
        }));
        x
    }

    #[allow(clippy::let_and_return)]
    pub fn with_read_access<F, U>(&self, f: F) -> Result<U, ChunkManagerError>
    where
        F: for<'a> FnOnce(ChunkRefReadAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkManagerError::Unloaded)?;

        let block_variant_access = chunk.variants.read_access();

        let x = Ok(f(ChunkRefReadAccess {
            block_variants: block_variant_access,
        }));
        x
    }
}

pub struct ChunkRefReadAccess<'a, S: BuildHasher = ahash::RandomState> {
    pub(crate) block_variants: SiccReadAccess<'a, BlockVoxel, S>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CaoBlock<'a> {
    Full(FullBlock),
    Subdivided(&'a SubdividedBlock),
}

impl<'a> CaoBlock<'a> {
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

pub enum MutCaoBlock<'a> {
    Full(&'a mut FullBlock),
    Subdivided(&'a mut SubdividedBlock),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChunkAccessOutput<'a> {
    pub block: CaoBlock<'a>,
}

impl<'a> ChunkAccessOutput<'a> {
    pub fn new(block: &'a BlockVoxel) -> Self {
        match block {
            BlockVoxel::Full(block) => Self {
                block: CaoBlock::Full(*block),
            },
            BlockVoxel::Subdivided(subdiv) => Self {
                block: CaoBlock::Subdivided(subdiv),
            },
        }
    }
}

#[derive(Clone, Debug, dm::Constructor)]
pub struct ChunkAccessInput {
    pub block: BlockVoxel,
}

pub struct MutChunkAccOutput<'a> {
    pub block: MutCaoBlock<'a>,
}

pub struct ChunkRefAccess<'a, S: BuildHasher = ahash::RandomState> {
    pub(crate) block_variants: SiccAccess<'a, BlockVoxel, S>,
}

impl<'a, S: BuildHasher + Clone> ChunkRefAccess<'a, S> {
    pub(crate) fn get_mutable_output(
        &mut self,
        pos: IVec3,
    ) -> Result<MutChunkAccOutput<'_>, ChunkAccessError> {
        let block = self
            .block_variants
            .get_mut(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        let output = match block {
            BlockVoxel::Full(full) => MutCaoBlock::Full(full),
            BlockVoxel::Subdivided(subdiv) => MutCaoBlock::Subdivided(subdiv),
        };

        Ok(MutChunkAccOutput { block: output })
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

impl<'a, S: BuildHasher> WriteAccess for ChunkRefAccess<'a, S> {
    type WriteErr = ChunkAccessError;
    type WriteType = ChunkAccessInput;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.block_variants.set(pos, Some(data.block))?;

        Ok(())
    }
}

impl<'a, S: BuildHasher> ReadAccess for ChunkRefAccess<'a, S> {
    type ReadErr = ChunkAccessError;
    type ReadType<'b> = ChunkAccessOutput<'b> where Self: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        let block = self
            .block_variants
            .get(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        Ok(ChunkAccessOutput::new(block))
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefAccess<'a, S> {}

impl<'a, S: BuildHasher> ReadAccess for ChunkRefReadAccess<'a, S> {
    type ReadErr = ChunkAccessError;
    type ReadType<'b> = ChunkAccessOutput<'b> where Self: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        let block = self
            .block_variants
            .get(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        Ok(ChunkAccessOutput::new(block))
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefReadAccess<'a, S> {}
