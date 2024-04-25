use std::{iter::Zip, ops::RangeInclusive};

use bevy::{
    math::{ivec2, ivec3},
    prelude::{Event, IVec3},
};
use noise::{NoiseFn, Perlin};
use parking_lot::RwLock;

use crate::{
    data::{
        registries::{block::BlockVariantRegistry, Registries, Registry},
        resourcepath::{rpath, ResourcePath},
        tile::{Face, Transparency},
        voxel::rotations::BlockModelRotation,
    },
    topo::{
        block::{Microblock, SubdividedBlock},
        MbWriteBehaviour, SubdivAccess,
    },
};

use super::{
    access::{ChunkBounds, HasBounds, WriteAccess},
    block::BlockVoxel,
    chunk::{Chunk, ChunkPos, VoxelVariantData},
    chunk_ref::{ChunkRefVxlAccess, ChunkVoxelInput},
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
    block_variants: &'a mut IndexedChunkStorage<BlockVoxel>,
}

impl<'a> ChunkBounds for GeneratorInputAccess<'a> {}

impl<'a> WriteAccess for GeneratorInputAccess<'a> {
    type WriteType = ChunkVoxelInput;
    type WriteErr = ChunkAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.block_variants.set(pos, data.block)?;

        Ok(())
    }
}

pub struct GeneratorInput {
    transparency: DenseChunkContainer<Transparency>,
    variants: IndexedChunkStorage<BlockVoxel>,
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
            block_variants: &mut self.variants,
        }
    }

    pub fn to_chunk(self) -> Chunk {
        Chunk {
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
    void: <BlockVariantRegistry as Registry>::Id,
    debug: <BlockVariantRegistry as Registry>::Id,
    stone: <BlockVariantRegistry as Registry>::Id,
    water: <BlockVariantRegistry as Registry>::Id,
}

#[derive(Clone)]
pub struct Generator {
    registries: Registries,
    palette: GeneratorPalette,
    default_rotation: BlockModelRotation,
    noise: Perlin,
    scale: f64,
}

impl Generator {
    pub fn new(seed: u32, registries: &Registries) -> Self {
        let _noise = Perlin::new(seed);
        let variants = registries.get_registry::<BlockVariantRegistry>().unwrap();

        let palette = GeneratorPalette {
            void: variants.get_id(&rpath("void")).unwrap(),
            debug: variants.get_id(&rpath("debug")).unwrap(),
            stone: variants.get_id(&rpath("stone")).unwrap(),
            water: variants.get_id(&rpath("water")).unwrap(),
        };

        Self {
            registries: registries.clone(),
            palette,
            default_rotation: BlockModelRotation::new(Face::North, Face::Top).unwrap(),
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
    pub fn write_to_chunk<'chunk>(
        &self,
        cs_pos: ChunkPos,
        access: &mut ChunkRefVxlAccess<'chunk>,
    ) -> Result<(), GeneratorError<ChunkAccessError>> {
        const THRESHOLD: f64 = 0.5;

        let varreg = self
            .registries
            .get_registry::<BlockVariantRegistry>()
            .unwrap();

        let mut sd_access = SubdivAccess::new(
            varreg,
            access,
            MbWriteBehaviour::Ignore,
            Microblock::new(self.palette.void),
        );

        let ws_min = cs_pos.worldspace_min();
        let ws_min_sd = ws_min * 4;
        let ws_max = cs_pos.worldspace_max();

        if cs_pos.y < 0 {
            for x in 0..Chunk::SIZE {
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        sd_access.set(
                            ivec3(x, y, z),
                            ChunkVoxelInput::new(BlockVoxel::new_full(self.palette.water)),
                        )?;
                    }
                }
            }

            return Ok(());
        }

        sd_access.set(
            ivec3(0, 0, 0),
            ChunkVoxelInput::new(BlockVoxel::new_full(self.palette.debug)),
        )?;

        for sd_x in 0..Chunk::SUBDIVIDED_CHUNK_SIZE {
            for sd_y in 0..Chunk::SUBDIVIDED_CHUNK_SIZE {
                for sd_z in 0..Chunk::SUBDIVIDED_CHUNK_SIZE {
                    let ls_pos_sd = ivec3(sd_x, sd_y, sd_z);

                    if ls_pos_sd.cmplt(IVec3::splat(4)).all() {
                        continue;
                    }

                    let ws_pos_sd = ws_min_sd + ls_pos_sd;

                    let noise_pos = ivec3(ws_pos_sd.x, ws_pos_sd.y, ws_pos_sd.z).as_dvec3()
                        * (self.scale / 4.0);
                    let noise = self.noise.get([noise_pos.x, noise_pos.y, noise_pos.z]);

                    if noise > THRESHOLD {
                        sd_access.set_mb(ls_pos_sd, Microblock::new(self.palette.stone))?;
                    }
                }
            }
        }

        Ok(())
    }
}
