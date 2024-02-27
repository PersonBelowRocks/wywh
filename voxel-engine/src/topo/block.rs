use crate::{
    data::{
        registries::{variant::BlockVariantRegistry, Registry},
        voxel::rotations::BlockModelRotation,
    },
    util::cubic::Cubic,
};

pub const SUBDIVISIONS: u32 = 4;
pub const SUBDIVISIONS_USIZE: usize = SUBDIVISIONS as usize;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum BlockVoxel {
    Full(FullBlock),
    Subdivided(SubdividedBlock),
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct FullBlock {
    rotation: Option<BlockModelRotation>,
    block: <BlockVariantRegistry as Registry>::Id,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct SubdividedBlock {
    rotation: Option<BlockModelRotation>,
    data: Cubic<{ SUBDIVISIONS_USIZE }, <BlockVariantRegistry as Registry>::Id>,
}
