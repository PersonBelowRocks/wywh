use bevy::math::ivec2;

use bevy::math::vec2;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::default;
use bevy::prelude::Color;

use bevy::prelude::IVec2;

use bevy::prelude::StandardMaterial;

use crate::data::registries::texture::TextureRegistry;
use crate::data::registries::variant::VariantRegistry;
use crate::data::registries::Registries;
use crate::data::tile::Face;

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
        _access: &Acc,
        _cx: Context,
    ) -> Result<MesherOutput, MesherError<Acc::ReadErr>>
    where
        Acc: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds,
    {
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
        let texture_registry = registries.get_registry::<TextureRegistry>().unwrap();

        Self {
            material: ExtendedMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(texture_registry.color_texture().clone()),
                    normal_map_texture: Some(texture_registry.normal_texture().clone()),
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    // base_color: Color::rgb(0.5, 0.5, 0.65),
                    ..default()
                },
                extension: GreedyMeshMaterial {
                    texture_scale: texture_registry.texture_scale(),

                    faces: texture_registry.face_texture_buffer(),
                },
            },

            registries: registries.clone(),
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

                let Some(tex) = slice.get_texture(pos)? else {
                    continue;
                };

                let quad = Quad::from_points(pos.as_vec2(), pos.as_vec2() + vec2(1.0, 1.0));

                let mut quad_end = pos;

                // let widened = quad.widen_until(1.0, Chunk::SIZE as u32, |n| {
                //     let candidate_pos = ivec2(pos.x + n as i32, pos.y);
                //     if !slice.is_meshable(candidate_pos).unwrap()
                //         || mask.is_masked(candidate_pos).unwrap()
                //         || slice.get_texture(candidate_pos).unwrap() != Some(tex)
                //     {
                //         quad_end.x = (pos.x + n as i32) - 1;
                //         true
                //     } else {
                //         false
                //     }
                // });

                // let heightened = widened.heighten_until(1.0, Chunk::SIZE as u32, |n| {
                //     let mut abort = false;
                //     for q_x in pos.x..=quad_end.x {
                //         let candidate_pos = ivec2(q_x, pos.y + n as i32);
                //         if !slice.is_meshable(candidate_pos).unwrap()
                //             || mask.is_masked(candidate_pos).unwrap()
                //             || slice.get_texture(candidate_pos).unwrap() != Some(tex)
                //         {
                //             quad_end.y = (pos.y + n as i32) - 1;
                //             abort = true;
                //             break;
                //         }
                //     }
                //     abort
                // });

                let heightened = quad;

                mask.mask_region(pos, quad_end);

                buffer.push(MeshableQuad {
                    magnitude: slice.layer as _,
                    face: slice.face,
                    quad: heightened,
                    quad_tex: QuadTextureData {
                        texture: tex.texture,
                        rotation: tex.rotation,
                        flip_uv_x: matches!(slice.face, Face::South | Face::East | Face::Bottom),
                        flip_uv_y: false,
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
        let variants = self.registries.get_registry::<VariantRegistry>().unwrap();

        for face in [Face::Top, Face::Bottom] {
            for y in 0..Chunk::SIZE {
                let slice = VoxelChunkSlice::new(face, y, access, cx.adjacency, &variants);
                self.calculate_slice_quads(&slice, &mut quads)?;
            }
        }

        for face in [Face::North, Face::South] {
            for x in 0..Chunk::SIZE {
                let slice = VoxelChunkSlice::new(face, x, access, cx.adjacency, &variants);
                self.calculate_slice_quads(&slice, &mut quads)?;
            }
        }

        for face in [Face::East, Face::West] {
            for z in 0..Chunk::SIZE {
                let slice = VoxelChunkSlice::new(face, z, access, cx.adjacency, &variants);
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
