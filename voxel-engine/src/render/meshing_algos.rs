use bevy::pbr::ExtendedMaterial;
use bevy::prelude::default;
use bevy::prelude::Color;
use bevy::prelude::Handle;
use bevy::prelude::IVec2;
use bevy::prelude::IVec3;
use bevy::prelude::Image;
use bevy::prelude::Mesh;
use bevy::prelude::StandardMaterial;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;

use crate::data::tile::Face;
use crate::data::tile::VoxelId;
use crate::render::adjacency::mask_pos_with_face;
use crate::render::greedy_mesh::VoxelChunkSlice;
use crate::render::quad::Quad;
use crate::topo::access::ChunkBounds;
use crate::topo::access::ReadAccess;
use crate::topo::chunk::Chunk;

use super::error::MesherError;
use super::greedy_mesh_material::GreedyMeshMaterial;
use super::mesh_builder::Context;
use super::mesh_builder::Mesher;
use super::mesh_builder::MesherOutput;

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

impl Mesher for SimplePbrMesher {
    type Material = StandardMaterial;

    fn build<Acc>(
        &self,
        access: &Acc,
        cx: Context,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = VoxelId> + ChunkBounds,
    {
        let mut positions = Vec::<[f32; 3]>::new();
        let mut normals = Vec::<[f32; 3]>::new();
        let mut uvs = Vec::<[f32; 2]>::new();

        let mut indices = Vec::<u32>::new();
        let mut current_idx: u32 = 0;

        // for face in Face::FACES {
        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let pos = IVec3::new(x, y, z);
                    let voxel_id = access.get(pos)?;

                    if voxel_id.debug_transparency().is_transparent() {
                        continue;
                    }

                    for face in Face::FACES {
                        let adjacent_pos = face.offset_position(pos);
                        let adjacent_transparency = match access.get(adjacent_pos) {
                            Ok(adjacent_voxel_id) => adjacent_voxel_id.debug_transparency(),
                            Err(_) => {
                                let pos_in_adjacent_chunk = mask_pos_with_face(face, adjacent_pos);
                                cx.adjacency.sample(face, pos_in_adjacent_chunk).expect("We're only iterating through 0..16 so the position should be valid")
                            }
                        };

                        if adjacent_transparency.is_transparent() {
                            let pos_on_face = face.pos_on_face(pos);
                            let quad = Quad::from_points(
                                pos_on_face.as_vec2(),
                                (pos_on_face + IVec2::splat(1)).as_vec2(),
                            );

                            let vertex_positions = quad
                                .positions(face, face.axis().choose(pos.as_vec3()))
                                .map(|v| v.to_array());

                            positions.extend(vertex_positions.into_iter());
                            normals.extend([face.normal().as_vec3().to_array(); 4]);
                            // TODO: texture system
                            uvs.extend([[0.0, 0.0]; 4]);

                            let face_indices = [0, 1, 2, 3, 2, 1].map(|idx| idx + current_idx);
                            if matches!(face, Face::Bottom | Face::East | Face::North) {
                                indices.extend(face_indices.into_iter().rev())
                            } else {
                                indices.extend(face_indices.into_iter())
                            }
                            current_idx += 4;
                        }
                    }
                }
            }
        }
        // }

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        // mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.set_indices(Some(Indices::U32(indices)));

        Ok(MesherOutput { mesh })
    }

    fn material(&self) -> Self::Material {
        self.material.clone()
    }
}

#[derive(Clone)]
pub struct GreedyMesher {
    material: ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>,
}

