use std::{iter::Zip, ops::RangeInclusive};

use bevy::prelude::{Event, IVec3};
use noise::{NoiseFn, Perlin};
use parking_lot::RwLock;

use crate::data::{
    registries::Registries,
    tile::Transparency,
    voxel::{BlockModel, VoxelModel},
};

use super::{
    access::{ChunkBounds, HasBounds, WriteAccess},
    chunk::{Chunk, ChunkPos},
    chunk_ref::ChunkVoxelInput,
    error::{ChunkVoxelAccessError, GeneratorError},
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
    models: &'a mut IndexedChunkStorage<BlockModel>,
}

impl<'a> ChunkBounds for GeneratorInputAccess<'a> {}

impl<'a> WriteAccess for GeneratorInputAccess<'a> {
    type WriteType = ChunkVoxelInput;
    type WriteErr = ChunkVoxelAccessError;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.transparency.set(pos, data.transparency)?;

        if let Some(VoxelModel::Block(model)) = data.model {
            self.models.set(pos, model);
        }

        Ok(())
    }
}

pub struct GeneratorInput {
    transparency: DenseChunkContainer<Transparency>,
    models: IndexedChunkStorage<BlockModel>,
}

impl GeneratorInput {
    pub fn new() -> Self {
        Self {
            transparency: DenseChunkContainer::Empty,
            models: IndexedChunkStorage::new(),
        }
    }

    pub fn access(&mut self) -> GeneratorInputAccess<'_> {
        GeneratorInputAccess {
            transparency: AutoDenseContainerAccess::new(
                &mut self.transparency,
                Transparency::Transparent,
            ),
            models: &mut self.models,
        }
    }

    pub fn to_chunk(self) -> Chunk {
        Chunk {
            transparency: SyncDenseChunkContainer(RwLock::new(self.transparency)),
            models: SyncIndexedChunkContainer(RwLock::new(self.models)),
        }
    }
}

#[derive(Event, Debug)]
pub struct GenerateChunk {
    pub pos: ChunkPos,
    pub generator: GeneratorChoice,
}

#[derive(Clone)]
pub struct Generator {
    registries: Registries,
    noise: Perlin,
    scale: f64,
}

impl Generator {
    pub fn new(seed: u32, registries: Registries) -> Self {
        let _noise = Perlin::new(seed);

        Self {
            registries,
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
        Acc: WriteAccess<WriteType = ChunkVoxelInput> + HasBounds,
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
                    let ls_pos = IVec3::new(ls_x, ls_y, ls_z);

                    let noise = self.noise.get(noise_pos.into());
                    todo!()
                }
            }
        }

        Ok(())
    }
}
