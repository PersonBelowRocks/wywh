use std::hash::{self, BuildHasher};

use bevy::math::IVec3;

use crate::topo::{
    access::{ChunkBounds, ReadAccess, WriteAccess},
    storage::{
        data_structures::{IndexedChunkStorage, LayeredChunkStorage},
        error::OutOfBounds,
    },
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

pub struct SlccAccess<'a, T>(RwLockWriteGuard<'a, LayeredChunkStorage<T>>);

impl<'a, T> ChunkBounds for SlccAccess<'a, T> {}

impl<'a, T: 'a> ReadAccess for SlccAccess<'a, T> {
    type ReadErr = OutOfBounds;
    type ReadType<'b> = Option<&'a T> where 'a: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        todo!() // self.0.get(pos)
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
    type ReadType<'b> = Option<&'a T> where Self: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        todo!() // self.0.get(pos)
    }
}

// SICC for short
pub struct SyncIndexedChunkContainer<T: hash::Hash + Eq, S: BuildHasher = ahash::RandomState>(
    pub(crate) RwLock<IndexedChunkStorage<T, S>>,
);

/// Avoid writing the huge name of SyncIndexedChunkContainer
pub type Sicc<T, S> = SyncIndexedChunkContainer<T, S>;

impl<T: hash::Hash + Eq> SyncIndexedChunkContainer<T, ahash::RandomState> {
    pub fn new() -> Self {
        Self::with_random_state(ahash::RandomState::new())
    }
}

impl<T: hash::Hash + Eq, S: BuildHasher> SyncIndexedChunkContainer<T, S> {
    pub fn with_random_state(random_state: S) -> Self {
        let storage = IndexedChunkStorage::with_random_state(random_state);
        Self(RwLock::new(storage))
    }

    pub fn access(&self) -> SiccAccess<'_, T, S> {
        SiccAccess(self.0.write())
    }

    pub fn read_access(&self) -> SiccReadAccess<'_, T, S> {
        SiccReadAccess(self.0.read())
    }
}

// Does not implement read access due to type system and borrowck shenanigans
pub struct SiccAccess<'a, T: hash::Hash + Eq, S: BuildHasher>(
    RwLockWriteGuard<'a, IndexedChunkStorage<T, S>>,
);

impl<'a, T: hash::Hash + Eq, S: BuildHasher> ChunkBounds for SiccAccess<'a, T, S> {}

impl<'a, T: hash::Hash + Eq, S: BuildHasher> WriteAccess for SiccAccess<'a, T, S> {
    type WriteErr = OutOfBounds;
    type WriteType = Option<T>;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        match data {
            Some(v) => {
                self.0.set(pos, v)?;
            }
            None => {
                self.0.clear(pos)?;
            }
        }

        Ok(())
    }
}

impl<'a, T: hash::Hash + Eq, S: BuildHasher> ReadAccess for SiccAccess<'a, T, S> {
    type ReadErr = OutOfBounds;
    type ReadType<'b> = Option<&'b T> where 'a: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        Ok(self.0.get(pos)?)
    }
}

pub struct SiccReadAccess<'a, T: hash::Hash + Eq, S: BuildHasher>(
    RwLockReadGuard<'a, IndexedChunkStorage<T, S>>,
);

impl<'a, T: hash::Hash + Eq, S: BuildHasher> ChunkBounds for SiccReadAccess<'a, T, S> {}

impl<'a, T: hash::Hash + Eq, S: BuildHasher> ReadAccess for SiccReadAccess<'a, T, S> {
    type ReadErr = OutOfBounds;
    type ReadType<'b> = Option<&'b T> where Self: 'b;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr> {
        Ok(self.0.get(pos)?)
    }
}
