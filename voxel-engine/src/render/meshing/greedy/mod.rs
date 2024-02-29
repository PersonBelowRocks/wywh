use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{
    data::{
        registries::{variant::BlockVariantRegistry, Registry, RegistryRef},
        tile::Face,
        voxel::VoxelModel,
    },
    render::quad::{
        anon::Quad,
        data::DataQuad,
        isometric::{IsometrizedQuad, PositionedQuad, QuadIsometry},
    },
    topo::{
        access::ChunkAccess,
        chunk::Chunk,
        chunk_ref::ChunkVoxelOutput,
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
pub struct ChunkQuadSlice<'a, C: ChunkAccess, Nb: ChunkAccess> {
    face: Face,
    mag: i32,

    access: &'a C,
    neighbors: &'a Neighbors<Nb>,
    registry: &'a RegistryRef<'a, BlockVariantRegistry>,
}

pub const MAX: IVec2 = IVec2::splat(Chunk::SIZE);

#[allow(type_alias_bounds)]
pub type CqsResult<T, C: ChunkAccess, Nb: ChunkAccess> =
    Result<T, CqsError<C::ReadErr, Nb::ReadErr>>;

impl<'a, C: ChunkAccess, Nb: ChunkAccess> ChunkQuadSlice<'a, C, Nb> {
    #[inline]
    pub fn new(
        face: Face,
        magnitude: i32,
        access: &'a C,
        neighbors: &'a Neighbors<Nb>,
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

    pub fn reposition(&mut self, face: Face, magnitude: i32) -> Result<(), OutOfBounds> {
        if 0 > magnitude && magnitude > Chunk::SIZE {
            return Err(OutOfBounds);
        }

        self.face = face;
        self.mag = magnitude;

        Ok(())
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

    /// Transforms a position from localspace to facespace
    #[inline]
    pub fn pos_2d(&self, pos: IVec3) -> IVec2 {
        ivec_project_to_2d(pos, self.face)
    }

    #[inline]
    pub fn get_3d(&self, pos: IVec3) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        self.access.get(pos).map_err(|e| CqsError::AccessError(e))
    }

    /// `pos` is in localspace and can exceed the regular chunk bounds by 1 for any component of the vector.
    /// In this case the `ChunkVoxelOutput` is taken from a neighboring chunk.
    #[inline]
    pub fn auto_neighboring_get(&self, pos: IVec3) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        if Self::contains_3d(pos) && !neighbors::is_in_bounds_3d(pos) {
            self.get_3d(pos)
        } else if !Self::contains_3d(pos) && neighbors::is_in_bounds_3d(pos) {
            Ok(self.neighbors.get_3d(pos)?)
        } else {
            return Err(CqsError::OutOfBounds);
        }
    }

    #[inline]
    pub fn get(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos3d = self.pos_3d(pos);
        self.get_3d(pos3d)
    }

    #[inline]
    pub fn get_above(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_above = self.pos_3d(pos) + self.face.normal();
        self.auto_neighboring_get(pos_above)
    }

    #[inline(always)]
    pub fn get_quad(&self, pos: IVec2) -> CqsResult<Option<DataQuad>, C, Nb> {
        let cvo = self.get(pos)?;
        let transparency = self.registry.get_by_id(cvo.variant).transparency;

        let cvo_above = self.get_above(pos)?;
        let transparency_above = self.registry.get_by_id(cvo_above.variant).transparency;

        if transparency.is_transparent() || transparency_above.is_opaque() {
            // nothing to see here if we're either transparent or the block above is obscuring us
            return Ok(None);
        }

        let variant = self.registry.get_by_id(cvo.variant);
        if let Some(model) = variant.model {
            let submodel = match cvo.rotation {
                Some(rotation) => model.submodel(rotation.front()),
                None => model.default_submodel(),
            };

            let texture = submodel.get_texture(self.face);
            let quad = DataQuad::new(Quad::ONE, texture);

            Ok(Some(quad))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
pub mod tests {}
