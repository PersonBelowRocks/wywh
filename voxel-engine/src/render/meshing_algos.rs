use bevy::prelude::IVec2;
use bevy::prelude::IVec3;
use bevy::prelude::Mesh;
use bevy::prelude::StandardMaterial;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;

use crate::data::tile::Face;
use crate::data::tile::VoxelId;
use crate::render::adjacency::mask_pos_with_face;
use crate::render::quad::Quad;
use crate::topo::access::ChunkBounds;
use crate::topo::access::ReadAccess;
use crate::topo::chunk::Chunk;
use crate::topo::error::ChunkVoxelAccessError;

use super::adjacency::AdjacentTransparency;
use super::error::MesherError;
use super::mesh_builder::Mesher;
use super::mesh_builder::MesherOutput;

#[derive(Clone)]
pub struct SimplePbrMesher;

impl Mesher for SimplePbrMesher {
    type Material = StandardMaterial;

    fn build<Acc>(
        &self,
        access: &Acc,
        adjacency: &AdjacentTransparency,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = VoxelId> + ChunkBounds,
    {
        let mut positions = Vec::<[f32; 3]>::new();
        let mut normals = Vec::<[f32; 3]>::new();
        let mut uvs = Vec::<[f32; 2]>::new();

        let mut indices = Vec::<u32>::new();
        let mut current_idx: u32 = 0;

        for face in Face::FACES {
            for x in 0..Chunk::SIZE {
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        let pos = IVec3::new(x, y, z);
                        let voxel_id = access.get(pos)?;

                        if voxel_id.debug_transparency().is_transparent() {
                            continue;
                        }

                        let adjacent_pos = face.offset_position(pos);
                        let adjacent_transparency = match access.get(adjacent_pos) {
                            Ok(adjacent_voxel_id) => adjacent_voxel_id.debug_transparency(),
                            Err(_) => {
                                let pos_in_adjacent_chunk = mask_pos_with_face(face, adjacent_pos);
                                adjacency.sample(face, pos_in_adjacent_chunk).expect("We're only iterating through 0..16 so the position should be valid")
                            }
                        };

                        if adjacent_transparency.is_transparent() {
                            let pos_on_face = face.pos_on_face(pos);
                            let quad = Quad::from_points(
                                pos_on_face.as_vec2(),
                                (pos_on_face + IVec2::splat(1)).as_vec2(),
                            );

                            // TODO: finish this mesher
                        }
                    }
                }
            }
        }

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.set_indices(Some(Indices::U32(indices)));

        todo!()
    }

    fn material(&self) -> Self::Material {
        todo!()
    }
}

#[derive(Clone)]
pub struct GreedyMesher;

impl Mesher for GreedyMesher {
    // TODO: greedy meshing mat
    type Material = StandardMaterial;

    fn build<Acc>(
        &self,
        access: &Acc,
        adjacency: &AdjacentTransparency,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = VoxelId> + ChunkBounds,
    {
        todo!()
    }

    fn material(&self) -> Self::Material {
        todo!()
    }
}
