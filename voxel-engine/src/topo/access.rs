use bevy::prelude::*;
use std::error::Error;

use super::bounding_box::BoundingBox;
use super::chunk::Chunk;
use super::chunk_ref::ChunkVoxelOutput;

pub trait ChunkAccess<'access>
where
    Self: ReadAccess<ReadType<'access> = ChunkVoxelOutput<'access>>,
    Self: ChunkBounds + 'access,
{
}

impl<'a, T: 'a> ChunkAccess<'a> for T where
    T: ReadAccess<ReadType<'a> = ChunkVoxelOutput<'a>> + ChunkBounds
{
}

pub trait ReadAccess {
    type ReadType<'a>
    where
        Self: 'a;
    type ReadErr: Error + 'static;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType<'_>, Self::ReadErr>;
}

pub trait WriteAccess {
    type WriteType;
    type WriteErr: Error + 'static;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr>;
}

pub trait HasBounds {
    fn bounds(&self) -> BoundingBox;
}

pub trait ChunkBounds: HasBounds {}

impl<T: ChunkBounds> HasBounds for T {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}
