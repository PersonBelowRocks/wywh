use bevy::{math::ivec2, prelude::IVec2};

use crate::{
    data::{
        registries::{variant::VariantRegistry, Registry, RegistryRef},
        texture::{FaceTexture, FaceTextureRotation},
        tile::{Face, Transparency},
        voxel::rotations::BlockModelRotation,
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
    default_rotation: BlockModelRotation,

    pub face: Face,
    pub layer: i32,

    access: &'a A,
    adjacency: &'a AdjacentTransparency,
    registry: &'b RegistryRef<'a, VariantRegistry>,
}

impl<'a, 'b, A: ChunkAccess> VoxelChunkSlice<'a, 'b, A> {
    pub fn new(
        face: Face,
        layer: i32,
        access: &'a A,
        adjacency: &'a AdjacentTransparency,
        registry: &'b RegistryRef<'a, VariantRegistry>,
    ) -> Self {
        Self {
            default_rotation: BlockModelRotation::new(Face::North, Face::Top).unwrap(),

            face,
            layer,

            access,
            adjacency,
            registry,
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

        let rotation = vox.rotation.unwrap_or(self.default_rotation);

        let variant = self.registry.get_by_id(vox.variant);

        Ok(variant
            .model
            .and_then(|vm| vm.as_block_model())
            .and_then(|bm| bm.faces_for_rotation(rotation).get(self.face).copied())
            .map(|mut tex| {
                tex.rotation += if self.face.is_vertical() {
                    FaceTextureRotation::new(2)
                } else {
                    FaceTextureRotation::new(1)
                };
                tex
            }))
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
