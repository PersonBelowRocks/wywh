use bevy::prelude::*;
use std::error::Error;

use super::bounding_box::BoundingBox;
use super::chunk::Chunk;
use super::chunk_ref::ChunkVoxelOutput;

pub trait ChunkAccess<'a>
where
    Self: ReadAccess<ReadType = ChunkVoxelOutput<'a>>,
    Self: ChunkBounds,
{
}

impl<'a, T> ChunkAccess<'a> for T where T: ReadAccess<ReadType = ChunkVoxelOutput<'a>> + ChunkBounds {}

pub trait ReadAccess {
    type ReadType;
    // TODO: create custom access error trait that lets a caller check if the access errored due to an out of bounds position
    type ReadErr: Error + 'static;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr>;
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
