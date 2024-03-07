use std::ops::DerefMut;

use bevy::prelude::*;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::topo::access::{HasBounds, ReadAccess, WriteAccess};
use crate::topo::bounding_box::BoundingBox;
use crate::topo::chunk::Chunk;
use crate::topo::error::ChunkAccessError;
use crate::util;

use super::super::data_structures::DenseChunkStorage;

impl<T> HasBounds for DenseChunkStorage<T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}

impl<T> WriteAccess for DenseChunkStorage<T> {
    type WriteType = T;
    type WriteErr = ChunkAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        let idx = util::try_ivec3_to_usize_arr(pos).map_err(|_| ChunkAccessError::OutOfBounds)?;
        let slot = self.get_mut(idx).ok_or(ChunkAccessError::OutOfBounds)?;

        *slot = data;
        Ok(())
    }
}

#[derive(Clone)]
pub enum DenseChunkContainer<T> {
    Filled(Box<DenseChunkStorage<T>>),
    Empty,
}

pub struct AutoDenseContainerAccess<'a, T> {
    container: &'a mut DenseChunkContainer<T>,
    default: T,
}

impl<'a, T> AutoDenseContainerAccess<'a, T> {
    pub fn new(container: &'a mut DenseChunkContainer<T>, default: T) -> Self {
        Self { container, default }
    }
}

impl<'a, T> ReadAccess for AutoDenseContainerAccess<'a, T> {
    type ReadErr = ChunkAccessError;
    type ReadType = &'a T;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        if !Chunk::BOUNDING_BOX.contains(pos) {
            Err(ChunkAccessError::OutOfBounds)?
        }

        match &self.container {
            DenseChunkContainer::Empty => Ok(&self.default),
            DenseChunkContainer::Filled(storage) => todo!(), // storage.get(pos),
        }
    }
}

impl<'a, T: Copy> WriteAccess for AutoDenseContainerAccess<'a, T> {
    type WriteErr = ChunkAccessError;
    type WriteType = T;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        match self.container {
            DenseChunkContainer::Empty => {
                let mut storage = Box::new(DenseChunkStorage::new(self.default));
                storage.set(pos, data)?;
                *self.container = DenseChunkContainer::Filled(storage);
                Ok(())
            }
            DenseChunkContainer::Filled(ref mut storage) => storage.set(pos, data),
        }
    }
}

impl<'a, T: Copy> HasBounds for AutoDenseContainerAccess<'a, T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}

impl<T: Copy> DenseChunkContainer<T> {
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

    pub(crate) fn internal_set(&mut self, pos: IVec3, data: T) -> Result<(), ChunkAccessError> {
        match self {
            Self::Empty => Err(ChunkAccessError::NotInitialized),
            Self::Filled(b) => b.set(pos, data),
        }
    }

    pub(crate) fn internal_get(&self, pos: IVec3) -> Result<T, ChunkAccessError> {
        match self {
            Self::Empty => Err(ChunkAccessError::NotInitialized),
            Self::Filled(b) => todo!(), // b.get(pos),
        }
    }

    pub fn auto_access(&mut self, default: T) -> AutoDenseContainerAccess<'_, T> {
        AutoDenseContainerAccess {
            container: self,
            default,
        }
    }
}

pub struct SyncDenseChunkContainer<T>(pub(crate) RwLock<DenseChunkContainer<T>>);

pub struct SyncDenseContainerAccess<'a, T: Copy>(RwLockWriteGuard<'a, DenseChunkContainer<T>>);

pub struct SyncDenseContainerReadAccess<'a, T: Copy>(RwLockReadGuard<'a, DenseChunkContainer<T>>);

impl<T: Copy> SyncDenseChunkContainer<T> {
    pub fn empty() -> Self {
        Self(RwLock::new(DenseChunkContainer::Empty))
    }

    pub fn new(data: DenseChunkStorage<T>) -> Self {
        Self(RwLock::new(DenseChunkContainer::filled(data)))
    }

    pub fn access(&self) -> SyncDenseContainerAccess<'_, T> {
        SyncDenseContainerAccess(self.0.write())
    }

    pub fn read_access(&self) -> SyncDenseContainerReadAccess<'_, T> {
        SyncDenseContainerReadAccess(self.0.read())
    }
}

impl<'a, T: Copy> HasBounds for SyncDenseContainerReadAccess<'a, T> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}
