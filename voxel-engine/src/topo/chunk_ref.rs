use std::{
    hash::BuildHasher,
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
};

use bevy::prelude::IVec3;

use crate::data::{
    registries::{variant::VariantRegistry, RegistryId},
    voxel::rotations::BlockModelRotation,
};

use super::{
    access::{ChunkBounds, ReadAccess, WriteAccess},
    chunk::{Chunk, ChunkPos, VoxelVariantData},
    error::{ChunkAccessError, ChunkManagerError},
    storage::containers::data_storage::{SiccAccess, SiccReadAccess},
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
            variants: variant_access,
        }));
        x
    }

    #[allow(clippy::let_and_return)]
    pub fn with_read_access<F, U>(&self, f: F) -> Result<U, ChunkManagerError>
    where
        F: for<'a> FnOnce(ChunkRefVxlReadAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkManagerError::Unloaded)?;

        let variant_access = chunk.variants.read_access();

        let x = Ok(f(ChunkRefVxlReadAccess {
            variants: variant_access,
        }));
        x
    }
}

pub struct ChunkRefVxlReadAccess<'a, S: BuildHasher = ahash::RandomState> {
    pub(crate) variants: SiccReadAccess<'a, VoxelVariantData, S>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChunkVoxelOutput {
    pub variant: RegistryId<VariantRegistry>,
    pub rotation: Option<BlockModelRotation>,
}

#[derive(Copy, Clone, Debug)]
pub struct ChunkVoxelInput {
    pub variant: RegistryId<VariantRegistry>,
    pub rotation: Option<BlockModelRotation>,
}

pub struct ChunkRefVxlAccess<'a, S: BuildHasher = ahash::RandomState> {
    variants: SiccAccess<'a, VoxelVariantData, S>,
}

impl<'a, S: BuildHasher> WriteAccess for ChunkRefVxlAccess<'a, S> {
    type WriteErr = ChunkAccessError;
    type WriteType = ChunkVoxelInput;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.variants.set(
            pos,
            Some(VoxelVariantData::new(data.variant, data.rotation)),
        )?;

        Ok(())
    }
}

impl<'a, S: BuildHasher> ReadAccess for ChunkRefVxlAccess<'a, S> {
    type ReadErr = ChunkAccessError;
    type ReadType = ChunkVoxelOutput;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        let variant_data = self
            .variants
            .get(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        Ok(ChunkVoxelOutput {
            variant: variant_data.variant,
            rotation: variant_data.rotation,
        })
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefVxlAccess<'a, S> {}

impl<'a, S: BuildHasher> ReadAccess for ChunkRefVxlReadAccess<'a, S> {
    type ReadErr = ChunkAccessError;
    type ReadType = ChunkVoxelOutput;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        let variant_data = self
            .variants
            .get(pos)?
            .ok_or(ChunkAccessError::NotInitialized)?;

        Ok(ChunkVoxelOutput {
            variant: variant_data.variant,
            rotation: variant_data.rotation,
        })
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefVxlReadAccess<'a, S> {}
