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
    tile::Transparency,
    voxel::rotations::BlockModelRotation,
};

use super::{
    access::{ChunkBounds, ReadAccess, WriteAccess},
    chunk::{Chunk, ChunkPos, VoxelVariantData},
    error::{ChunkAccessError, ChunkRefAccessError},
    storage::containers::{
        data_storage::{SiccAccess, SiccReadAccess},
        dense::{SyncDenseContainerAccess, SyncDenseContainerReadAccess},
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

    pub fn treat_as_changed(&self) -> Result<(), ChunkRefAccessError> {
        let changed = self
            .changed
            .upgrade()
            .ok_or(ChunkRefAccessError::Unloaded)?;
        changed.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[allow(clippy::let_and_return)] // We need do to this little crime so the borrowchecker doesn't yell at us
    pub fn with_access<F, U>(&self, f: F) -> Result<U, ChunkRefAccessError>
    where
        F: for<'a> FnOnce(ChunkRefVxlAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkRefAccessError::Unloaded)?;
        self.treat_as_changed()?;

        let transparency_access = chunk.transparency.access();
        let variant_access = chunk.variants.access();

        let x = Ok(f(ChunkRefVxlAccess {
            transparency: transparency_access,
            variants: variant_access,
        }));
        x
    }

    #[allow(clippy::let_and_return)]
    pub fn with_read_access<F, U>(&self, f: F) -> Result<U, ChunkRefAccessError>
    where
        F: for<'a> FnOnce(ChunkRefVxlReadAccess<'a, ahash::RandomState>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkRefAccessError::Unloaded)?;

        let voxel_access = chunk.transparency.read_access();
        let variant_access = chunk.variants.read_access();

        let x = Ok(f(ChunkRefVxlReadAccess {
            transparency: voxel_access,
            variants: variant_access,
        }));
        x
    }
}

pub struct ChunkRefVxlReadAccess<'a, S: BuildHasher> {
    transparency: SyncDenseContainerReadAccess<'a, Transparency>,
    variants: SiccReadAccess<'a, VoxelVariantData, S>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChunkVoxelOutput {
    pub transparency: Transparency,
    pub variant: RegistryId<VariantRegistry>,
    pub rotation: Option<BlockModelRotation>,
}

#[derive(Copy, Clone, Debug)]
pub struct ChunkVoxelInput {
    pub transparency: Transparency,
    pub variant: RegistryId<VariantRegistry>,
    pub rotation: Option<BlockModelRotation>,
}

pub struct ChunkRefVxlAccess<'a, S: BuildHasher> {
    transparency: SyncDenseContainerAccess<'a, Transparency>,
    variants: SiccAccess<'a, VoxelVariantData, S>,
}

impl<'a, S: BuildHasher> WriteAccess for ChunkRefVxlAccess<'a, S> {
    type WriteErr = ChunkAccessError;
    type WriteType = ChunkVoxelInput;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.transparency.set(pos, data.transparency)?;
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
            transparency: self.transparency.get(pos)?,
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
            transparency: self.transparency.get(pos)?,
            variant: variant_data.variant,
            rotation: variant_data.rotation,
        })
    }
}

impl<'a, S: BuildHasher> ChunkBounds for ChunkRefVxlReadAccess<'a, S> {}
