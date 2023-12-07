use std::{iter::Zip, ops::RangeInclusive};

use bevy::{
    math::ivec3,
    prelude::{Event, IVec3},
};
use noise::{NoiseFn, Perlin};
use parking_lot::RwLock;

use crate::data::{
    registries::{variant::VariantRegistry, Registries, Registry, RegistryId},
    tile::{Face, Transparency},
    voxel::rotations::BlockModelRotation,
};

use super::{
    access::{ChunkBounds, HasBounds, WriteAccess},
    chunk::{Chunk, ChunkPos, VariantType, VoxelVariantData},
    chunk_ref::ChunkVoxelInput,
    error::{ChunkAccessError, GeneratorError},
    storage::{
        containers::{
            data_storage::SyncIndexedChunkContainer,
            dense::{AutoDenseContainerAccess, DenseChunkContainer, SyncDenseChunkContainer},
        },
        data_structures::IndexedChunkStorage,
    },
};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum GeneratorChoice {
    Default,
}

pub struct GeneratorInputAccess<'a> {
    transparency: AutoDenseContainerAccess<'a, Transparency>,
    variants: &'a mut IndexedChunkStorage<VoxelVariantData>,
}

impl<'a> ChunkBounds for GeneratorInputAccess<'a> {}

impl<'a> WriteAccess for GeneratorInputAccess<'a> {
    type WriteType = ChunkVoxelInput;
    type WriteErr = ChunkAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.transparency.set(pos, data.transparency)?;
        self.variants
            .set(pos, VoxelVariantData::new(data.variant, data.rotation))?;

        Ok(())
    }
}

pub struct GeneratorInput {
    transparency: DenseChunkContainer<Transparency>,
    variants: IndexedChunkStorage<VoxelVariantData>,
}

impl GeneratorInput {
    pub fn new() -> Self {
        Self {
            transparency: DenseChunkContainer::Empty,
            variants: IndexedChunkStorage::new(),
        }
    }

    pub fn access(&mut self) -> GeneratorInputAccess<'_> {
        GeneratorInputAccess {
            transparency: AutoDenseContainerAccess::new(
                &mut self.transparency,
                Transparency::Transparent,
            ),
            variants: &mut self.variants,
        }
    }

    pub fn to_chunk(self) -> Chunk {
        Chunk {
            transparency: SyncDenseChunkContainer(RwLock::new(self.transparency)),
            variants: SyncIndexedChunkContainer(RwLock::new(self.variants)),
        }
    }
}

#[derive(Event, Debug)]
pub struct GenerateChunk {
    pub pos: ChunkPos,
    pub generator: GeneratorChoice,
}

#[derive(Copy, Clone)]
struct GeneratorPalette {
    void: RegistryId<VariantRegistry>,
    debug: RegistryId<VariantRegistry>,
}

#[derive(Clone)]
pub struct Generator {
    palette: GeneratorPalette,
    default_rotation: BlockModelRotation,
    noise: Perlin,
    scale: f64,
    positions: hb::HashMap<IVec3, ChunkVoxelInput>,
}

impl Generator {
    pub fn new(seed: u32, registries: &Registries) -> Self {
        let _noise = Perlin::new(seed);
        let variants = registries.get_registry::<VariantRegistry>().unwrap();

        let palette = GeneratorPalette {
            void: variants.get_id("void").unwrap(),
            debug: variants.get_id("debug").unwrap(),
        };

        let positions = [
            (
                ivec3(0, 0, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::North, Face::Top).unwrap()),
                },
            ),
            (
                ivec3(2, 0, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::East, Face::Top).unwrap()),
                },
            ),
            (
                ivec3(4, 0, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::South, Face::Top).unwrap()),
                },
            ),
            (
                ivec3(6, 0, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::West, Face::Top).unwrap()),
                },
            ),
            // pitching
            (
                ivec3(0, 8, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::North, Face::Top).unwrap()),
                },
            ),
            (
                ivec3(2, 8, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::Top, Face::South).unwrap()),
                },
            ),
            (
                ivec3(4, 8, 0),
                ChunkVoxelInput {
                    transparency: Transparency::Opaque,
                    variant: palette.debug,
                    rotation: Some(BlockModelRotation::new(Face::Bottom, Face::North).unwrap()),
                },
            ),
        ];

        Self {
            palette,
            default_rotation: BlockModelRotation::new(Face::North, Face::Top).unwrap(),
            noise: Perlin::new(seed),
            scale: 0.1,
            positions: hb::HashMap::from_iter(positions),
        }
    }

    fn zipped_coordinate_iter(
        ws_min: i32,
        ws_max: i32,
    ) -> Zip<RangeInclusive<i32>, RangeInclusive<i32>> {
        (0..=(Chunk::SIZE - 1)).zip(ws_min..=ws_max)
    }

    #[inline]
    pub fn write_to_chunk<Acc>(
        &self,
        cs_pos: ChunkPos,
        access: &mut Acc,
    ) -> Result<(), GeneratorError<Acc::WriteErr>>
    where
        Acc: WriteAccess<WriteType = ChunkVoxelInput> + ChunkBounds,
    {
        const THRESHOLD: f64 = 0.5;

        if !access.bounds().is_chunk() {
            Err(GeneratorError::AccessNotChunk)?
        }

        let ws_min = cs_pos.worldspace_min();
        let ws_max = cs_pos.worldspace_max();

        for (ls_x, ws_x) in Self::zipped_coordinate_iter(ws_min.x, ws_max.x) {
            for (ls_y, ws_y) in Self::zipped_coordinate_iter(ws_min.y, ws_max.y) {
                for (ls_z, ws_z) in Self::zipped_coordinate_iter(ws_min.z, ws_max.z) {
                    let noise_pos = ivec3(ws_x, ws_y, ws_z).as_dvec3() * self.scale;
                    let ls_pos = ivec3(ls_x, ls_y, ls_z);
                    let ws_pos = ivec3(ws_x, ws_y, ws_z);

                    let input = self
                        .positions
                        .get(&ws_pos)
                        .copied()
                        .unwrap_or(ChunkVoxelInput {
                            transparency: Transparency::Transparent,
                            variant: self.palette.void,
                            rotation: None,
                        });

                    access.set(ls_pos, input)?;
                }
            }
        }

        Ok(())
    }
}
