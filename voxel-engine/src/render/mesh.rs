use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, MeshVertexAttribute, VertexAttributeValues},
        render_resource::{PrimitiveTopology, VertexFormat},
    },
};

use crate::{
    data::tile::Transparency,
    render::{
        adjacency::{mask_pos_with_face, voxel_id_to_transparency_debug},
        vertex::VoxelFaceVertexData,
    },
    topo::{
        access::ReadAccess,
        chunk::ChunkPos,
        chunk_ref::{ChunkRef, ChunkRefVxlReadAccess},
        error::ChunkVoxelAccessError,
        realm::ChunkManager,
    },
};

use crate::data::tile::{Face, VoxelId};
use crate::topo::chunk::Chunk;

use super::adjacency::AdjacentTransparency;

pub struct ChunkMesh {
    pos: ChunkPos,
    mesh: Mesh,
}

impl From<ChunkMesh> for Mesh {
    fn from(value: ChunkMesh) -> Self {
        value.mesh
    }
}

#[allow(clippy::inconsistent_digit_grouping)]
#[allow(dead_code)]
impl ChunkMesh {
    pub const VOXEL_DATA_ATTR_ID: usize = 4099_0;

    pub const VOXEL_DATA_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("Voxel_Data", Self::VOXEL_DATA_ATTR_ID, VertexFormat::Uint32);

    pub fn build(chunk: &ChunkRef, adjacency: &AdjacentTransparency) -> Self {
        chunk
            .with_read_access(|access| Self {
                mesh: Self::build_mesh(&access, adjacency),
                pos: chunk.pos(),
            })
            .unwrap()
    }

    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    fn build_mesh(access: &ChunkRefVxlReadAccess, adjacency: &AdjacentTransparency) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        let mut voxel_data: Vec<u32> = vec![];
        let mut indices: Vec<u32> = vec![];
        let mut current_idx: u32 = 0;

        const AIR_VOXEL_ID: VoxelId = VoxelId::new(0);

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let pos: IVec3 = [x, y, z].into();

                    if access.get(pos) == Ok(AIR_VOXEL_ID) {
                        continue;
                    }

                    for face in Face::FACES {
                        let adjacent = face.offset_position(pos);

                        debug_assert!((adjacent - pos).abs().dot(IVec3::splat(1)) == 1);

                        let adjacent_transparency = match access.get(adjacent) {
                            Ok(voxel_id) => voxel_id_to_transparency_debug(voxel_id),
                            Err(ChunkVoxelAccessError::OutOfBounds) => {
                                let pos_in_adjacent_chunk = mask_pos_with_face(face, adjacent);
                                let transparency = adjacency.sample(face, pos_in_adjacent_chunk).expect("We're only iterating through 0..16 so the position should be valid");
                                if transparency.is_transparent() {
                                    println!("Found transparent")
                                }
                                transparency
                            }
                            Err(error) => {
                                panic!("Access returned error {0} while building mesh", error)
                            }
                        };

                        if adjacent_transparency.is_transparent() {
                            // TODO: extract the face vertex logic into an own struct or something
                            for c in 0..4 {
                                let data = VoxelFaceVertexData {
                                    face,
                                    corner: c,
                                    vxl_pos: pos,
                                    texture_pos: [0, 0].into(),
                                };

                                voxel_data.push(data.pack().unwrap())
                            }

                            let indices_pattern = [0u32, 1, 2, 3, 2, 1]
                                .into_iter()
                                .map(|idx| idx + current_idx)
                                .collect::<Vec<_>>();

                            indices.extend_from_slice(&indices_pattern);

                            current_idx += 4;
                        }
                    }
                }
            }
        }

        let vertices = voxel_data.len();

        mesh.set_indices(Some(Indices::U32(indices)));
        mesh.insert_attribute(
            Self::VOXEL_DATA_ATTR,
            VertexAttributeValues::Uint32(voxel_data),
        );

        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            (0..vertices)
                .map(|_| {
                    [
                        rand::random::<f32>(),
                        rand::random::<f32>(),
                        rand::random::<f32>(),
                    ]
                })
                .collect::<Vec<_>>(),
        );

        mesh
    }
}