impl GreedyMesher {
    pub fn new(atlas_texture: Handle<Image>) -> Self {
        Self {
            material: ExtendedMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(atlas_texture),
                    ..default()
                },
                extension: GreedyMeshMaterial {},
            },
        }
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
        Acc: ReadAccess<ReadType = VoxelId> + ChunkBounds,
    {
        // due to greedy meshing (and meshing in general) being a somewhat complicated
        // process, this function is deliberately written very verbosely,
        // using only common operations and language features to keep it as
        // language agnostic as possible and avoid logic errors.

        // we separate the meshing process into 3 sweeps across each of the 3D axes.
        // this lets us convert the 3D problem into a 2D one, by building planes
        // of geometry at a time instead of the whole cubic volume at once.

        #[derive(Debug)]
        struct PositionedQuad {
            magnitude: f32,
            face: Face,
            quad: Quad,
        }

        let mut quads = Vec::<PositionedQuad>::new();

        // TODO: dont duplicate these 3 loops, its silly
        // X sweep
        for face in [Face::North, Face::South] {
            for x in 0..Chunk::SIZE {
                let mut slice = VoxelChunkSlice::new(face, access, cx.adjacency, x);
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        let pos = IVec2::new(y, z);
                        if !slice.is_meshable(pos).unwrap() {
                            continue;
                        }

                        let quad = Quad::from_points(
                            pos.as_vec2(),
                            pos.as_vec2(), //  + Vec2::splat(1.0),
                        );

                        let mut quad_end = pos;

                        // println!("widening");
                        let widened = quad.widen_until(1.0, Chunk::SIZE as u32, |n| {
                            // print!("{:?} ", (pos.x + n as i32, pos.y));
                            if !slice.is_meshable([pos.x + n as i32, pos.y].into()).unwrap() {
                                quad_end.x = (pos.x + n as i32) - 1;
                                true
                            } else {
                                false
                            }
                        });
                        // println!("");

                        // println!("heightening");
                        let heightened = widened.heighten_until(1.0, Chunk::SIZE as u32, |n| {
                            let mut abort = false;
                            for q_x in pos.x..=quad_end.x {
                                // print!("{:?} ", (q_x, pos.y + n as i32));
                                if !slice.is_meshable([q_x, pos.y + n as i32].into()).unwrap() {
                                    quad_end.y = (pos.y + n as i32) - 1;
                                    abort = true;
                                    break;
                                }
                            }
                            abort
                        });
                        // println!("");
                        // println!("---------");

                        slice.mask(pos, quad_end);

                        quads.push(PositionedQuad {
                            magnitude: x as _,
                            face,
                            quad: heightened, // widened.heighten(1.0),
                        })
                    }
                }
            }
        }

        // Y sweep
        for face in [Face::Top, Face::Bottom] {
            for y in 0..Chunk::SIZE {
                let mut slice = VoxelChunkSlice::new(face, access, cx.adjacency, y);
                for x in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        let pos = IVec2::new(x, z);
                        if !slice.is_meshable(pos).unwrap() {
                            continue;
                        }

                        let quad = Quad::from_points(
                            pos.as_vec2(),
                            pos.as_vec2(), //  + Vec2::splat(1.0),
                        );

                        let mut quad_end = pos;

                        // println!("widening");
                        let widened = quad.widen_until(1.0, Chunk::SIZE as u32, |n| {
                            // print!("{:?} ", (pos.x + n as i32, pos.y));
                            if !slice.is_meshable([pos.x + n as i32, pos.y].into()).unwrap() {
                                quad_end.x = (pos.x + n as i32) - 1;
                                true
                            } else {
                                false
                            }
                        });
                        // println!("");

                        // println!("heightening");
                        let heightened = widened.heighten_until(1.0, Chunk::SIZE as u32, |n| {
                            let mut abort = false;
                            for q_x in pos.x..=quad_end.x {
                                // print!("{:?} ", (q_x, pos.y + n as i32));
                                if !slice.is_meshable([q_x, pos.y + n as i32].into()).unwrap() {
                                    quad_end.y = (pos.y + n as i32) - 1;
                                    abort = true;
                                    break;
                                }
                            }
                            abort
                        });
                        // println!("");
                        // println!("---------");

                        slice.mask(pos, quad_end);

                        quads.push(PositionedQuad {
                            magnitude: y as _,
                            face,
                            quad: heightened, // widened.heighten(1.0),
                        })
                    }
                }
            }
        }

        // Z sweep
        for face in [Face::East, Face::West] {
            for z in 0..Chunk::SIZE {
                let mut slice = VoxelChunkSlice::new(face, access, cx.adjacency, z);
                for x in 0..Chunk::SIZE {
                    for y in 0..Chunk::SIZE {
                        let pos = IVec2::new(x, y);
                        if !slice.is_meshable(pos).unwrap() {
                            continue;
                        }

                        let quad = Quad::from_points(
                            pos.as_vec2(),
                            pos.as_vec2(), //  + Vec2::splat(1.0),
                        );

                        let mut quad_end = pos;

                        // println!("widening");
                        let widened = quad.widen_until(1.0, Chunk::SIZE as u32, |n| {
                            // print!("{:?} ", (pos.x + n as i32, pos.y));
                            if !slice.is_meshable([pos.x + n as i32, pos.y].into()).unwrap() {
                                quad_end.x = (pos.x + n as i32) - 1;
                                true
                            } else {
                                false
                            }
                        });
                        // println!("");

                        // println!("heightening");
                        let heightened = widened.heighten_until(1.0, Chunk::SIZE as u32, |n| {
                            let mut abort = false;
                            for q_x in pos.x..=quad_end.x {
                                // print!("{:?} ", (q_x, pos.y + n as i32));
                                if !slice.is_meshable([q_x, pos.y + n as i32].into()).unwrap() {
                                    quad_end.y = (pos.y + n as i32) - 1;
                                    abort = true;
                                    break;
                                }
                            }
                            abort
                        });
                        // println!("");
                        // println!("---------");

                        slice.mask(pos, quad_end);

                        quads.push(PositionedQuad {
                            magnitude: z as _,
                            face,
                            quad: heightened, // widened.heighten(1.0),
                        })
                    }
                }
            }
        }

        let mut positions = Vec::<[f32; 3]>::new();
        let mut normals = Vec::<[f32; 3]>::new();
        let mut uvs = Vec::<[f32; 2]>::new();
        let mut textures = Vec::<[f32; 2]>::new();

        let mut indices = Vec::<u32>::new();
        let mut current_idx: u32 = 0;

        for PositionedQuad {
            magnitude,
            face,
            quad,
        } in quads.into_iter()
        {
            let vertex_positions = quad.positions(face, magnitude).map(|v| v.to_array());
            positions.extend(vertex_positions.into_iter());
            normals.extend([face.normal().as_vec3().to_array(); 4]);
            uvs.extend([[0.0, 0.0]; 4]);
            textures.extend([[0.0, 0.0]; 4]);

            let face_indices = [0, 1, 2, 3, 2, 1].map(|idx| idx + current_idx);
            if matches!(face, Face::Bottom | Face::East | Face::North) {
                indices.extend(face_indices.into_iter().rev())
            } else {
                indices.extend(face_indices.into_iter())
            }
            current_idx += 4;
        }

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(GreedyMeshMaterial::TEXTURE_MESH_ATTR, textures);
        mesh.set_indices(Some(Indices::U32(indices)));

        Ok(MesherOutput { mesh })
    }

    fn material(&self) -> Self::Material {
        self.material.clone()
    }
}
