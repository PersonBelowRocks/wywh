use bevy::math::{IVec2, IVec3};

use crate::{
    data::{
        registries::{
            block::{BlockVariantId, BlockVariantRegistry},
            Registry, RegistryRef,
        },
        texture::FaceTexture,
        tile::Face,
        voxel::rotations::BlockModelRotation,
    },
    render::quad::{
        anon::Quad,
        data::DataQuad,
        isometric::{IsometrizedQuad, PositionedQuad, QuadIsometry},
    },
    topo::{
        block::{Microblock, SubdividedBlock},
        ivec_project_to_2d, ivec_project_to_3d,
        neighbors::{self, Neighbors},
        world::{chunk::ChunkReadHandle, Chunk, OutOfBounds},
    },
    util::{
        self, microblock_to_full_block, microblock_to_full_block_3d, microblock_to_subdiv_pos_3d,
        rem_euclid_2_pow_n,
    },
};

use self::error::CqsError;

pub mod algorithm;
pub mod error;
pub mod greedy_mesh;

#[derive(Clone)]
pub struct ChunkQuadSlice<'a, 'chunk> {
    pub face: Face,
    pub mag: i32,

    handle: &'a ChunkReadHandle<'chunk>,
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
        handle: &'a ChunkReadHandle<'chunk>,
        neighbors: &'a Neighbors<'chunk>,
        registry: &'a RegistryRef<'a, BlockVariantRegistry>,
    ) -> Result<Self, OutOfBounds> {
        if 0 > magnitude && magnitude > Chunk::SUBDIVIDED_CHUNK_SIZE {
            return Err(OutOfBounds);
        }

        Ok(Self {
            face,
            mag: magnitude,
            handle,
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

    pub fn mag_at_block_edge(&self) -> bool {
        rem_euclid_2_pow_n(
            self.mag + i32::clamp(self.face.axis_direction(), -1, 0),
            SubdividedBlock::SUBDIVISIONS_LOG2,
        ) == SubdividedBlock::SUBDIVISIONS - 1
    }

    pub fn contains_mb(pos: IVec2) -> bool {
        Self::contains(microblock_to_full_block(pos))
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
            util::floored_div_2_pow_n(self.mag, SubdividedBlock::SUBDIVISIONS_LOG2),
        )
    }

    #[inline]
    pub fn pos_3d_sd(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(
            pos,
            self.face,
            util::rem_euclid_2_pow_n(self.mag, SubdividedBlock::SUBDIVISIONS_LOG2),
        )
    }

    /// Transforms a position from localspace to facespace
    #[inline]
    pub fn pos_2d(&self, pos: IVec3) -> IVec2 {
        ivec_project_to_2d(pos, self.face)
    }

    #[inline]
    pub fn get_3d(&self, pos: IVec3) -> CqsResult<BlockVariantId> {
        Ok(self.handle.get(pos)?)
    }

    #[inline]
    pub fn get_mb_3d(&self, pos_mb: IVec3) -> CqsResult<BlockVariantId> {
        Ok(self.handle.get_mb(pos_mb)?)
    }

    /// `pos` is in localspace and can exceed the regular chunk bounds by 1 for any component of the vector.
    /// In this case the `ChunkAccessOutput` is taken from a neighboring chunk.
    #[inline]
    pub fn auto_neighboring_get(&self, pos: IVec3) -> CqsResult<BlockVariantId> {
        if Self::contains_3d(pos) && !neighbors::is_in_bounds_3d(pos) {
            self.get_3d(pos)
        } else if !Self::contains_3d(pos) && neighbors::is_in_bounds_3d(pos) {
            Ok(self.neighbors.get_3d(pos)?)
        } else {
            return Err(CqsError::OutOfBounds);
        }
    }

    #[inline]
    pub fn auto_neighboring_get_mb(&self, pos_mb: IVec3) -> CqsResult<BlockVariantId> {
        let pos = microblock_to_full_block_3d(pos_mb);

        if Self::contains_3d(pos) && !neighbors::is_in_bounds_3d(pos) {
            self.get_mb_3d(pos_mb)
        } else if !Self::contains_3d(pos) && neighbors::is_in_bounds_3d(pos) {
            Ok(self.neighbors.get_3d_mb(pos_mb)?)
        } else {
            return Err(CqsError::OutOfBounds);
        }
    }

    #[inline]
    pub fn get(&self, pos: IVec2) -> CqsResult<BlockVariantId> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos3d = self.pos_3d(pos);
        self.get_3d(pos3d)
    }

    #[inline]
    pub fn get_mb(&self, pos_mb: IVec2) -> CqsResult<BlockVariantId> {
        if !Self::contains_mb(pos_mb) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_mb_3d = self.pos_3d_mb(pos_mb);
        self.get_mb_3d(pos_mb_3d)
    }

    #[inline]
    pub fn get_above(&self, pos: IVec2) -> CqsResult<BlockVariantId> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_above = self.pos_3d(pos) + self.face.normal();
        self.auto_neighboring_get(pos_above)
    }

    #[inline]
    pub fn get_mb_above(&self, mb_pos: IVec2) -> CqsResult<BlockVariantId> {
        if !Self::contains(mb_pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_mb_above = self.pos_3d_mb(mb_pos) + self.face.normal();
        self.auto_neighboring_get_mb(pos_mb_above)
    }

    /// Get a quad for given position. This function operates on microblock resolution, so the relevant
    /// block for the provided `pos_mb` is at position `pos_mb / 4` in chunkspace.
    /// Returns `None` if the microblock at the position is obscured by a block "above" it
    /// or if the block at the position doesn't have a model.
    #[inline]
    pub fn get_quad_mb(&self, pos_mb: IVec2) -> CqsResult<Option<DataQuad>> {
        let microblock = self.get_mb(pos_mb)?;
        let microblock_above = self.get_mb_above(pos_mb)?;

        let entry = self.registry.get_by_id(microblock);
        let entry_above = self.registry.get_by_id(microblock_above);

        if entry.options.transparency.is_transparent()
            || entry_above.options.transparency.is_opaque()
        {
            return Ok(None);
        }

        let Some(model) = entry.model else {
            return Ok(None);
        };

        // TODO: get rid of submodels completely, we should register all possible rotations
        // on startup and not calculate anything during runtime
        let submodel = model.default_submodel();
        let texture = submodel.texture(self.face);

        Ok(Some(DataQuad::new(Quad::ONE, texture)))
    }
}
