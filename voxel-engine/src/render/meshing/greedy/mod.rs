use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{
    data::{
        registries::{block::BlockVariantRegistry, Registry, RegistryRef},
        texture::FaceTexture,
        tile::Face,
        voxel::{rotations::BlockModelRotation, VoxelModel},
    },
    render::quad::{
        anon::Quad,
        data::DataQuad,
        isometric::{IsometrizedQuad, PositionedQuad, QuadIsometry},
    },
    topo::{
        access::{ChunkAccess, ReadAccess},
        block::SubdividedBlock,
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
        if 0 > magnitude && magnitude > Chunk::SIZE {
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
        Self::contains(pos.div_euclid(SubdividedBlock::SUBDIVS_VEC))
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

    /// Transforms a position from facespace to localspace
    #[inline]
    pub fn pos_3d(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(pos, self.face, self.mag)
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
    pub fn get(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos3d = self.pos_3d(pos);
        self.get_3d(pos3d)
    }

    #[inline]
    pub fn get_above(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_above = self.pos_3d(pos) + self.face.normal();
        self.auto_neighboring_get(pos_above)
    }

    /// Get a quad for given position. This function operates on microblock resolution, so the relevant
    /// block for the provided `pos_mb` is at position `pos_mb / 4` in chunkspace.
    /// Returns `None` if the microblock at the position is obscured by a block "above" it
    /// or if the block at the position doesn't have a model.
    #[inline]
    pub fn get_quad_mb(&self, pos_mb: IVec2) -> CqsResult<Option<DataQuad>> {
        let pos = pos_mb.div_euclid(SubdividedBlock::SUBDIVS_VEC);
        let pos_sd = pos_mb.rem_euclid(SubdividedBlock::SUBDIVS_VEC);

        // TODO: we need to be able to get individual microblocks for this to work, fix it!
        let cvo = self.get(pos)?;
        let cvo_above = self.get_above(pos)?;

        let transparent_above = match cvo_above.block {
            CvoBlock::Full(block) => self
                .registry
                .get_by_id(block.id)
                .options
                .transparency
                .is_transparent(),

            CvoBlock::Subdivided(block) => {
                let pos_sd_3d = self.pos_3d_sd(pos_sd).as_uvec3();
                let microblock = block
                    .get(pos_sd_3d)
                    .map_err(|_| CqsError::SubdivBlockAccessOutOfBounds)?;

                self.registry
                    .get_by_id(microblock.id)
                    .options
                    .transparency
                    .is_transparent()
            }
        };

        match cvo.block {
            CvoBlock::Full(block) => {}
            CvoBlock::Subdivided(microblocks) => todo!(),
        }

        todo!()
    }
}

#[cfg(test)]
pub mod tests {}
