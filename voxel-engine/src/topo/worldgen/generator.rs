use bevy::{
    math::ivec3,
    prelude::{Event, IVec3},
};
use noise::{NoiseFn, Perlin};

use crate::{
    data::{
        registries::{block::BlockVariantRegistry, Registries, Registry},
        resourcepath::rpath,
        tile::Face,
        voxel::rotations::BlockModelRotation,
    },
    topo::{
        block::{BlockVoxel, Microblock, SubdividedBlock},
        error::ChunkAccessError,
        world::{chunk_ref::ChunkRefAccess, Chunk, ChunkAccessInput, ChunkPos},
        MbWriteBehaviour, SubdivAccess,
    },
};

use super::{error::GeneratorError, GenerationPriority};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum GeneratorChoice {
    Default,
}

#[derive(Event, Debug)]
pub struct GenerateChunk {
    pub pos: ChunkPos,
    pub priority: GenerationPriority,
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

    pub fn noise(&self, pos: IVec3) -> f64 {
        self.noise.get((pos.as_dvec3() * self.scale).to_array())
    }

    /// Calculate the average noise from all 8 microblock corners of the given position.
    pub fn block_corner_noise(&self, pos: IVec3) -> f64 {
        let mut total = 0.0;

        for x in [0, 3] {
            for y in [0, 3] {
                for z in [0, 3] {
                    total += self.noise_mb((pos * SubdividedBlock::SUBDIVISIONS) + ivec3(x, y, z));
                }
            }
        }

        total / 8.0
    }

    pub fn noise_mb(&self, pos_mb: IVec3) -> f64 {
        self.noise
            .get((pos_mb.as_dvec3() * (self.scale / 4.0)).to_array())
    }

    #[inline]
    pub fn write_to_chunk<'chunk>(
        &self,
        cs_pos: ChunkPos,
        access: &mut ChunkRefAccess<'chunk>,
    ) -> Result<(), GeneratorError<ChunkAccessError>> {
        const THRESHOLD: f64 = 0.25;

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

        let stone_block = BlockVoxel::new_full(self.palette.stone);

        let ws_min = cs_pos.worldspace_min();
        let ws_min_sd = ws_min * 4;
        let _ws_max = cs_pos.worldspace_max();

        if cs_pos.y() < 0 {
            for x in 0..Chunk::SIZE {
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        sd_access.set(
                            ivec3(x, y, z),
                            ChunkAccessInput::new(BlockVoxel::new_full(self.palette.water)),
                        )?;
                    }
                }
            }

            return Ok(());
        }

        sd_access.set(
            ivec3(0, 0, 0),
            ChunkAccessInput::new(BlockVoxel::new_full(self.palette.debug)),
        )?;

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let ls_pos = ivec3(x, y, z);
                    let ws_pos = ls_pos + ws_min;

                    let avg_corner_noise = self.block_corner_noise(ws_pos);

                    if avg_corner_noise > THRESHOLD {
                        sd_access.set(ls_pos, ChunkAccessInput::new(stone_block.clone()))?;
                    // This test here is a decent-ish hueristic check that discards a lot of empty blocks (void) but
                    // includes enough edges and corners and stuff that it makes the resulting terrain smoother
                    } else if avg_corner_noise > (THRESHOLD / 2.0) {
                        for mb_x in 0..SubdividedBlock::SUBDIVISIONS {
                            for mb_y in 0..SubdividedBlock::SUBDIVISIONS {
                                for mb_z in 0..SubdividedBlock::SUBDIVISIONS {
                                    let mb_pos = ivec3(mb_x, mb_y, mb_z);
                                    let ls_pos_sd =
                                        mb_pos + (ls_pos * SubdividedBlock::SUBDIVISIONS);

                                    let ws_pos_sd = ls_pos_sd + ws_min_sd;

                                    let noise = self.noise_mb(ws_pos_sd);
                                    if noise > THRESHOLD {
                                        sd_access.set_mb(
                                            ls_pos_sd,
                                            Microblock::new(self.palette.stone),
                                        )?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
