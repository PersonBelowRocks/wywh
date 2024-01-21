use bevy::math::ivec2;
use bevy::math::ivec3;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::default;
use bevy::prelude::Color;

use bevy::prelude::StandardMaterial;

use crate::data::registries::texture::TextureRegistry;
use crate::data::registries::variant::VariantRegistry;
use crate::data::registries::Registries;
use crate::data::registries::Registry;
use crate::data::tile::Face;

use crate::render::error::MesherResult;
use crate::render::occlusion::ChunkOcclusionMap;
use crate::render::quad::isometric::PositionedQuad;
use crate::topo::access::ChunkAccess;
use crate::topo::access::WriteAccess;
use crate::topo::chunk::Chunk;

use crate::render::error::MesherError;
use crate::render::mesh_builder::Context;
use crate::render::mesh_builder::Mesher;
use crate::render::quad::MeshableQuad;
use crate::topo::ivec_project_to_3d;
use crate::topo::neighbors::Neighbors;

use super::error::CqsError;
use super::greedy_mesh::ChunkSliceMask;
use super::material::GreedyMeshMaterial;
use super::ChunkQuadSlice;

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

    fn build<A, Nb>(&self, _access: A, _cx: Context<Nb>) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
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

    fn calculate_occlusion<A, Nb>(
        &self,
        access: &A,
        neighbors: &Neighbors<Nb>,
    ) -> Result<ChunkOcclusionMap, MesherError<A::ReadErr, Nb::ReadErr>>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        let mut occlusion = ChunkOcclusionMap::new();
        let varreg = self.registries.get_registry::<VariantRegistry>().unwrap();

        // occlusion for the actual chunk
        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let ls_pos = ivec3(x, y, z);

                    let cvo = access
                        .get(ls_pos)
                        .map_err(|e| MesherError::AccessError(e))?;

                    let variant = varreg.get_by_id(cvo.variant);
                    if let Some(model) = variant.model {
                        let bo = model.occlusion(cvo.rotation);
                        occlusion.set(ls_pos, bo).map_err(MesherError::custom)?;
                    }
                }
            }
        }

        // occlusion for the neighbor chunks
        for face in Face::FACES {
            for x in -1..=Chunk::SIZE {
                for y in -1..=Chunk::SIZE {
                    let pos_on_face = ivec2(x, y);

                    let cvo = neighbors
                        .get(face, pos_on_face)
                        .map_err(MesherError::NeighborAccessError)?;

                    let variant = varreg.get_by_id(cvo.variant);
                    if let Some(model) = variant.model {
                        let ls_pos = {
                            let mut mag = face.axis_direction();
                            if mag > 0 {
                                mag = Chunk::SIZE;
                            }

                            ivec_project_to_3d(pos_on_face, face, mag)
                        };

                        let bo = model.occlusion(cvo.rotation);
                        occlusion.set(ls_pos, bo).map_err(MesherError::custom)?;
                    }
                }
            }
        }

        Ok(occlusion)
    }

    fn calculate_slice_quads<A, Nb>(
        &self,
        cqs: &ChunkQuadSlice<A, Nb>,
        buffer: &mut Vec<MeshableQuad>,
    ) -> Result<(), CqsError<A::ReadErr, Nb::ReadErr>>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        let mut mask = ChunkSliceMask::default();

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                let fpos = ivec2(x, y);
                let Some(dataquad) = cqs.get_quad(fpos)? else {
                    continue;
                };

                let mut current = PositionedQuad::new(fpos, dataquad);

                // widen
                let mut widen_by = 0;
                for dx in 1..(Chunk::SIZE - x) {
                    match cqs.get_quad(fpos + ivec2(dx, 0))? {
                        Some(merge_candidate) if merge_candidate == current.dataquad => {
                            widen_by = dx
                        }
                        _ => break,
                    }
                }

                current.widen(widen_by).unwrap();

                // heighten
                todo!();
            }
        }

        todo!()
    }
}

impl Mesher for GreedyMesher {
    // TODO: greedy meshing mat
    type Material = ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>;

    fn build<A, Nb>(&self, access: A, cx: Context<Nb>) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        todo!()
    }

    fn material(&self) -> Self::Material {
        self.material.clone()
    }
}
