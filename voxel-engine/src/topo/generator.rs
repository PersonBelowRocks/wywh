use std::{iter::Zip, ops::RangeInclusive};

use bevy::prelude::{Event, IVec3};
use noise::{NoiseFn, Perlin};
use parking_lot::RwLock;

use crate::data::{
    registries::{variant::VariantRegistry, Registries, Registry, RegistryId},
    tile::Transparency,
};

use super::{
    access::{ChunkBounds, HasBounds, WriteAccess},
    chunk::{Chunk, ChunkPos, VariantType},
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
    variants: &'a mut IndexedChunkStorage<VariantType>,
}

impl<'a> ChunkBounds for GeneratorInputAccess<'a> {}

impl<'a> WriteAccess for GeneratorInputAccess<'a> {
    type WriteType = ChunkVoxelInput;
    type WriteErr = ChunkAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.transparency.set(pos, data.transparency)?;
        self.variants.set(pos, data.variant)?;

        Ok(())
    }
}

pub struct GeneratorInput {
    transparency: DenseChunkContainer<Transparency>,
    variants: IndexedChunkStorage<VariantType>,
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
    noise: Perlin,
    scale: f64,
}

impl Generator {
    pub fn new(seed: u32, registries: &Registries) -> Self {
        let _noise = Perlin::new(seed);
        let variants = registries.get_registry::<VariantRegistry>().unwrap();

        Self {
            palette: GeneratorPalette {
                void: variants.get_id("void").unwrap(),
                debug: variants.get_id("debug").unwrap(),
            },
            noise: Perlin::new(seed),
            scale: 0.1,
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
                    let noise_pos = IVec3::new(ws_x, ws_y, ws_z).as_dvec3() * self.scale;
                    let ls_pos = IVec3::new(ls_x, ls_y, ls_z);

                    let noise = self.noise.get(noise_pos.into());

                    if noise > THRESHOLD {
                        access.set(
                            ls_pos,
                            ChunkVoxelInput {
                                transparency: Transparency::Opaque,
                                variant: self.palette.debug,
                            },
                        )?;
                    } else {
                        access.set(
                            ls_pos,
                            ChunkVoxelInput {
                                transparency: Transparency::Transparent,
                                variant: self.palette.void,
                            },
                        )?;
                    }
                }
            }
        }

        Ok(())
    }
}
