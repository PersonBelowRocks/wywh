use std::ops::DerefMut;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use bevy::prelude::*;

use crate::topo::access::{HasBounds, ReadAccess, WriteAccess};
use crate::topo::bounding_box::BoundingBox;
use crate::topo::chunk::Chunk;
use crate::topo::error::ChunkVoxelAccessError;
use crate::util;

use super::data_structures::DenseChunkStorage;

impl<T> HasBounds for DenseChunkStorage<T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}

impl<T> WriteAccess for DenseChunkStorage<T> {
    type WriteType = T;
    type WriteErr = ChunkVoxelAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        let idx =
            util::try_ivec3_to_usize_arr(pos).map_err(|_| ChunkVoxelAccessError::OutOfBounds)?;
        let slot = self
            .get_mut(idx)
            .ok_or(ChunkVoxelAccessError::OutOfBounds)?;

        *slot = data;
        Ok(())
    }
}

impl<T: Copy> ReadAccess for DenseChunkStorage<T> {
    type ReadType = T;
    type ReadErr = ChunkVoxelAccessError;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        let idx =
            util::try_ivec3_to_usize_arr(pos).map_err(|_| ChunkVoxelAccessError::OutOfBounds)?;
        self.get_ref(idx)
            .ok_or(ChunkVoxelAccessError::OutOfBounds)
            .cloned()
    }
}

#[derive(Clone)]
pub enum RawChunkVoxelContainer<T> {
    Filled(Box<DenseChunkStorage<T>>),
    Empty,
}

pub struct AutoChunkVoxelContainerAccess<'a, T: Copy> {
    container: &'a mut RawChunkVoxelContainer<T>,
    default: T,
}

impl<'a, T: Copy> ReadAccess for AutoChunkVoxelContainerAccess<'a, T> {
    type ReadErr = ChunkVoxelAccessError;
    type ReadType = T;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        if !Chunk::BOUNDING_BOX.contains(pos) {
            Err(ChunkVoxelAccessError::OutOfBounds)?
        }

        match &self.container {
            RawChunkVoxelContainer::Empty => Ok(self.default),
            RawChunkVoxelContainer::Filled(storage) => storage.get(pos),
        }
    }
}

impl<'a, T: Copy> WriteAccess for AutoChunkVoxelContainerAccess<'a, T> {
    type WriteErr = ChunkVoxelAccessError;
    type WriteType = T;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        match self.container {
            RawChunkVoxelContainer::Empty => {
                let mut storage = Box::new(DenseChunkStorage::new(self.default));
                storage.set(pos, data)?;
                *self.container = RawChunkVoxelContainer::Filled(storage);
                Ok(())
            }
            RawChunkVoxelContainer::Filled(ref mut storage) => storage.set(pos, data),
        }
    }
}

impl<'a, T: Copy> HasBounds for AutoChunkVoxelContainerAccess<'a, T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}

impl<T: Copy> RawChunkVoxelContainer<T> {
    pub fn filled(data: DenseChunkStorage<T>) -> Self {
        Self::Filled(Box::new(data))
    }

    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn fill(&mut self, data: DenseChunkStorage<T>) {
        match self {
            Self::Empty => *self = Self::Filled(Box::new(data)),
            Self::Filled(b) => *b.deref_mut() = data,
        }
    }

    pub(crate) fn internal_set(
        &mut self,
        pos: IVec3,
        data: T,
    ) -> Result<(), ChunkVoxelAccessError> {
        match self {
            Self::Empty => Err(ChunkVoxelAccessError::NotInitialized),
            Self::Filled(b) => b.set(pos, data),
        }
    }

    pub(crate) fn internal_get(&self, pos: IVec3) -> Result<T, ChunkVoxelAccessError> {
        match self {
            Self::Empty => Err(ChunkVoxelAccessError::NotInitialized),
            Self::Filled(b) => b.get(pos),
        }
    }

    pub fn auto_access(&mut self, default: T) -> AutoChunkVoxelContainerAccess<'_, T> {
        AutoChunkVoxelContainerAccess {
            container: self,
            default,
        }
    }
}

pub struct SyncChunkVoxelContainer<T>(pub(crate) RwLock<RawChunkVoxelContainer<T>>);

pub struct SyncChunkVoxelContainerAccess<'a, T: Copy>(
    RwLockWriteGuard<'a, RawChunkVoxelContainer<T>>,
);

pub struct SyncChunkVoxelContainerReadAccess<'a, T: Copy>(
    RwLockReadGuard<'a, RawChunkVoxelContainer<T>>,
);

impl<T: Copy> SyncChunkVoxelContainer<T> {
    pub fn empty() -> Self {
        Self(RwLock::new(RawChunkVoxelContainer::Empty))
    }

    pub fn new(data: DenseChunkStorage<T>) -> Self {
        Self(RwLock::new(RawChunkVoxelContainer::filled(data)))
    }

    pub fn access(&self) -> SyncChunkVoxelContainerAccess<'_, T> {
        SyncChunkVoxelContainerAccess(self.0.write().unwrap())
    }

    pub fn read_access(&self) -> SyncChunkVoxelContainerReadAccess<'_, T> {
        SyncChunkVoxelContainerReadAccess(self.0.read().unwrap())
    }
}

impl<'a, T: Copy> ReadAccess for SyncChunkVoxelContainerAccess<'a, T> {
    type ReadType = T;
    type ReadErr = <DenseChunkStorage<T> as ReadAccess>::ReadErr;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        self.0.internal_get(pos)
    }
}

impl<'a, T: Copy> WriteAccess for SyncChunkVoxelContainerAccess<'a, T> {
    type WriteType = T;
    type WriteErr = <DenseChunkStorage<T> as WriteAccess>::WriteErr;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.0.internal_set(pos, data)
    }
}

impl<'a, T: Copy> HasBounds for SyncChunkVoxelContainerAccess<'a, T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}

impl<'a, T: Copy> ReadAccess for SyncChunkVoxelContainerReadAccess<'a, T> {
    type ReadType = T;
    type ReadErr = <DenseChunkStorage<T> as ReadAccess>::ReadErr;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        self.0.internal_get(pos)
    }
}

impl<'a, T: Copy> HasBounds for SyncChunkVoxelContainerReadAccess<'a, T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}
