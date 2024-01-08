use bevy::math::{ivec3, IVec2, IVec3};

use crate::{
    data::tile::Face,
    render::meshing::greedy::ivec_project_to_3d,
    topo::{
        access::{ChunkBounds, ReadAccess},
        bounding_box::BoundingBox,
        chunk::Chunk,
        chunk_ref::ChunkVoxelOutput,
        storage::error::OutOfBounds,
    },
    util::{self, ConversionError},
};

use super::{access::ChunkAccess, error::NeighborsAccessError};

fn ivec3_to_1d(v: IVec3) -> Result<usize, ConversionError> {
    let [x, y, z] = util::try_ivec3_to_usize_arr(v)?;
    Ok(to_1d(x, y, z))
}

fn to_1d(x: usize, y: usize, z: usize) -> usize {
    const MAX: usize = 3;
    return (z * MAX * MAX) + (y * MAX) + x;
}

fn localspace_to_chunk_pos(pos: IVec3) -> IVec3 {
    ivec3(
        pos.x.div_euclid(Chunk::SIZE),
        pos.y.div_euclid(Chunk::SIZE),
        pos.z.div_euclid(Chunk::SIZE),
    )
}

fn localspace_to_neighbor_localspace(pos: IVec3) -> IVec3 {
    ivec3(
        pos.x.rem_euclid(Chunk::SIZE),
        pos.y.rem_euclid(Chunk::SIZE),
        pos.z.rem_euclid(Chunk::SIZE),
    )
}

// TODO: document what localspace, worldspace, chunkspace, and facespace are
#[derive(Clone)]
pub struct Neighbors<C: ChunkAccess> {
    chunks: [Option<C>; 3 * 3 * 3],
    default: ChunkVoxelOutput,
}

/// Test if the provided facespace vector is in bounds
pub fn is_in_bounds(pos: IVec2) -> bool {
    let min: IVec2 = -IVec2::ONE;
    let max: IVec2 = IVec2::splat(Chunk::SIZE) + IVec2::ONE;

    pos.cmpge(min).all() && pos.cmplt(max).all()
}

/// Test if the provided localspace vector is in bounds
pub fn is_in_bounds_3d(pos: IVec3) -> bool {
    let min: IVec3 = -IVec3::ONE;
    let max: IVec3 = IVec3::splat(Chunk::SIZE) + IVec3::ONE;

    pos.cmpge(min).all() && pos.cmplt(max).all()
}

pub type NbResult<T, E> = Result<T, NeighborsAccessError<E>>;

impl<C: ChunkAccess> Neighbors<C> {
    /// `pos` is in localspace
    fn internal_get(&self, pos: IVec3) -> NbResult<C::ReadType, C::ReadErr> {
        let chk_pos = localspace_to_chunk_pos(pos);

        if chk_pos == IVec3::ZERO {
            // tried to access center chunk (aka. the chunk for which we represent the neighbors)
            return Err(NeighborsAccessError::OutOfBounds);
        }

        let chk_index =
            ivec3_to_1d(chk_pos + IVec3::ONE).map_err(|_| NeighborsAccessError::OutOfBounds)?;
        let chk = self
            .chunks
            .get(chk_index)
            .ok_or(NeighborsAccessError::OutOfBounds)?;

        match chk {
            Some(access) => {
                let neighbor_local = localspace_to_neighbor_localspace(pos);
                Ok(access.get(neighbor_local)?)
            }
            None => Ok(self.default),
        }
    }

    /// `pos` in facespace
    pub fn get(&self, face: Face, pos: IVec2) -> NbResult<C::ReadType, C::ReadErr> {
        if !is_in_bounds(pos) {
            return Err(NeighborsAccessError::OutOfBounds);
        }

        let pos_3d = {
            let mut mag = face.axis_direction();
            if mag > 0 {
                mag = Chunk::SIZE;
            }

            ivec_project_to_3d(pos, face, mag)
        };

        self.internal_get(pos_3d)
    }

    /// `pos` in localspace
    pub fn get_3d(&self, pos: IVec3) -> NbResult<C::ReadType, C::ReadErr> {
        if !is_in_bounds_3d(pos) {
            return Err(NeighborsAccessError::OutOfBounds);
        }

        self.internal_get(pos)
    }
}

