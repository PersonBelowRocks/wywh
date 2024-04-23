use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{
    data::{
        registries::{block::BlockVariantRegistry, Registry, RegistryRef},
        texture::FaceTexture,
        tile::Face,
        voxel::{rotations::BlockModelRotation, BlockSubmodel, VoxelModel},
    },
    render::quad::{
        anon::Quad,
        data::DataQuad,
        isometric::{IsometrizedQuad, PositionedQuad, QuadIsometry},
    },
    topo::{
        access::{ChunkAccess, ReadAccess},
        block::{Microblock, SubdividedBlock},
        chunk::Chunk,
        chunk_ref::{ChunkVoxelOutput, CrVra, CvoBlock},
        ivec_project_to_2d, ivec_project_to_3d,
        neighbors::{self, Neighbors},
        storage::error::OutOfBounds,
    },
};

use self::error::CqsError;

pub mod algorithm;
pub mod error;
pub mod greedy_mesh;
pub mod material;

#[derive(Clone)]
pub struct ChunkQuadSlice<'a, 'chunk> {
    face: Face,
    mag: i32,

    access: &'a CrVra<'chunk>,
    neighbors: &'a Neighbors<'chunk>,
    registry: &'a RegistryRef<'a, BlockVariantRegistry>,
}

pub const MAX: IVec2 = IVec2::splat(Chunk::SIZE);

pub type CqsResult<T> = Result<T, CqsError>;

impl<'a, 'chunk> ChunkQuadSlice<'a, 'chunk> {
    #[inline]
    pub fn new(
        face: Face,
        magnitude: i32,
        access: &'a CrVra<'chunk>,
        neighbors: &'a Neighbors<'chunk>,
        registry: &'a RegistryRef<'a, BlockVariantRegistry>,
    ) -> Result<Self, OutOfBounds> {
        if 0 > magnitude && magnitude > Chunk::SUBDIVIDED_CHUNK_SIZE {
            return Err(OutOfBounds);
        }

        Ok(Self {
            face,
            mag: magnitude,
            access,
            neighbors,
            registry,
        })
    }

    fn face_texture_for_variant(
        &self,
        variant_id: <BlockVariantRegistry as Registry>::Id,
        rotation: Option<BlockModelRotation>,
    ) -> Option<FaceTexture> {
        let model = self.registry.get_by_id(variant_id).model?;
        let submodel = rotation
            .map(|r| model.submodel(r.front()))
            .unwrap_or(model.default_submodel());

        Some(submodel.texture(self.face))
    }

    pub fn reposition(&mut self, face: Face, magnitude: i32) -> Result<(), OutOfBounds> {
        if 0 > magnitude && magnitude > Chunk::SIZE {
            return Err(OutOfBounds);
        }

        self.face = face;
        self.mag = magnitude;

        Ok(())
    }

    pub fn contains_mb(pos: IVec2) -> bool {
        Self::contains(pos.div_euclid(SubdividedBlock::SUBDIVS_VEC2))
    }

    pub fn contains(pos: IVec2) -> bool {
        pos.cmplt(MAX).all() && pos.cmpge(IVec2::ZERO).all()
    }

    pub fn contains_3d(pos: IVec3) -> bool {
        pos.cmplt(Chunk::VEC).all() && pos.cmpge(IVec3::ZERO).all()
    }

    pub fn isometrize(&self, quad: PositionedQuad) -> IsometrizedQuad {
        let iso = QuadIsometry::new(quad.pos(), self.mag, self.face);
        IsometrizedQuad::new(iso, quad)
    }

    /// Transforms a position from micro-facespace to micro-localspace
    #[inline]
    pub fn pos_3d_mb(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(pos, self.face, self.mag)
    }

    /// Transforms a position from facespace to localspace
    #[inline]
    pub fn pos_3d(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(
            pos,
            self.face,
            self.mag.div_euclid(SubdividedBlock::SUBDIVISIONS),
        )
    }

    #[inline]
    pub fn pos_3d_sd(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(
            pos,
            self.face,
            self.mag.rem_euclid(SubdividedBlock::SUBDIVISIONS),
        )
    }

    /// Transforms a position from localspace to facespace
    #[inline]
    pub fn pos_2d(&self, pos: IVec3) -> IVec2 {
        ivec_project_to_2d(pos, self.face)
    }

    #[inline]
    pub fn get_3d(&self, pos: IVec3) -> CqsResult<ChunkVoxelOutput> {
        self.access.get(pos).map_err(|e| CqsError::AccessError(e))
    }

