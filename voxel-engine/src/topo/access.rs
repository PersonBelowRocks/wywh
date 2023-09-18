use bevy::prelude::*;
use std::error::Error;

use super::bounding_box::BoundingBox;

pub trait ReadAccess {
    type ReadType;
    type ReadErr: Error;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr>;
}

pub trait WriteAccess {
    type WriteType;
    type WriteErr: Error;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr>;
}

pub trait HasBounds {
    fn bounds(&self) -> BoundingBox;
}

pub trait GeneralAccess:
    ReadAccess<ReadType = Self::DataType> + WriteAccess<WriteType = Self::DataType> + HasBounds
{
    type DataType;
}

impl<V, DT> GeneralAccess for V
where
    V: WriteAccess<WriteType = DT> + ReadAccess<ReadType = DT> + HasBounds,
{
    type DataType = DT;
}
