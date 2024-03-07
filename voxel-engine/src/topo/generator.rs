use std::{iter::Zip, ops::RangeInclusive};

use bevy::{
    math::{ivec2, ivec3},
    prelude::{Event, IVec3},
};
use noise::{NoiseFn, Perlin};
use parking_lot::RwLock;

use crate::data::{
    registries::{block::BlockVariantRegistry, Registries, Registry},
    resourcepath::{rpath, ResourcePath},
    tile::{Face, Transparency},
    voxel::rotations::BlockModelRotation,
};

use super::{
    access::{ChunkBounds, HasBounds, WriteAccess},
    block::BlockVoxel,
    chunk::{Chunk, ChunkPos, VoxelVariantData},
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
    variants: &'a mut IndexedChunkStorage<BlockVoxel>,
}

impl<'a> ChunkBounds for GeneratorInputAccess<'a> {}

impl<'a> WriteAccess for GeneratorInputAccess<'a> {
    type WriteType = ChunkVoxelInput;
    type WriteErr = ChunkAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.variants.set(pos, todo!())?;

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
            variants: &mut self.variants,
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

        for (ls_x, _ws_x) in Self::zipped_coordinate_iter(ws_min.x, ws_max.x) {
            for (ls_y, _ws_y) in Self::zipped_coordinate_iter(ws_min.y, ws_max.y) {
                for (ls_z, _ws_z) in Self::zipped_coordinate_iter(ws_min.z, ws_max.z) {
                    let ls_pos = ivec3(ls_x, ls_y, ls_z);

                    access.set(
                        ls_pos,
                        ChunkVoxelInput {
                            variant: self.palette.void,
                            rotation: None,
                        },
                    )?;
                }
            }
        }

        for (ls_x, ws_x) in Self::zipped_coordinate_iter(ws_min.x, ws_max.x) {
            for (ls_z, ws_z) in Self::zipped_coordinate_iter(ws_min.z, ws_max.z) {
                if cs_pos.y < 0 {
                    for (ls_y, _ws_y) in Self::zipped_coordinate_iter(ws_min.y, ws_max.y) {
                        let ls_pos = ivec3(ls_x, ls_y, ls_z);

                        access.set(
                            ls_pos,
                            ChunkVoxelInput {
                                variant: self.palette.water,
                                rotation: None,
                            },
                        )?;
                    }
                } else {
                    let noise_pos = ivec2(ws_x, ws_z).as_dvec2() * self.scale;
                    let noise = self.noise.get([noise_pos.x, noise_pos.y]);

                    let height = (noise * 32.0) as i32;

                    for (ls_y, ws_y) in Self::zipped_coordinate_iter(ws_min.y, ws_max.y) {
                        let ls_pos = ivec3(ls_x, ls_y, ls_z);
                        let ws_pos = ivec3(ws_x, ws_y, ws_z);

                        if ws_pos.y <= height {
                            access.set(
                                ls_pos,
                                ChunkVoxelInput {
                                    variant: self.palette.stone,
                                    rotation: None,
                                },
                            )?;
                        }
                    }
                }
            }
        }

        for (ls_x, ws_x) in Self::zipped_coordinate_iter(ws_min.x, ws_max.x) {
            for (ls_y, ws_y) in Self::zipped_coordinate_iter(ws_min.y, ws_max.y) {
                for (ls_z, ws_z) in Self::zipped_coordinate_iter(ws_min.z, ws_max.z) {
                    let noise_pos = ivec3(ws_x, ws_y, ws_z).as_dvec3() * self.scale;
                    let _ls_pos = ivec3(ls_x, ls_y, ls_z);
                    let _ws_pos = ivec3(ws_x, ws_y, ws_z);

                    let _noise = self.noise.get([noise_pos.x, noise_pos.z]);
                }
            }
        }

        Ok(())
    }
}
