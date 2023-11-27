use bevy::{math::ivec2, prelude::IVec2};

use crate::{
    data::{
        tile::{Face, Transparency},
        voxel::FaceTexture,
    },
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::Chunk,
        chunk_ref::ChunkVoxelOutput,
    },
};

use super::adjacency::AdjacentTransparency;

pub(crate) trait ChunkAccess: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}
impl<T> ChunkAccess for T where T: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}

#[derive(Default)]
pub(crate) struct ChunkSliceMask([[bool; Chunk::USIZE]; Chunk::USIZE]);

impl ChunkSliceMask {
    pub fn contains(pos: IVec2) -> bool {
        pos.cmpge(ivec2(0, 0)).all() && pos.cmplt(ivec2(Chunk::SIZE, Chunk::SIZE)).all()
    }

    pub fn mask(&mut self, pos: IVec2) -> bool {
        if Self::contains(pos) {
            self.0[pos.x as usize][pos.y as usize] = true;

            true
        } else {
            false
        }
    }

    pub fn mask_region(&mut self, from: IVec2, to: IVec2) -> bool {
        if !Self::contains(from) || !Self::contains(to) {
            return false;
        }

        let min = from.min(to);
        let max = from.max(to);

        for x in min.x..=max.x {
            for y in min.y..=max.y {
                self.0[x as usize][y as usize] = true;
            }
        }

        true
    }

    pub fn is_masked(&self, pos: IVec2) -> Option<bool> {
        if Self::contains(pos) {
            Some(self.0[pos.x as usize][pos.y as usize])
        } else {
            None
        }
    }
}

pub(crate) struct VoxelChunkSlice<'a, 'b, A: ChunkAccess> {
    // mask: [[bool; Chunk::USIZE]; Chunk::USIZE],
    pub face: Face,
    access: &'a A,
    adjacency: &'b AdjacentTransparency,
    pub layer: i32,
}

impl<'a, 'b, A: ChunkAccess> VoxelChunkSlice<'a, 'b, A> {
    pub fn new(face: Face, access: &'a A, adjacency: &'b AdjacentTransparency, layer: i32) -> Self {
        Self {
            // mask: [[false; Chunk::USIZE]; Chunk::USIZE],
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

    pub fn get(&self, pos: IVec2) -> Result<ChunkVoxelOutput, A::ReadErr> {
        let pos_3d = self.face.axis().pos_in_3d(pos, self.layer);
        self.access.get(pos_3d)
    }

    pub fn get_texture(&self, pos: IVec2) -> Result<Option<FaceTexture>, A::ReadErr> {
        let pos_3d = self.face.axis().pos_in_3d(pos, self.layer);
        let vox = self.access.get(pos_3d)?;

        Ok(vox.model.map(|m| m.texture(self.face)))
    }

    pub fn get_transparency_above(&self, pos: IVec2) -> Option<Transparency> {
        if !self.contains(pos) {
            return None;
        }

        let pos_3d = self.face.axis().pos_in_3d(pos, self.layer) + self.face.normal();
        let transparency = match self.access.get(pos_3d) {
            Ok(adjacent_voxel) => adjacent_voxel.transparency,
            Err(_) => {
                let pos_in_adjacent_chunk = self.face.pos_on_face(pos_3d);
                self.adjacency.sample(self.face, pos_in_adjacent_chunk)?
            }
        };

        Some(transparency)
    }

    // pub fn is_masked(&self, pos: IVec2) -> Option<bool> {
    //     if !self.contains(pos) {
    //         return None;
    //     }

    //     Some(self.mask[pos.x as usize][pos.y as usize])
    // }

    // TODO: should return a result
    pub fn is_meshable(&self, pos: IVec2) -> Option<bool> {
        if !self.contains(pos) {
            return Some(false);
        }

        Some(
            self.get(pos).ok()?.transparency.is_opaque()
                // && !self.is_masked(pos)?
                && self.get_transparency_above(pos)?.is_transparent(),
        )
    }
}
