use std::cmp::min;

use bevy::{
    math::{ivec2, ivec3, IVec2, Vec3Swizzles},
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
            scale: 0.01,
        }
    }

    pub fn noise(&self, pos: IVec3) -> f64 {
        self.noise.get((pos.as_dvec3() * self.scale).to_array())
    }

    pub fn block_corner_noise_2d(&self, pos_2d: IVec2) -> f64 {
        let mut total = 0.0;

        for k in [0, 3] {
            for j in [0, 3] {
                total += self.noise_mb_2d((pos_2d * SubdividedBlock::SUBDIVISIONS) + ivec2(k, j));
            }
        }

        total / 4.0
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

    pub fn noise_mb_2d(&self, pos_mb_2d: IVec2) -> f64 {
        self.noise
            .get((pos_mb_2d.as_dvec2() * (self.scale / 4.0)).to_array())
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
        let ws_min_sd = ws_min * SubdividedBlock::SUBDIVISIONS;
        let ws_max = cs_pos.worldspace_max();

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

        // FIXME: this entire section of code produces broken terrain, fix it
        for x in 0..Chunk::SIZE {
            for z in 0..Chunk::SIZE {
                let ls_pos_2d = ivec2(x, z);
                let ws_pos_2d = ls_pos_2d + ws_min.xz();

                let avg_noise = self.block_corner_noise_2d(ws_pos_2d);

                let height = avg_noise * 20.0 + 10.0;
                let floored_height = height.floor() as i32;

                if floored_height < 0 {
                    continue;
                }

                if floored_height > ws_max.y {
                    for y in 0..Chunk::SIZE {
                        let ls_pos = ivec3(x, y, z);

                        sd_access.set(ls_pos, ChunkAccessInput::new(stone_block.clone()))?;
                    }
                } else {
                    let max_y = floored_height - ws_min.y;

                    if max_y < 0 {
                        continue;
                    }

                    for y in 0..max_y {
                        let ls_pos = ivec3(x, y, z);

                        sd_access.set(ls_pos, ChunkAccessInput::new(stone_block.clone()))?;
                    }

                    for mb_x in 0..4 {
                        for mb_z in 0..4 {
                            let ls_pos_mb_2d =
                                ivec2(mb_x, mb_z) + (ls_pos_2d * SubdividedBlock::SUBDIVISIONS);

                            let ws_pos_mb_2d = ls_pos_mb_2d + ws_min_sd.xz();
                            let avg_noise = self.noise_mb_2d(ws_pos_mb_2d);

                            let height = avg_noise * 20.0 + 10.0;
                            let height_mb = (height * 4.0).floor() as i32;
                            let leftovers =
                                height_mb - (floored_height * SubdividedBlock::SUBDIVISIONS);

                            if leftovers < 0 {
                                continue;
                            }

                            for mb_y in 0..min(leftovers, 3) {
                                let y = mb_y + (max_y * SubdividedBlock::SUBDIVISIONS);

                                let ls_pos_mb = ivec3(ls_pos_mb_2d.x, y, ls_pos_mb_2d.y);

                                sd_access.set_mb(ls_pos_mb, Microblock::new(self.palette.stone))?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
