use bevy::math::ivec2;
use bevy::math::vec2;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::default;
use bevy::prelude::Color;
use bevy::prelude::Handle;
use bevy::prelude::IVec2;
use bevy::prelude::IVec3;
use bevy::prelude::Image;
use bevy::prelude::Mesh;
use bevy::prelude::Rect;
use bevy::prelude::StandardMaterial;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;

use crate::data::registry::Registries;
use crate::data::tile::Face;
use crate::data::tile::VoxelId;
use crate::data::voxel::FaceTextureRotation;
use crate::render::adjacency::mask_pos_with_face;
use crate::render::greedy_mesh::VoxelChunkSlice;
use crate::render::quad::Quad;
use crate::topo::access::ChunkBounds;
use crate::topo::access::ReadAccess;
use crate::topo::chunk::Chunk;
use crate::topo::chunk_ref::ChunkVoxelOutput;

use super::error::MesherError;
use super::greedy_mesh::ChunkSliceMask;
use super::greedy_mesh_material::GreedyMeshMaterial;
use super::mesh_builder::ChunkMeshAttributes;
use super::mesh_builder::Context;
use super::mesh_builder::Mesher;
use super::mesh_builder::MesherOutput;
use super::quad::MeshableQuad;
use super::quad::QuadTextureData;

#[derive(Clone)]
pub struct SimplePbrMesher {
    material: StandardMaterial,
}

impl SimplePbrMesher {
    pub fn new() -> Self {
        Self {
            material: StandardMaterial {
                base_color: Color::GRAY,
                ..default()
            },
        }
    }
}

// TODO: optimize the hell out of this little guy
impl Mesher for SimplePbrMesher {
    type Material = StandardMaterial;

    fn build<Acc>(
        &self,
        access: &Acc,
        cx: Context,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds,
    {
        // let mut positions = Vec::<[f32; 3]>::new();
        // let mut normals = Vec::<[f32; 3]>::new();
        // let mut uvs = Vec::<[f32; 2]>::new();

        // let mut indices = Vec::<u32>::new();
        // let mut current_idx: u32 = 0;

        // // for face in Face::FACES {
        // for x in 0..Chunk::SIZE {
        //     for y in 0..Chunk::SIZE {
        //         for z in 0..Chunk::SIZE {
        //             let pos = IVec3::new(x, y, z);
        //             let voxel_id = access.get(pos)?;

        //             if voxel_id.debug_transparency().is_transparent() {
        //                 continue;
        //             }

        //             for face in Face::FACES {
        //                 let adjacent_pos = face.offset_position(pos);
        //                 let adjacent_transparency = match access.get(adjacent_pos) {
        //                     Ok(adjacent_voxel_id) => adjacent_voxel_id.debug_transparency(),
        //                     Err(_) => {
        //                         let pos_in_adjacent_chunk = mask_pos_with_face(face, adjacent_pos);
        //                         cx.adjacency.sample(face, pos_in_adjacent_chunk).expect("We're only iterating through 0..16 so the position should be valid")
        //                     }
        //                 };

        //                 if adjacent_transparency.is_transparent() {
        //                     let pos_on_face = face.pos_on_face(pos);
        //                     let quad = Quad::from_points(
        //                         pos_on_face.as_vec2(),
        //                         (pos_on_face + IVec2::splat(1)).as_vec2(),
        //                     );

        //                     let vertex_positions = quad
        //                         .positions(face, face.axis().choose(pos.as_vec3()))
        //                         .map(|v| v.to_array());

        //                     positions.extend(vertex_positions.into_iter());
        //                     normals.extend([face.normal().as_vec3().to_array(); 4]);
        //                     uvs.extend([[0.0, 0.0]; 4]);

        //                     let face_indices = [0, 1, 2, 3, 2, 1].map(|idx| idx + current_idx);
        //                     if matches!(face, Face::Bottom | Face::East | Face::North) {
        //                         indices.extend(face_indices.into_iter().rev())
        //                     } else {
        //                         indices.extend(face_indices.into_iter())
        //                     }
        //                     current_idx += 4;
        //                 }
        //             }
        //         }
        //     }
        // }
        // // }

        // let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        // mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        // mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        // // mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        // mesh.set_indices(Some(Indices::U32(indices)));

        // Ok(MesherOutput { mesh })

        todo!()
    }

    fn material(&self) -> Self::Material {
        self.material.clone()
    }
}

#[derive(Clone)]
pub struct GreedyMesher {
    material: ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>,
    registries: Registries,
}

impl GreedyMesher {
    pub fn new(registries: Registries) -> Self {
        Self {
            material: ExtendedMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(registries.textures.atlas_texture()),
                    base_color: Color::rgb(0.5, 0.5, 0.65),

                    ..default()
                },
                extension: GreedyMeshMaterial {
                    texture_scale: registries.textures.texture_scale(),
                },
            },

