use bevy::{prelude::*, render::{render_resource::{PrimitiveTopology, VertexFormat}, mesh::{MeshVertexAttribute, VertexAttributeValues, Indices}}};

use crate::{chunk::Chunk, registry::TextureRegistry, tile::{Face, VoxelId}, render::vertex::VoxelFaceVertexData};

pub struct AdjacentChunks<'a> {
    adjacent: hb::HashMap<Face, &'a Chunk>
}

impl<'a> AdjacentChunks<'a> {
    pub fn new() -> Self {
        Self {
            adjacent: Default::default()
        }
    }
}

pub struct ChunkMesh {
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

    pub const VOXEL_DATA_ATTR: MeshVertexAttribute = MeshVertexAttribute::new(
        "Voxel_Data", 
        Self::VOXEL_DATA_ATTR_ID, 
        VertexFormat::Uint32);

    pub fn build(chunk: &Chunk) -> Option<Self> {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        let mut voxel_data: Vec<u32> = vec![];
        let mut indices: Vec<u32> = vec![];
        let mut current_idx: u32 = 0;

        const AIR_VOXEL_ID: VoxelId = VoxelId::new(0);

        for x in 0..(Chunk::SIZE as i32) {
            for y in 0..(Chunk::SIZE as i32) {
                for z in 0..(Chunk::SIZE as i32) {
                    let pos: IVec3 = [x, y, z].into();

                    if chunk.get_voxel(pos) == Some(&AIR_VOXEL_ID) {
                        continue
                    }

                    for face in Face::FACES {
                        let adjacent = face.get_position_offset(pos);
                        
                        let vox = chunk.get_voxel(adjacent);
                        if vox.filter(|&t| *t != AIR_VOXEL_ID).is_none() {
                            for c in 0..4 {
                                let data = VoxelFaceVertexData {
                                    face,
                                    corner: c,
                                    vxl_pos: pos,
                                    texture_pos: [0, 0].into()
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
            VertexAttributeValues::Uint32(voxel_data)
        );

        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            (0..vertices).map(|_| [
                rand::random::<f32>(),
                rand::random::<f32>(),
                rand::random::<f32>(),
            ]).collect::<Vec<_>>()
        );

        // mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0., 0., 0.]; vertices]);
        
        Some(Self {
            mesh
        })
    }
}