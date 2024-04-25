use bevy::math::IVec3;

use crate::{
    data::registries::{block::BlockVariantRegistry, Registry, RegistryRef},
    topo::chunk_ref::{MutChunkVxlOutput, MutCvoBlock},
    util::{microblock_to_full_block_3d, microblock_to_subdiv_pos_3d},
};

use super::{
    access::{ReadAccess, WriteAccess},
    block::{BlockVoxel, FullBlock, Microblock, SubdividedBlock},
    chunk_ref::{ChunkRefVxlAccess, ChunkVoxelInput, ChunkVoxelOutput, CrVra, CvoBlock},
    error::{ChunkAccessError, ChunkRefAccessError},
};

fn is_subdividable<'a>(
    registry: &RegistryRef<'a, BlockVariantRegistry>,
    id: <BlockVariantRegistry as Registry>::Id,
) -> bool {
    registry.get_by_id(id).options.subdividable
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum SubdivReadAccessOutput {
    Full(FullBlock),
    Micro(Microblock),
}

pub struct SubdivReadAccess<'chunk>(CrVra<'chunk>);

impl<'chunk> SubdivReadAccess<'chunk> {
    pub fn new(access: CrVra<'chunk>) -> Self {
        Self(access)
    }

    pub fn access(&self) -> &CrVra<'chunk> {
        &self.0
    }

    pub fn get(&self, pos: IVec3) -> Result<ChunkVoxelOutput<'_>, ChunkAccessError> {
        self.0.get(pos)
    }

    pub fn get_mb(&self, pos_mb: IVec3) -> Result<SubdivReadAccessOutput, ChunkAccessError> {
        let pos = microblock_to_full_block_3d(pos_mb);

        Ok(match self.get(pos)?.block {
            CvoBlock::Full(block) => SubdivReadAccessOutput::Full(block),
            CvoBlock::Subdivided(block) => {
                let pos_sd = microblock_to_subdiv_pos_3d(pos_mb).as_uvec3();
                SubdivReadAccessOutput::Micro(block.get(pos_sd).unwrap())
            }
        })
    }
}

/// Dictates what will happen when writing a microblock to a position occupied by a non-subdividable full block.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MbWriteBehaviour {
    /// The full block will be entirely replaced with a subdividable block filled with a default value,
    /// (usually void), with one of its microblocks set to the provided microblock
    Replace,
    /// Will ignore the write completely, essentially a no-op.
    Ignore, // TODO: maybe some non-subdividable block variants can reference a separate, subdividable variant that
            //  should be used instead when they are attempted to be subdivided?
}

pub struct SubdivAccess<'reg, 'chunk, 'acc> {
    access: &'acc mut ChunkRefVxlAccess<'chunk>,
    registry: RegistryRef<'reg, BlockVariantRegistry>,
    mb_write_behaviour: MbWriteBehaviour,
    default: Microblock,
}

impl<'reg, 'chunk, 'acc> SubdivAccess<'reg, 'chunk, 'acc> {
    pub fn new(
        registry: RegistryRef<'reg, BlockVariantRegistry>,
        access: &'acc mut ChunkRefVxlAccess<'chunk>,
        mb_write_behaviour: MbWriteBehaviour,
        default: Microblock,
    ) -> Self {
        Self {
            access,
            registry,
            mb_write_behaviour,
            default,
        }
    }

