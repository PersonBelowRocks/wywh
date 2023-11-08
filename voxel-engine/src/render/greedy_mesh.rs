use bevy::{math::ivec2, prelude::IVec2};

use crate::{
    data::tile::{Face, Transparency, VoxelId},
    render::quad::Quad,
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::Chunk,
    },
};

use super::{adjacency::AdjacentTransparency, quad::PositionedQuad};

pub(crate) trait ChunkAccess: ReadAccess<ReadType = VoxelId> + ChunkBounds {}
impl<T> ChunkAccess for T where T: ReadAccess<ReadType = VoxelId> + ChunkBounds {}

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
    face: Face,
    access: &'a A,
    adjacency: &'b AdjacentTransparency,
    layer: i32,
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
            self.get(pos).ok()?.debug_transparency().is_opaque()
                // && !self.is_masked(pos)?
                && self.get_transparency_above(pos)?.is_transparent(),
        )
    }

    pub fn calculate_quads(&self, buffer: &mut Vec<PositionedQuad>) -> Result<(), A::ReadErr> {
        let mut mask = ChunkSliceMask::default();

        for k in 0..Chunk::SIZE {
            for j in 0..Chunk::SIZE {
                let pos = IVec2::new(k, j);
                if !self.is_meshable(pos).unwrap() || mask.is_masked(pos).unwrap() {
                    continue;
                }

                let quad = Quad::from_points(pos.as_vec2(), pos.as_vec2());

                let mut quad_end = pos;

                let widened = quad.widen_until(1.0, Chunk::SIZE as u32, |n| {
                    let candidate_pos = ivec2(pos.x + n as i32, pos.y);
                    if !self.is_meshable(candidate_pos).unwrap()
                        || mask.is_masked(candidate_pos).unwrap()
                    {
                        quad_end.x = (pos.x + n as i32) - 1;
                        true
                    } else {
                        false
                    }
                });

                let heightened = widened.heighten_until(1.0, Chunk::SIZE as u32, |n| {
                    let mut abort = false;
                    for q_x in pos.x..=quad_end.x {
                        let candidate_pos = ivec2(q_x, pos.y + n as i32);
                        if !self.is_meshable(candidate_pos).unwrap()
                            || mask.is_masked(candidate_pos).unwrap()
                        {
                            quad_end.y = (pos.y + n as i32) - 1;
                            abort = true;
                            break;
                        }
                    }
                    abort
                });

                mask.mask_region(pos, quad_end);

                buffer.push(PositionedQuad {
                    magnitude: self.layer as _,
                    face: self.face,
                    quad: heightened, // widened.heighten(1.0),
                })
            }
        }

        Ok(())
    }
}
