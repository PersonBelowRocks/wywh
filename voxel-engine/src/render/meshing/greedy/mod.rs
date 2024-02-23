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
pub mod tests {
    use bevy::math::vec2;
    use parking_lot::{RwLock, RwLockReadGuard};

    use crate::{
        data::{
            registries::{
                texture::{TestTextureRegistryLoader, TextureRegistry},
                variant::VariantRegistryLoader,
            },
            resourcepath::rpath,
            texture::FaceTextureRotation,
            tile::Transparency,
            voxel::descriptor::{
                BlockDescriptor, FaceTextureDescriptor, VariantDescriptor, VoxelModelDescriptor,
            },
        },
        topo::{
            access::{ChunkBounds, HasBounds, ReadAccess},
            neighbors::NeighborsBuilder,
            storage::{data_structures::HashmapChunkStorage, error::OutOfBounds},
        },
        util::FaceMap,
    };

    use super::*;

    pub struct TestAccess
    where
        Self: ChunkAccess,
    {
        pub map: HashmapChunkStorage<ChunkVoxelOutput>,
        pub default: ChunkVoxelOutput,
    }

    impl ChunkBounds for TestAccess {}
    impl ReadAccess for TestAccess {
        type ReadErr = OutOfBounds;
        type ReadType = ChunkVoxelOutput;

        fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
            if !self.bounds().contains(pos) {
                return Err(OutOfBounds);
            }

            Ok(self.map.get(pos).unwrap_or(self.default))
        }
    }

    pub fn testing_registries() -> (BlockVariantRegistry, TextureRegistry) {
        let textures = {
            let mut loader = TestTextureRegistryLoader::new();
            loader.add(rpath("test1"), vec2(0.0, 0.0), None);
            loader.add(rpath("test2"), vec2(0.0, 1.0), None);

            loader.build()
        };

        let mut vloader = VariantRegistryLoader::new();
        vloader.register(
            rpath("var1"),
            BlockDescriptor {
                transparency: Transparency::Opaque,
                directions: FaceMap::new(),
                default: {
                    let mut map = FaceMap::from_fn(|_| {
                        Some(FaceTextureDescriptor::new(
                            rpath("test1"),
                            FaceTextureRotation::new(0),
                        ))
                    });
                    map.set(
                        Face::East,
                        FaceTextureDescriptor::new(rpath("test2"), FaceTextureRotation::new(0)),
                    );
                    map
                },
            },
        );

        vloader.register(
            rpath("void"),
            BlockDescriptor {
                transparency: Transparency::Transparent,
                directions: FaceMap::default(),
                default: FaceMap::default(),
            },
        );

        vloader.register(
            rpath("filled"),
            BlockDescriptor {
                transparency: Transparency::Opaque,
                directions: FaceMap::default(),
                default: FaceMap::default(),
            },
        );

        (vloader.build_registry(&textures).unwrap(), textures)
    }

    #[test]
    fn test_reading() {
        let (varreg, texreg) = testing_registries();

        let variants_lock = RwLock::new(varreg);
        let variants: RegistryRef<BlockVariantRegistry> =
            RwLockReadGuard::map(variants_lock.read(), |g| g);

        let void_cvo = ChunkVoxelOutput {
            variant: variants.get_id(&rpath("void")).unwrap(),
            rotation: None,
        };

        let test_cvo = ChunkVoxelOutput {
            variant: variants.get_id(&rpath("var1")).unwrap(),
            rotation: None,
        };

        let neighbors = {
            let mut builder = NeighborsBuilder::<TestAccess>::new(ChunkVoxelOutput {
                variant: variants.get_id(&rpath("filled")).unwrap(),
                rotation: None,
            });

            builder
                .set_neighbor(
                    ivec3(0, -1, 0),
                    TestAccess {
                        map: HashmapChunkStorage::new(),
                        default: void_cvo,
                    },
                )
                .unwrap();

            builder
                .set_neighbor(
                    ivec3(1, 1, 1),
                    TestAccess {
                        map: HashmapChunkStorage::new(),
                        default: void_cvo,
                    },
                )
                .unwrap();

            builder
                .set_neighbor(
                    ivec3(-1, -1, -1),
                    TestAccess {
                        map: HashmapChunkStorage::new(),
                        default: void_cvo,
                    },
                )
                .unwrap();

            builder.build()
        };

        let access = TestAccess {
            map: {
                let mut storage = HashmapChunkStorage::<ChunkVoxelOutput>::new();

                storage.set(ivec3(8, 0, 8), test_cvo).unwrap();
                storage.set(ivec3(8, 1, 8), test_cvo).unwrap();
                storage.set(ivec3(8, 15, 8), test_cvo).unwrap();
                storage.set(ivec3(0, 0, 0), test_cvo).unwrap();

                storage
            },
            default: void_cvo,
        };

        let mut cqs = ChunkQuadSlice::new(Face::Bottom, 0, &access, &neighbors, &variants).unwrap();

        assert_eq!(None, cqs.get_quad(ivec2(4, 4)).unwrap());

        let expected_texture = texreg.get_id(&rpath("test1")).unwrap();

        assert_eq!(
            expected_texture,
            cqs.get_quad(ivec2(8, 8)).unwrap().unwrap().texture.id
        );

        cqs.reposition(Face::East, 8).unwrap();

        let expected_texture = texreg.get_id(&rpath("test2")).unwrap();

        assert_eq!(
            expected_texture,
            cqs.get_quad(ivec2(8, 1)).unwrap().unwrap().texture.id
        );
    }
}