    #[inline]
    pub fn get_mb_3d(&self, pos_mb: IVec3) -> CqsResult<Microblock> {
        let pos = pos_mb.div_euclid(SubdividedBlock::SUBDIVS_VEC3);

        Ok(match self.get_3d(pos)?.block {
            CvoBlock::Full(block) => Microblock {
                rotation: block.rotation,
                id: block.id,
            },
            CvoBlock::Subdivided(block) => {
                let pos_sd = pos_mb.rem_euclid(SubdividedBlock::SUBDIVS_VEC3).as_uvec3();
                block.get(pos_sd).unwrap()
            }
        })
    }

    /// `pos` is in localspace and can exceed the regular chunk bounds by 1 for any component of the vector.
    /// In this case the `ChunkVoxelOutput` is taken from a neighboring chunk.
    #[inline]
    pub fn auto_neighboring_get(&self, pos: IVec3) -> CqsResult<ChunkVoxelOutput> {
        if Self::contains_3d(pos) && !neighbors::is_in_bounds_3d(pos) {
            self.get_3d(pos)
        } else if !Self::contains_3d(pos) && neighbors::is_in_bounds_3d(pos) {
            Ok(self.neighbors.get_3d(pos)?)
        } else {
            return Err(CqsError::OutOfBounds);
        }
    }

    #[inline]
    pub fn auto_neighboring_get_mb(&self, pos_mb: IVec3) -> CqsResult<Microblock> {
        let pos = pos_mb.div_euclid(SubdividedBlock::SUBDIVS_VEC3);

        if Self::contains_3d(pos) && !neighbors::is_in_bounds_3d(pos) {
            self.get_mb_3d(pos_mb)
        } else if !Self::contains_3d(pos) && neighbors::is_in_bounds_3d(pos) {
            let nb_block = self.neighbors.get_3d(pos)?.block;

            Ok(match nb_block {
                CvoBlock::Full(block) => Microblock {
                    rotation: block.rotation,
                    id: block.id,
                },
                CvoBlock::Subdivided(block) => {
                    let pos_sd = pos_mb.rem_euclid(SubdividedBlock::SUBDIVS_VEC3).as_uvec3();
                    block.get(pos_sd).unwrap()
                }
            })
        } else {
            return Err(CqsError::OutOfBounds);
        }
    }

    #[inline]
    pub fn get(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos3d = self.pos_3d(pos);
        self.get_3d(pos3d)
    }

    #[inline]
    pub fn get_mb(&self, pos_mb: IVec2) -> CqsResult<Microblock> {
        if !Self::contains_mb(pos_mb) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_mb_3d = self.pos_3d_mb(pos_mb);
        self.get_mb_3d(pos_mb_3d)
    }

    #[inline]
    pub fn get_above(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_above = self.pos_3d(pos) + self.face.normal();
        self.auto_neighboring_get(pos_above)
    }

    #[inline]
    pub fn get_mb_above(&self, pos_mb: IVec2) -> CqsResult<Microblock> {
        if !Self::contains_mb(pos_mb) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_mb_above = self.pos_3d_mb(pos_mb) + self.face.normal();
        let pos_above = pos_mb_above.div_euclid(SubdividedBlock::SUBDIVS_VEC3);

        let block = self.auto_neighboring_get(pos_above)?.block;

        Ok(match block {
            CvoBlock::Full(block) => Microblock {
                rotation: block.rotation,
                id: block.id,
            },
            CvoBlock::Subdivided(block) => {
                let pos_sd_above = pos_mb_above
                    .rem_euclid(SubdividedBlock::SUBDIVS_VEC3)
                    .as_uvec3();
                block.get(pos_sd_above).unwrap()
            }
        })
    }

    /// Get a quad for given position. This function operates on microblock resolution, so the relevant
    /// block for the provided `pos_mb` is at position `pos_mb / 4` in chunkspace.
    /// Returns `None` if the microblock at the position is obscured by a block "above" it
    /// or if the block at the position doesn't have a model.
    #[inline]
    pub fn get_quad_mb(&self, pos_mb: IVec2) -> CqsResult<Option<DataQuad>> {
        let pos = pos_mb.div_euclid(SubdividedBlock::SUBDIVS_VEC2);

        let microblock = self.get_mb(pos_mb)?;
        let microblock_above = self.get_mb_above(pos_mb)?;

        let entry = self.registry.get_by_id(microblock.id);
        let entry_above = self.registry.get_by_id(microblock_above.id);

        if entry.options.transparency.is_transparent()
            || entry_above.options.transparency.is_opaque()
        {
            return Ok(None);
        }

        let Some(model) = entry.model else {
            return Ok(None);
        };

        let submodel = microblock
            .rotation
            .map(|r| model.submodel(r.front()))
            .unwrap_or(model.default_submodel());

        let texture = submodel.texture(self.face);

        Ok(Some(DataQuad::new(Quad::ONE, texture)))
    }
}