            registries,
        }
    }

    fn calculate_slice_quads<A: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds>(
        &self,
        slice: &VoxelChunkSlice<A>,
        buffer: &mut Vec<MeshableQuad>,
    ) -> Result<(), MesherError<A::ReadErr>> {
        let mut mask = ChunkSliceMask::default();

        for k in 0..Chunk::SIZE {
            for j in 0..Chunk::SIZE {
                let pos = IVec2::new(k, j);
                if !slice.is_meshable(pos).unwrap() || mask.is_masked(pos).unwrap() {
                    continue;
                }

                let quad = Quad::from_points(pos.as_vec2(), pos.as_vec2());

                let mut quad_end = pos;

                let widened = quad.widen_until(1.0, Chunk::SIZE as u32, |n| {
                    let candidate_pos = ivec2(pos.x + n as i32, pos.y);
                    if !slice.is_meshable(candidate_pos).unwrap()
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
                        if !slice.is_meshable(candidate_pos).unwrap()
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

                buffer.push(MeshableQuad {
                    magnitude: slice.layer as _,
                    face: slice.face,
                    quad: heightened,
                    quad_tex: QuadTextureData {
                        pos: vec2(0.0, 0.0),
                        rotation: FaceTextureRotation::default(),
                    },
                })
            }
        }

        Ok(())
    }
}

impl Mesher for GreedyMesher {
    // TODO: greedy meshing mat
    type Material = ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>;

    fn build<Acc>(
        &self,
        access: &Acc,
        cx: Context,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds,
    {
        // we separate the meshing process into 3 sweeps across each of the 3D axes.
        // this lets us convert the 3D problem into a 2D one, by building planes
        // of geometry at a time instead of the whole cubic volume at once.

        // TODO: this is horribly inefficient when it comes to allocations, we should preserve a vec with a high capacity between meshing passes
        let mut quads = Vec::<MeshableQuad>::new();

        for face in [Face::Top, Face::Bottom] {
            for y in 0..Chunk::SIZE {
                let slice = VoxelChunkSlice::new(face, access, cx.adjacency, y);
                self.calculate_slice_quads(&slice, &mut quads)?;
            }
        }

        for face in [Face::North, Face::South] {
            for x in 0..Chunk::SIZE {
                let slice = VoxelChunkSlice::new(face, access, cx.adjacency, x);
                self.calculate_slice_quads(&slice, &mut quads)?;
            }
        }

        for face in [Face::East, Face::West] {
            for z in 0..Chunk::SIZE {
                let slice = VoxelChunkSlice::new(face, access, cx.adjacency, z);
                self.calculate_slice_quads(&slice, &mut quads)?;
            }
        }

        let mut attrs = ChunkMeshAttributes::default();
        let mut current_index = 0;

        for quad in quads.into_iter() {
            quad.add_to_mesh(current_index, &mut attrs);
            current_index += 4;
        }

        Ok(MesherOutput {
            mesh: attrs.to_mesh(),
        })
    }

    fn material(&self) -> Self::Material {
        self.material.clone()
    }
}
