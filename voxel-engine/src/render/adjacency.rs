use bevy::prelude::{IVec2, IVec3};

use crate::data::tile::Transparency;
use crate::data::tile::{Face, VoxelId};
use crate::topo::access::ReadAccess;
use crate::topo::chunk::{Chunk, ChunkPos};
use crate::topo::error::ChunkVoxelAccessError;
use crate::topo::realm::ChunkManager;
use crate::util::SquareArray;

struct ChunkFace<T> {
    data: SquareArray<{ Chunk::USIZE }, T>,
    face: Face,
}

impl<T: Default + Copy> ChunkFace<T> {
    pub fn new(face: Face) -> Self {
        Self {
            data: [[T::default(); Chunk::USIZE]; Chunk::USIZE],
            face,
        }
    }

    pub fn set(&mut self, pos: IVec2, value: T) -> bool {
        if (0..16).contains(&pos.x) && (0..16).contains(&pos.y) {
            self.data[pos.x as usize][pos.y as usize] = value;
            true
        } else {
            false
        }
    }

    pub fn get(&self, pos: IVec2) -> Option<T> {
        if (0..16).contains(&pos.x) && (0..16).contains(&pos.y) {
            Some(self.data[pos.x as usize][pos.y as usize])
        } else {
            None
        }
    }
}

#[inline(always)]
fn adjacent_chunk_vxl_index(face: Face, k: i32, j: i32) -> IVec3 {
    let pos = {
        let p = face.opposite().offset_position([0, 0, 0].into());

        (p * 16).clamp(IVec3::splat(0), IVec3::splat(15))
    };

    match face {
        Face::North | Face::South => [pos.x, k, j], // x
        Face::Top | Face::Bottom => [k, pos.y, j],  // y
        Face::West | Face::East => [k, j, pos.z],   // z
    }
    .into()
}

#[inline(always)]
pub(crate) fn mask_pos_with_face(face: Face, pos: IVec3) -> IVec2 {
    // let offset = face.offset();

    let (k, j) = match face {
        Face::North | Face::South => (pos.y, pos.z),
        Face::Top | Face::Bottom => (pos.x, pos.z),
        Face::West | Face::East => (pos.x, pos.y),
    };

    IVec2::new(k, j)
}

pub(crate) fn voxel_id_to_transparency_debug(id: VoxelId) -> Transparency {
    match u32::from(id) {
        0 => Transparency::Transparent,
        _ => Transparency::Opaque,
    }
}

fn transparency_for_adjacent_chunk_face(
    access: impl ReadAccess<ReadType = VoxelId, ReadErr = ChunkVoxelAccessError>,
    face: Face,
) -> Option<ChunkFace<Transparency>> {
    let mut chunk_face_transparency = ChunkFace::<Transparency>::new(face);

    for k in 0..(Chunk::USIZE as i32) {
        for j in 0..(Chunk::USIZE as i32) {
            let position = adjacent_chunk_vxl_index(face, k, j);

            let result = access.get(position);

            if let Err(ChunkVoxelAccessError::NotInitialized) = result {
                return None;
            }

            let voxel_id = result.expect("Result should be okay'd by previous checks.");
            let transparency = voxel_id_to_transparency_debug(voxel_id);

            chunk_face_transparency.set([k, j].into(), transparency);
        }
    }

    Some(chunk_face_transparency)
}

pub struct AdjacentTransparency([Option<ChunkFace<Transparency>>; 6]);

impl AdjacentTransparency {
    #[inline]
    pub fn sample(&self, face: Face, pos: IVec2) -> Option<Transparency> {
        if pos.clamp(IVec2::splat(0), IVec2::splat(Chunk::USIZE as i32)) != pos {
            return None;
        }

        let chunk_face = match &self.0[face as usize] {
            Some(chunk_face) => chunk_face,
            None => return Some(Transparency::Opaque),
        };

        Some(chunk_face.data[pos.x as usize][pos.y as usize])
    }

    pub fn new(pos: ChunkPos, manager: &ChunkManager) -> Self {
        let vec: IVec3 = pos.into();
        let mut chunks: [Option<ChunkFace<Transparency>>; 6] = Default::default();

        for (mut_chunk_face, face) in chunks.iter_mut().zip(Face::FACES.into_iter()) {
            let chunk_ref = manager
                .get_loaded_chunk(face.offset_position(vec).into())
                .ok();

            *mut_chunk_face = chunk_ref.and_then(|r| {
                r.with_read_access(|access| transparency_for_adjacent_chunk_face(access, face))
                    .ok()
                    .flatten()
            })
        }

        Self(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_chunk_pos_logic() {
        fn algo(face: Face, k: i32, j: i32) -> IVec3 {
            let pos = {
                let p = face.opposite().offset_position([0, 0, 0].into());

                (p * 16).clamp(IVec3::splat(0), IVec3::splat(15))
            };

            match face {
                Face::North | Face::South => [pos.x, k, j], // x
                Face::Top | Face::Bottom => [k, pos.y, j],  // y
                Face::West | Face::East => [k, j, pos.z],   // z
            }
            .into()
        }

        let pos = algo(Face::Top, 5, 4);
        assert_eq!(pos, [5, 0, 4].into());

        let pos = algo(Face::Bottom, 3, 2);
        assert_eq!(pos, [3, 15, 2].into());

        let pos = algo(Face::North, 4, 4);
        assert_eq!(pos, [0, 4, 4].into());

        let pos = algo(Face::South, 9, 1);
        assert_eq!(pos, [15, 9, 1].into());

        let pos = algo(Face::West, 2, 7);
        assert_eq!(pos, [2, 7, 15].into());

        let pos = algo(Face::East, 6, 5);
        assert_eq!(pos, [6, 5, 0].into());
    }
}
