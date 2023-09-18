use std::{iter::Zip, ops::RangeInclusive};

use bevy::prelude::{Event, IVec3};
use noise::{NoiseFn, Perlin};

use crate::data::tile::VoxelId;

use super::{
    access::{HasBounds, WriteAccess},
    chunk::{Chunk, ChunkPos},
    error::GeneratorError,
};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum GeneratorChoice {
    Default,
}

#[derive(Event, Debug)]
pub struct GenerateChunk<T> {
    pub pos: ChunkPos,
    pub generator: GeneratorChoice,
    pub default_value: T,
}

#[derive(Debug, Clone)]
pub struct Generator {
    noise: Perlin,
    scale: f64,
}

impl Generator {
    pub fn new(seed: u32) -> Self {
        let noise = Perlin::new(seed);

        Self {
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
        Acc: WriteAccess<WriteType = VoxelId> + HasBounds,
    {
        if !access.bounds().is_chunk() {
            Err(GeneratorError::AccessNotChunk)?
        }

        let ws_min = cs_pos.worldspace_min();
        let ws_max = cs_pos.worldspace_max();

        for (ls_x, ws_x) in Self::zipped_coordinate_iter(ws_min.x, ws_max.x) {
            for (ls_y, ws_y) in Self::zipped_coordinate_iter(ws_min.y, ws_max.y) {
                for (ls_z, ws_z) in Self::zipped_coordinate_iter(ws_min.z, ws_max.z) {
                    let noise_pos = IVec3::new(ws_x, ws_y, ws_z).as_dvec3() * self.scale;

                    let noise = self.noise.get(noise_pos.into());
                    let voxel_id = if noise > 0.5 {
                        VoxelId::from(1)
                    } else {
                        VoxelId::from(0)
                    };

                    let ls_pos = IVec3::new(ls_x, ls_y, ls_z);
                    access.set(ls_pos, voxel_id)?;
                }
            }
        }

        Ok(())
    }
}