    pub fn access(&self) -> &ChunkRefVxlAccess<'chunk> {
        &self.access
    }

    pub fn mb_write_behaviour(&self) -> MbWriteBehaviour {
        self.mb_write_behaviour
    }

    pub fn set_mb_write_behaviour(&mut self, new: MbWriteBehaviour) {
        self.mb_write_behaviour = new
    }

    pub fn get(&self, pos: IVec3) -> Result<ChunkVoxelOutput<'_>, ChunkAccessError> {
        self.access.get(pos)
    }

    pub fn get_mb(&self, pos_mb: IVec3) -> Result<SubdivReadAccessOutput, ChunkAccessError> {
        let pos = microblock_to_full_block_3d(pos_mb);

        Ok(match self.get(pos)?.block {
            CvoBlock::Full(block) => SubdivReadAccessOutput::Full(block),
            CvoBlock::Subdivided(block) => {
                let pos_sd = microblock_to_subdiv_pos_3d(pos_mb).as_uvec3();
                SubdivReadAccessOutput::Micro(block.get(pos_sd).unwrap())
            }
        })
    }

    pub fn set(&mut self, pos: IVec3, vxl: ChunkVoxelInput) -> Result<(), ChunkAccessError> {
        self.access.set(pos, vxl)
    }

    /// Sets the microblock at `pos_mb`. Behaviour changes depending on the value of
    /// `MbWriteBehaviour` (see its documentation for info).
    pub fn set_mb(
        &mut self,
        pos_mb: IVec3,
        microblock: Microblock,
    ) -> Result<(), ChunkAccessError> {
        let pos = microblock_to_full_block_3d(pos_mb);
        let registry = &self.registry;

        let out = self.access.get(pos)?;
        match out.block {
            CvoBlock::Full(full) if is_subdividable(registry, full.id) => {
                let filling = Microblock {
                    rotation: full.rotation,
                    id: full.id,
                };

                let mut subdiv = SubdividedBlock::new(filling);
                let pos_sd = microblock_to_subdiv_pos_3d(pos_mb).as_uvec3();
                subdiv.set(pos_sd, microblock).unwrap();

                self.access
                    .set(pos, ChunkVoxelInput::new(BlockVoxel::Subdivided(subdiv)))?;
            }
            CvoBlock::Subdivided(subdiv) => {
                let mut subdiv = subdiv.clone();
                let pos_sd = microblock_to_subdiv_pos_3d(pos_mb).as_uvec3();
                subdiv.set(pos_sd, microblock).unwrap();

                self.access
                    .set(pos, ChunkVoxelInput::new(BlockVoxel::Subdivided(subdiv)))?;
            }
            CvoBlock::Full(full) if !is_subdividable(registry, full.id) => {
                match self.mb_write_behaviour {
                    MbWriteBehaviour::Ignore => (),
                    MbWriteBehaviour::Replace => {
                        let mut subdiv = SubdividedBlock::new(self.default);
                        let pos_sd = microblock_to_subdiv_pos_3d(pos_mb).as_uvec3();
                        subdiv.set(pos_sd, microblock).unwrap();

                        self.access
                            .set(pos, ChunkVoxelInput::new(BlockVoxel::Subdivided(subdiv)))?;
                    }
                }
            }

            _ => unreachable!(),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{ivec3, uvec3};
    use parking_lot::{RwLock, RwLockReadGuard};

    use crate::{
        data::registries::{block::BlockVariantRegistry, texture::TextureRegistry},
        testing_utils::MockChunk,
        topo::block::BlockVoxel,
    };

    // TODO: test replacing behaviour

    #[test]
    fn test_write_read_mb_ignore() {
        let texreg = TextureRegistry::new_mock();
        let varreg = RwLock::new(BlockVariantRegistry::new_mock(&texreg));
        let guard = varreg.read();
        let chunk = MockChunk::new(BlockVoxel::new_full(BlockVariantRegistry::VOID));

        let mut access = chunk.access();
        let mut sd_access = SubdivAccess::new(
            RwLockReadGuard::map(guard, |g| g),
            &mut access,
            MbWriteBehaviour::Ignore,
            Microblock::new(BlockVariantRegistry::VOID),
        );

        let full_block = BlockVoxel::new_full(BlockVariantRegistry::FULL);
        let full_subdividable_block = BlockVoxel::new_full(BlockVariantRegistry::SUBDIV);
        let microblock = Microblock::new(BlockVariantRegistry::VOID);

        sd_access
            .set(ivec3(4, 4, 4), ChunkVoxelInput::new(full_block.clone()))
            .unwrap();
        sd_access
            .set(
                ivec3(4, 5, 4),
                ChunkVoxelInput::new(full_subdividable_block.clone()),
            )
            .unwrap();

        sd_access
            .set_mb(ivec3(4, 4, 4) * SubdividedBlock::SUBDIVISIONS, microblock)
            .unwrap();
        sd_access
            .set_mb(ivec3(4, 5, 4) * SubdividedBlock::SUBDIVISIONS, microblock)
            .unwrap();

        drop(sd_access);
        drop(access);

        let read_access = chunk.read_access();
        let sd_access = SubdivReadAccess::new(read_access);

        assert_eq!(
            CvoBlock::Full(FullBlock::new(BlockVariantRegistry::FULL)),
            sd_access.get(ivec3(4, 4, 4)).unwrap().block
        );

        let subdiv = sd_access.get(ivec3(4, 5, 4)).unwrap().block;

        let CvoBlock::Subdivided(subdiv) = subdiv else {
            panic!("expected block to be subdivided")
        };

        assert_eq!(
            Microblock::new(BlockVariantRegistry::VOID),
            subdiv.get(uvec3(0, 0, 0)).unwrap()
        );

        assert_eq!(
            Microblock::new(BlockVariantRegistry::SUBDIV),
            subdiv.get(uvec3(0, 1, 0)).unwrap()
        );
        assert_eq!(
            Microblock::new(BlockVariantRegistry::SUBDIV),
            subdiv.get(uvec3(0, 0, 1)).unwrap()
        );
        assert_eq!(
            Microblock::new(BlockVariantRegistry::SUBDIV),
            subdiv.get(uvec3(1, 0, 0)).unwrap()
        );
    }
}
