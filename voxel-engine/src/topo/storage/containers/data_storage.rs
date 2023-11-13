use bevy::math::IVec3;

use crate::topo::{
    access::{ChunkBounds, ReadAccess, WriteAccess},
    storage::{data_structures::LayeredChunkStorage, error::OutOfBounds},
};

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// SLCC for short
pub struct SyncLayeredChunkContainer<T>(RwLock<LayeredChunkStorage<T>>);

impl<T> SyncLayeredChunkContainer<T> {
    pub fn new() -> Self {
        Self(RwLock::new(LayeredChunkStorage::new()))
    }

    pub fn access(&self) -> SlccAccess<'_, T>
    where
        T: Copy,
    {
        SlccAccess(self.0.write())
    }

    pub fn read_access(&self) -> SlccReadAccess<'_, T>
    where
        T: Copy,
    {
        SlccReadAccess(self.0.read())
    }
}

pub struct SlccAccess<'a, T: Copy>(RwLockWriteGuard<'a, LayeredChunkStorage<T>>);

impl<'a, T: Copy> ChunkBounds for SlccAccess<'a, T> {}

impl<'a, T: Copy> ReadAccess for SlccAccess<'a, T> {
    type ReadErr = OutOfBounds;
    type ReadType = Option<T>;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        self.0.get(pos)
    }
}

impl<'a, T: Copy> WriteAccess for SlccAccess<'a, T> {
    type WriteErr = OutOfBounds;
    type WriteType = Option<T>;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        match data {
            Some(v) => self.0.set(pos, v)?,
            None => self.0.clear(pos)?,
        }

        Ok(())
    }
}

pub struct SlccReadAccess<'a, T: Copy>(RwLockReadGuard<'a, LayeredChunkStorage<T>>);

impl<'a, T: Copy> ChunkBounds for SlccReadAccess<'a, T> {}

impl<'a, T: Copy> ReadAccess for SlccReadAccess<'a, T> {
    type ReadErr = OutOfBounds;
    type ReadType = Option<T>;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        self.0.get(pos)
    }
}
