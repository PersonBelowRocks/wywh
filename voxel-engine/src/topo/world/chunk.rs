use std::fmt;

use bevy::math::ivec3;
use bevy::prelude::*;
use bitflags::bitflags;

use parking_lot::RwLock;

use crate::data::registries::block::BlockVariantRegistry;
use crate::data::registries::Registry;
use crate::data::voxel::rotations::BlockModelRotation;
use crate::topo::block::{BlockVoxel, SubdividedBlock};
use crate::topo::bounding_box::BoundingBox;
use crate::topo::controller::{LoadReasons, LoadshareMap};

#[derive(dm::From, dm::Into, dm::Display, Debug, PartialEq, Eq, Hash, Copy, Clone, Component)]
pub struct ChunkPos(IVec3);

impl ChunkPos {
    pub const ZERO: Self = Self(IVec3::ZERO);

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self(ivec3(x, y, z))
    }

    /// The corner of this chunk closest to +infinity
    /// For a `ChunkPos` of `[0, 0, 0]` this would be `[15, 15, 15]`.
    pub fn worldspace_max(self) -> IVec3 {
        (self.0 * Chunk::SIZE) + (Chunk::SIZE - 1)
    }

    /// The corner of this chunk closest to -infinity
    /// For a `ChunkPos` of `[0, 0, 0]` this would be `[0, 0, 0]`.
    pub fn worldspace_min(self) -> IVec3 {
        self.0 * Chunk::SIZE
    }

    pub fn x(self) -> i32 {
        self.0.x
    }

    pub fn y(self) -> i32 {
        self.0.y
    }

    pub fn z(self) -> i32 {
        self.0.z
    }

    pub fn as_ivec3(self) -> IVec3 {
        self.0
    }

    pub fn as_vec3(self) -> Vec3 {
        self.0.as_vec3()
    }
}

bitflags! {
    /// Flags that describe various properties of a chunk
    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    pub struct ChunkFlags: u32 {
        /// Indicates that the chunk is currently being populated by the world generator.
        const GENERATING = 0b1 << 0;
        /// Indicates that the chunk should be remeshed, when the engine remeshes the chunk this flag will
        /// be unset.
        const REMESH = 0b1 << 1;
        // TODO: have flags for each edge that was updated
        /// Indicates that the chunk's neighbors should be remeshed
        const REMESH_NEIGHBORS = 0b1 << 2;
        /// Indicates that this chunk was just generated and has not been meshed before
        const FRESHLY_GENERATED = 0b1 << 3;
        /// Indicates that a chunk has not been populated with the generator and is only really
        /// acting as a "dummy" until it's further processed by the engine.
        /// Chunks are not supposed to be primordial for long, primordial chunks are usually immediately
        /// queued for further processing by the engine to get them out of their primordial state.
        const PRIMORDIAL = 0b1 << 4;
    }
}

impl fmt::Debug for ChunkFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let permit_flag_names = [
            (Self::GENERATING, "GENERATING"),
            (Self::REMESH, "REMESH"),
            (Self::REMESH_NEIGHBORS, "REMESH_NEIGHBORS"),
            (Self::FRESHLY_GENERATED, "FRESHLY_GENERATED"),
            (Self::PRIMORDIAL, "PRIMORDIAL"),
        ];

        let mut list = f.debug_list();

        for (flag, name) in permit_flag_names {
            if self.contains(flag) {
                list.entry(&name);
            }
        }

        list.finish()
    }
}

#[derive(Copy, Clone, Debug, Component, PartialEq, Eq)]
pub struct ChunkEntity;

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, dm::Constructor)]
pub struct VoxelVariantData {
    pub variant: <BlockVariantRegistry as Registry>::Id,
    pub rotation: Option<BlockModelRotation>,
}

#[derive(Clone)]
pub struct ChunkLoadReasons {
    pub loadshares: LoadshareMap<LoadReasons>,
    pub cached_reasons: LoadReasons,
}

impl ChunkLoadReasons {
    /// Updates the cached load reasons and returns them.
    /// Should be called whenever a loadshare updates load reasons.
    pub fn update_cached_reasons(&mut self) -> LoadReasons {
        let mut cached = LoadReasons::empty();

        for &reasons in self.loadshares.values() {
            cached |= reasons;
        }

        self.cached_reasons = cached;
        cached
    }
}

pub struct Chunk {
    pub flags: RwLock<ChunkFlags>,
    pub load_reasons: RwLock<ChunkLoadReasons>,
    pub variants: (), // TODO:
}

const CHUNK_SIZE: usize = 16;

#[allow(dead_code)]
impl Chunk {
    pub const USIZE: usize = CHUNK_SIZE;
    pub const SIZE: i32 = Self::USIZE as i32;
    pub const SIZE_LOG2: u32 = Self::SIZE.ilog2();

    pub const SUBDIVIDED_CHUNK_SIZE: i32 = SubdividedBlock::SUBDIVISIONS * Self::SIZE;
    pub const SUBDIVIDED_CHUNK_USIZE: usize = Self::SUBDIVIDED_CHUNK_SIZE as usize;

    pub const VEC: IVec3 = IVec3::splat(Self::SIZE);

    pub const BOUNDING_BOX: BoundingBox = BoundingBox {
        min: IVec3::splat(0),
        max: IVec3::splat(Self::SIZE),
    };

    #[inline]
    pub fn new(
        filling: BlockVoxel,
        initial_flags: ChunkFlags,
        load_reasons: ChunkLoadReasons,
    ) -> Self {
        Self {
            flags: RwLock::new(initial_flags),
            load_reasons: RwLock::new(load_reasons),
            variants: todo!(), // SyncIndexedChunkContainer::filled(filling),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn chunkpos_to_worldspace() {
        fn test(chunk_pos_splat: i32, min_splat: i32, max_splat: i32) {
            let chunk_pos = ChunkPos::from(IVec3::splat(chunk_pos_splat));

            assert_eq!(chunk_pos.worldspace_min(), IVec3::splat(min_splat));
            assert_eq!(chunk_pos.worldspace_max(), IVec3::splat(max_splat));

            let mut count = 0;
            for _ in min_splat..=max_splat {
                count += 1
            }

            assert_eq!(count, 16)
        }

        test(0, 0, 15);
        test(1, 16, 31);
        test(2, 32, 47);
        test(-1, -16, -1);
        test(-2, -32, -17);
    }
}