#[cfg(test)]
pub mod tests {
    use bevy::math::{uvec3, vec3};
    use parking_lot::{RwLock, RwLockReadGuard};
    use tests::neighbors::NeighborsBuilder;

    use crate::{
        data::registries::texture::TextureRegistry,
        testing_utils::MockChunk,
        topo::{
            access::WriteAccess,
            block::{BlockVoxel, FullBlock},
            chunk_ref::ChunkVoxelInput,
        },
    };

    use super::*;

    fn testing_neighbors<'a>(chunk: &'a MockChunk) -> Neighbors<'a> {
        let mut builder = NeighborsBuilder::new(BlockVoxel::new_full(BlockVariantRegistry::FULL));
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let p = ivec3(x, y, z);
                    if p == IVec3::ZERO {
                        continue;
                    }

                    builder.set_neighbor(p, chunk.read_access()).unwrap();
                }
            }
        }

        builder.build()
    }

    fn testing_chunk() -> MockChunk {
        let block = BlockVoxel::new_full(BlockVariantRegistry::FULL);

        let mut chunk = MockChunk::new(BlockVoxel::new_full(BlockVariantRegistry::VOID));
        let mut access = chunk.access();

        ////////////////////////////////////////////////////////////////////////////////////////////
        // blocks on top chunk border for neighbor quad test

        access
            .set(ivec3(9, 15, 8), ChunkVoxelInput::new(block.clone()))
            .unwrap();

        access
            .set(ivec3(8, 15, 8), ChunkVoxelInput::new(block.clone()))
            .unwrap();

        let edge_subdiv = {
            let mut block = SubdividedBlock::new(Microblock::new(BlockVariantRegistry::VOID));

            let mb = Microblock::new(BlockVariantRegistry::SUBDIV);

            for x in 0..2 {
                for z in 0..2 {
                    block.set(uvec3(x, 3, z), mb).unwrap();
                }
            }

            block
        };

        access
            .set(
                ivec3(8, 15, 9),
                ChunkVoxelInput::new(BlockVoxel::Subdivided(edge_subdiv.clone())),
            )
            .unwrap();
        access
            .set(
                ivec3(8, 15, 7),
                ChunkVoxelInput::new(BlockVoxel::Subdivided(edge_subdiv.clone())),
            )
            .unwrap();

        ////////////////////////////////////////////////////////////////////////////////////////////
        // various blocks placed nicely inside the chunk for quad within chunk test

        access
            .set(ivec3(4, 0, 4), ChunkVoxelInput::new(block.clone()))
            .unwrap();
        access
            .set(ivec3(4, 0, 3), ChunkVoxelInput::new(block.clone()))
            .unwrap();
        access
            .set(ivec3(4, 1, 4), ChunkVoxelInput::new(block.clone()))
            .unwrap();
        access
            .set(ivec3(4, 2, 4), ChunkVoxelInput::new(block.clone()))
            .unwrap();
        access
            .set(ivec3(4, 1, 2), ChunkVoxelInput::new(block.clone()))
            .unwrap();

        let mut subdiv_block = SubdividedBlock::new(Microblock::new(BlockVariantRegistry::VOID));

        let mb = Microblock::new(BlockVariantRegistry::SUBDIV);

        for z in 0..4 {
            subdiv_block.set(uvec3(0, 0, z), mb).unwrap();
        }

        access
            .set(
                ivec3(4, 1, 3),
                ChunkVoxelInput::new(BlockVoxel::Subdivided(subdiv_block)),
            )
            .unwrap();

        ////////////////////////////////////////////////////////////////////////////////////////////

        drop(access);
        chunk
    }

    #[test]
    fn cqs_can_read() {
        let texreg = TextureRegistry::new_mock();
        let varreg = RwLock::new(BlockVariantRegistry::new_mock(&texreg));
        let neighbor_chunk = MockChunk::new(BlockVoxel::new_full(BlockVariantRegistry::VOID));
        let chunk = testing_chunk();
        let neighbors = testing_neighbors(&neighbor_chunk);

        let access = chunk.read_access();
        let guard = RwLockReadGuard::map(varreg.read(), |g| g);

        let mut cqs = ChunkQuadSlice::new(Face::Top, 11, &access, &neighbors, &guard).unwrap();

        assert_eq!(
            BlockVariantRegistry::FULL,
            cqs.get_mb(ivec2(16, 16)).unwrap().id
        );
        assert!(cqs.get_quad_mb(ivec2(16, 16)).unwrap().is_some());
    }

    #[test]
    fn cqs_get_quad_mb_within_chunk() {
        let texreg = TextureRegistry::new_mock();
        let varreg = RwLock::new(BlockVariantRegistry::new_mock(&texreg));
        let neighbor_chunk = MockChunk::new(BlockVoxel::new_full(BlockVariantRegistry::VOID));
        let chunk = testing_chunk();
        let neighbors = testing_neighbors(&neighbor_chunk);

        let access = chunk.read_access();
        let guard = RwLockReadGuard::map(varreg.read(), |g| g);

        let mut cqs = ChunkQuadSlice::new(Face::Top, 3, &access, &neighbors, &guard).unwrap();

        let subdiv_texture = FaceTexture::new(TextureRegistry::TEX2);
        let full_texture = FaceTexture::new(TextureRegistry::TEX1);

        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(16, 16)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(17, 17)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(18, 18)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(19, 19)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(12, 12)));
        assert_eq!(
            BlockVariantRegistry::FULL,
            cqs.get_mb(ivec2(16, 12)).unwrap().id
        );
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(16, 12)));
        assert_eq!(
            Ok(Some(DataQuad::new(Quad::ONE, full_texture))),
            cqs.get_quad_mb(ivec2(17, 12))
        );

        cqs.reposition(Face::Top, 4).unwrap();

        for z in 0..4 {
            assert_eq!(
                Ok(Some(DataQuad::new(Quad::ONE, subdiv_texture))),
                cqs.get_quad_mb(ivec2(16, 12 + z))
            );
        }
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(17, 12)));
    }

    #[test]
    fn cqs_get_quad_mb_across_chunks() {
        let texreg = TextureRegistry::new_mock();
        let varreg = RwLock::new(BlockVariantRegistry::new_mock(&texreg));

        // sets up some blocks and microblocks on the bottom edge of the neighboring chunk
        let neighbor_chunk = {
            let chunk = MockChunk::new(BlockVoxel::new_full(BlockVariantRegistry::VOID));
            let mut access = chunk.access();

            let full = BlockVoxel::new_full(BlockVariantRegistry::FULL);
            let subdiv = {
                let mut block = SubdividedBlock::new(Microblock::new(BlockVariantRegistry::VOID));

                for x in 0..4 {
                    block
                        .set(
                            uvec3(x, 0, 0),
                            Microblock::new(BlockVariantRegistry::SUBDIV),
                        )
                        .unwrap();
                }

                BlockVoxel::Subdivided(block)
            };

            access
                .set(ivec3(8, 0, 9), ChunkVoxelInput::new(full.clone()))
                .unwrap();
            access
                .set(ivec3(8, 0, 8), ChunkVoxelInput::new(full.clone()))
                .unwrap();
            access
                .set(ivec3(8, 0, 7), ChunkVoxelInput::new(subdiv))
                .unwrap();

            drop(access);
            chunk
        };

        let chunk = testing_chunk();
        let neighbors = testing_neighbors(&neighbor_chunk);

        let access = chunk.read_access();
        let guard = RwLockReadGuard::map(varreg.read(), |g| g);

        let mut cqs =
            ChunkQuadSlice::new(Face::Top, (4 * 15) + 3, &access, &neighbors, &guard).unwrap();

        let subdiv_texture = FaceTexture::new(TextureRegistry::TEX2);
        let full_texture = FaceTexture::new(TextureRegistry::TEX1);

        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(32, 32)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(33, 33)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(34, 34)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(35, 35)));
        assert_eq!(
            Ok(Some(DataQuad::new(Quad::ONE, full_texture))),
            cqs.get_quad_mb(ivec2(36, 32))
        );

        for d in 0..4 {
            assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(32 + d, 36 + d)));
        }

        for x in 0..4 {
            assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(32 + x, 28)));
        }

        assert_eq!(
            Ok(Some(DataQuad::new(Quad::ONE, subdiv_texture))),
            cqs.get_quad_mb(ivec2(32, 29))
        );
        assert_eq!(
            Ok(Some(DataQuad::new(Quad::ONE, subdiv_texture))),
            cqs.get_quad_mb(ivec2(33, 29))
        );

        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(34, 29)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(35, 29)));

        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(32, 30)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(33, 30)));
        assert_eq!(Ok(None), cqs.get_quad_mb(ivec2(34, 30)));
    }
}
