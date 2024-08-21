use bevy::{
    math::{ivec2, ivec3, IVec2},
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
        block::SubdividedBlock,
        world::{chunk::ChunkWriteHandle, ChunkHandleError, ChunkPos},
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
        access: &mut ChunkWriteHandle<'chunk>,
    ) -> Result<(), GeneratorError<ChunkHandleError>> {
        access.set_mb(ivec3(10, 10, 10), self.palette.stone)?;

        Ok(())
    }
}