fn is_valid_neighbor_chunk_pos(pos: IVec3) -> bool {
    const BB: BoundingBox = BoundingBox {
        min: IVec3::splat(-1),
        max: IVec3::ONE,
    };

    pos != IVec3::ZERO && BB.contains_inclusive(pos)
}

#[derive(Clone)]
pub struct NeighborsBuilder<C: ChunkAccess>(Neighbors<C>);

impl<C: ChunkAccess> NeighborsBuilder<C> {
    pub fn new(default: ChunkVoxelOutput) -> Self {
        Self(Neighbors {
            chunks: Default::default(),
            default,
        })
    }

    pub fn set_neighbor(&mut self, pos: IVec3, access: C) -> Result<(), OutOfBounds> {
        if !is_valid_neighbor_chunk_pos(pos) {
            return Err(OutOfBounds);
        }

        let idx = ivec3_to_1d(pos + IVec3::ONE).map_err(|_| OutOfBounds)?;

        let slot = self.0.chunks.get_mut(idx).ok_or(OutOfBounds)?;
        *slot = Some(access);

        Ok(())
    }

    pub fn build(self) -> Neighbors<C> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::ivec2;

    use crate::{
        data::{registries::RegistryId, tile::Transparency},
        topo::access::{self, HasBounds},
    };

    use super::*;

    fn make_cvo(id: u64) -> ChunkVoxelOutput {
        ChunkVoxelOutput {
            transparency: Transparency::Opaque,
            variant: RegistryId::new(id),
            rotation: None,
        }
    }

    struct TestAccess {
        even: u64,
        odd: u64,
    }

    impl ChunkBounds for TestAccess {}
    impl access::ReadAccess for TestAccess {
        type ReadErr = OutOfBounds;
        type ReadType = ChunkVoxelOutput;

        fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
            if !self.bounds().contains(pos) {
                return Err(OutOfBounds);
            }

