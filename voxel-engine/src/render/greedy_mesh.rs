use bevy::prelude::{Axis, IVec2};

use crate::{
    data::tile::{Face, Transparency, VoxelId},
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::Chunk,
    },
    util::Axis3D,
};

use super::adjacency::AdjacentTransparency;

pub(crate) trait Access: ReadAccess<ReadType = VoxelId> + ChunkBounds {}
impl<T> Access for T where T: ReadAccess<ReadType = VoxelId> + ChunkBounds {}

pub(crate) struct VoxelChunkSlice<'a, 'b, A: Access> {
    mask: [[bool; Chunk::USIZE]; Chunk::USIZE],
    face: Face,
    access: &'a A,
    adjacency: &'b AdjacentTransparency,
    layer: i32,
}

impl<'a, 'b, A: Access> VoxelChunkSlice<'a, 'b, A> {
    pub fn new(face: Face, access: &'a A, adjacency: &'b AdjacentTransparency, layer: i32) -> Self {
        Self {
            mask: [[false; Chunk::USIZE]; Chunk::USIZE],
            face,
            access,
            adjacency,
            layer,
        }
    }

    pub fn contains(&self, pos: IVec2) -> bool {
        self.access
            .bounds()
            .contains(self.face.axis().pos_in_3d(pos, self.layer))
    }

    pub fn get(&self, pos: IVec2) -> Result<VoxelId, A::ReadErr> {
        let pos_3d = self.face.axis().pos_in_3d(pos, self.layer);
        self.access.get(pos_3d)
    }

    pub fn get_transparency_above(&self, pos: IVec2) -> Option<Transparency> {
        if !self.contains(pos) {
            return None;
        }

        let pos_3d = self.face.axis().pos_in_3d(pos, self.layer) + self.face.normal();
        let transparency = match self.access.get(pos_3d) {
            Ok(adjacent_voxel_id) => adjacent_voxel_id.debug_transparency(),
            Err(_) => {
                let pos_in_adjacent_chunk = self.face.pos_on_face(pos_3d);
                self.adjacency.sample(self.face, pos_in_adjacent_chunk)?
            }
        };

        Some(transparency)
    }

    pub fn is_masked(&self, pos: IVec2) -> Option<bool> {
        if !self.contains(pos) {
            return None;
        }

        Some(self.mask[pos.x as usize][pos.y as usize])
    }

    pub fn mask(&mut self, from: IVec2, to: IVec2) {
        if !self.contains(from) || !self.contains(to) {
            return;
        }

        let min = from.min(to);
        let max = from.max(to);

        for x in min.x..=max.x {
            for y in min.y..=max.y {
                self.mask[x as usize][y as usize] = true
            }
        }
    }

    pub fn is_meshable(&self, pos: IVec2) -> Option<bool> {
        if !self.contains(pos) {
            return Some(false);
        }

        Some(
            self.get(pos).ok()?.debug_transparency().is_opaque()
                && !self.is_masked(pos)?
                && self.get_transparency_above(pos)?.is_transparent(),
        )
    }
}
