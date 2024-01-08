use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{
    data::{
        registries::{
            texture::TextureRegistry, variant::VariantRegistry, Registry, RegistryId, RegistryRef,
        },
        tile::{Face, Transparency},
        voxel::VoxelModel,
    },
    render::{
        adjacency::AdjacentTransparency,
        quad::{anon::Quad, data::DataQuad},
    },
    topo::{
        access::ChunkAccess,
        chunk::Chunk,
        chunk_ref::ChunkVoxelOutput,
        neighbors::{self, Neighbors},
    },
    util::Axis3D,
};

use self::error::CqsError;

pub mod algorithm;
pub mod error;
pub mod greedy_mesh;
pub mod material;

#[inline]
pub fn ivec_project_to_3d(pos: IVec2, face: Face, mag: i32) -> IVec3 {
    match face.axis() {
        Axis3D::X => ivec3(mag, pos.y, pos.x),
        Axis3D::Y => ivec3(pos.x, mag, pos.y),
        Axis3D::Z => ivec3(pos.x, pos.y, mag),
    }
}

#[inline]
pub fn ivec_project_to_2d(pos: IVec3, face: Face) -> IVec2 {
    match face.axis() {
        Axis3D::X => ivec2(pos.z, pos.y),
        Axis3D::Y => ivec2(pos.x, pos.z),
        Axis3D::Z => ivec2(pos.x, pos.y),
    }
}

#[derive(Clone)]
#[allow(private_bounds)]
pub struct ChunkQuadSlice<'a, C: ChunkAccess, Nb: ChunkAccess> {
    face: Face,
    mag: i32,

    access: C,
    neighbors: Neighbors<Nb>,
    registry: &'a RegistryRef<'a, VariantRegistry>,
}

pub const MAX: IVec2 = IVec2::splat(Chunk::SIZE);

#[allow(type_alias_bounds)]
pub type CqsResult<T, C: ChunkAccess, Nb: ChunkAccess> =
    Result<T, CqsError<C::ReadErr, Nb::ReadErr>>;

impl<'a, C: ChunkAccess, Nb: ChunkAccess> ChunkQuadSlice<'a, C, Nb> {
    pub fn contains(pos: IVec2) -> bool {
        pos.cmplt(MAX).all() && pos.cmpge(IVec2::ZERO).all()
    }

    pub fn contains_3d(pos: IVec3) -> bool {
        pos.cmplt(Chunk::VEC).all() && pos.cmpge(IVec3::ZERO).all()
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

        if cvo.transparency.is_transparent() || self.get_above(pos)?.transparency.is_opaque() {
            // nothing to see here if we're either transparent or the block above is obscuring us
            return Ok(None);
        }

        let variant = self.registry.get_by_id(cvo.variant);
        if let Some(VoxelModel::Block(model)) = variant.model {
            let submodel = match cvo.rotation {
                Some(rotation) => model.submodel(rotation.front()),
                None => model.default_submodel(),
            };

            let texture = submodel.get_texture(self.face);
            let mut quad = DataQuad::new(Quad::ONE, texture);

            // TODO: calculate occlusion

            Ok(Some(quad))
        } else {
            // can only get quads from a block model
            Ok(None)
        }
    }
}