            if (pos % 2).cmpeq(IVec3::ZERO).any() {
                Ok(make_cvo(self.even))
            } else {
                Ok(make_cvo(self.odd))
            }
        }
    }

    fn make_test_neighbors() -> Neighbors<TestAccess> {
        let mut builder = NeighborsBuilder::<TestAccess>::new(make_cvo(0));

        // FACES

        builder
            .set_neighbor(ivec3(1, 0, 0), TestAccess { even: 1, odd: 2 })
            .unwrap();

        builder
            .set_neighbor(ivec3(-1, 0, 0), TestAccess { even: 3, odd: 4 })
            .unwrap();

        builder
            .set_neighbor(ivec3(0, 1, 0), TestAccess { even: 5, odd: 6 })
            .unwrap();

        builder
            .set_neighbor(ivec3(0, -1, 0), TestAccess { even: 7, odd: 8 })
            .unwrap();

        builder
            .set_neighbor(ivec3(0, 0, 1), TestAccess { even: 9, odd: 10 })
            .unwrap();

        builder
            .set_neighbor(ivec3(0, 0, -1), TestAccess { even: 11, odd: 12 })
            .unwrap();

        // EDGES

        builder
            .set_neighbor(ivec3(1, 1, 0), TestAccess { even: 20, odd: 21 })
            .unwrap();

        builder
            .set_neighbor(ivec3(-1, 1, 0), TestAccess { even: 22, odd: 23 })
            .unwrap();

        builder
            .set_neighbor(ivec3(0, 1, 1), TestAccess { even: 24, odd: 25 })
            .unwrap();

        builder
            .set_neighbor(ivec3(0, 1, -1), TestAccess { even: 26, odd: 27 })
            .unwrap();

        // CORNERS

        builder
            .set_neighbor(ivec3(1, 1, 1), TestAccess { even: 30, odd: 31 })
            .unwrap();

        builder
            .set_neighbor(ivec3(-1, 1, 1), TestAccess { even: 32, odd: 33 })
            .unwrap();

        builder
            .set_neighbor(ivec3(1, 1, -1), TestAccess { even: 34, odd: 35 })
            .unwrap();

        builder
            .set_neighbor(ivec3(-1, 1, -1), TestAccess { even: 36, odd: 37 })
            .unwrap();

        builder.build()
    }

    #[test]
    fn test_builder() {
        const DUMMY: TestAccess = TestAccess { even: 0, odd: 0 };

        let mut builder = NeighborsBuilder::<TestAccess>::new(make_cvo(0));

        assert!(builder.set_neighbor(ivec3(0, 0, 0), DUMMY).is_err());
        assert!(builder.set_neighbor(ivec3(1, 1, 1), DUMMY).is_ok());
        assert!(builder.set_neighbor(ivec3(-1, -1, -1), DUMMY).is_ok());
        assert!(builder.set_neighbor(ivec3(-1, -2, -1), DUMMY).is_err());
    }

    #[test]
    fn test_neighbors() {
        let neighbors = make_test_neighbors();

        dbg!(ivec3(0, 0, 0) % 2);

        assert_eq!(
            0,
            neighbors
                .get(Face::Bottom, ivec2(-1, 0))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            7,
            neighbors
                .get(Face::Bottom, ivec2(0, 0))
                .unwrap()
                .variant
                .inner()
        );
        assert_eq!(
            8,
            neighbors
                .get(Face::Bottom, ivec2(1, 1))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            7,
            neighbors
                .get(Face::Bottom, ivec2(6, 10))
                .unwrap()
                .variant
                .inner()
        );
        assert_eq!(
            8,
            neighbors
                .get(Face::Bottom, ivec2(5, 5))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            0,
            neighbors
                .get(Face::Bottom, ivec2(16, 16))
                .unwrap()
                .variant
                .inner()
        );

        assert!(neighbors.get(Face::Bottom, ivec2(16, 17)).is_err());
        assert!(neighbors.get(Face::Bottom, ivec2(-2, 5)).is_err());

        // EDGES

        assert_eq!(
            5,
            neighbors
                .get(Face::Top, ivec2(0, 0))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            20,
            neighbors
                .get(Face::Top, ivec2(16, 5))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            20,
            neighbors
                .get(Face::North, ivec2(6, 16))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            1,
            neighbors
                .get(Face::North, ivec2(6, 6))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            22,
            neighbors
                .get(Face::Top, ivec2(-1, 5))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            24,
            neighbors
                .get(Face::Top, ivec2(5, 16))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            26,
            neighbors
                .get(Face::Top, ivec2(5, -1))
                .unwrap()
                .variant
                .inner()
        );

        // CORNERS

        assert_eq!(
            30,
            neighbors
                .get(Face::Top, ivec2(16, 16))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            32,
            neighbors
                .get(Face::Top, ivec2(-1, 16))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            34,
            neighbors
                .get(Face::Top, ivec2(16, -1))
                .unwrap()
                .variant
                .inner()
        );

        assert_eq!(
            36,
            neighbors
                .get(Face::Top, ivec2(-1, -1))
                .unwrap()
                .variant
                .inner()
        );
    }

    #[test]
    fn test_neighbors_3d() {
        let neighbors = make_test_neighbors();

        assert_eq!(
            1,
            neighbors.get_3d(ivec3(16, 5, 5)).unwrap().variant.inner()
        );
        assert!(neighbors.get_3d(ivec3(17, 5, 5)).is_err());
        assert!(neighbors.get_3d(ivec3(5, 5, 5)).is_err());

        assert_eq!(
            4,
            neighbors.get_3d(ivec3(-1, 5, 5)).unwrap().variant.inner()
        );
    }

    #[test]
    fn test_chunkspace_to_chunk_pos() {
        // for readability's sake
        fn f(x: i32, y: i32, z: i32) -> IVec3 {
            localspace_to_chunk_pos(ivec3(x, y, z))
        }

        assert_eq!(ivec3(0, 0, 0), f(8, 5, 6));
        assert_eq!(ivec3(0, 0, 0), f(0, 0, 0));
        assert_eq!(ivec3(0, 0, 0), f(15, 15, 15));
        assert_eq!(ivec3(0, 0, 1), f(15, 15, 16));
        assert_eq!(ivec3(0, -1, 0), f(0, -1, 0));
        assert_eq!(ivec3(0, -1, 1), f(0, -1, 16));
    }

    #[test]
    fn test_move_pos_origin() {
        // for readability's sake
        fn f(x: i32, y: i32, z: i32) -> IVec3 {
            localspace_to_neighbor_localspace(ivec3(x, y, z))
        }

        assert_eq!(ivec3(5, 5, 5), f(5, 5, 5));
        assert_eq!(ivec3(0, 0, 0), f(0, 0, 0));
        assert_eq!(ivec3(0, 15, 0), f(0, -1, 0));
        assert_eq!(ivec3(0, 0, 5), f(0, 16, 5));
    }
}
