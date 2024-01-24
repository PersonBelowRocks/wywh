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
use crate::render::quad::isometric::IsometrizedQuad;
use crate::render::quad::isometric::PositionedQuad;
use crate::topo::access::ChunkAccess;
use crate::topo::access::WriteAccess;
use crate::topo::chunk::Chunk;

use crate::render::error::MesherError;
use crate::render::mesh_builder::Context;
use crate::render::mesh_builder::Mesher;
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
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    // base_color: Color::rgb(0.5, 0.5, 0.65),
                    ..default()
                },
                extension: GreedyMeshMaterial {
                    texture_scale: texture_registry.texture_scale(),

                    faces: Vec::new(),
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
        buffer: &mut Vec<IsometrizedQuad>,
    ) -> Result<(), CqsError<A::ReadErr, Nb::ReadErr>>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        let mut mask = ChunkSliceMask::default();

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                let fpos = ivec2(x, y);
                if mask.is_masked(fpos).unwrap() {
                    continue;
                }

                let Some(dataquad) = cqs.get_quad(fpos)? else {
                    continue;
                };

                let mut current = PositionedQuad::new(fpos, dataquad);

                // widen
                let mut widen_by = 0;
                for dx in 1..(Chunk::SIZE - x) {
                    let candidate_pos = fpos + ivec2(dx, 0);

                    match cqs.get_quad(candidate_pos)? {
                        Some(merge_candidate) if merge_candidate == current.dataquad => {
                            widen_by = dx
                        }
                        _ => break,
                    }

                    let candidate_quad = cqs.get_quad(candidate_pos)?;
                    if matches!(candidate_quad, None)
                        || matches!(candidate_quad, Some(q) if q.texture != current.dataquad.texture)
                    {
                        break;
                    }
                }

                current.widen(widen_by).unwrap();

                // heighten
                let mut heighten_by = 0;
                'heighten: for dy in 1..(Chunk::SIZE - y) {
                    // sweep the width of the quad to test if all quads at this Y are the same
                    // if the sweep stumbles into a quad at this Y that doesn't equal the current quad, it
                    // will terminate the outer loop since we've heightened by as much as we can
                    for hx in (current.min().x)..=(current.max().x) {
                        let candidate_pos = ivec2(hx, dy + fpos.y);

                        let candidate_quad = cqs.get_quad(candidate_pos)?;
                        if matches!(candidate_quad, None)
                            || matches!(candidate_quad, Some(q) if q.texture != current.dataquad.texture)
                        {
                            break 'heighten;
                        }
                    }

                    // if we reach this line, the sweep loop was successful and all quads at this Y
                    // equaled the current quad, so we can heighten by at least this amount
                    heighten_by = dy;
                }

                current.heighten(heighten_by).unwrap();

                // mask_region will return false if any of the positions provided are outside of the
                // chunk bounds, so we do a little debug mode sanity check here to make sure thats
                // not the case, and catch the error early
                debug_assert!(mask.mask_region(current.min(), current.max()));
                let isoquad = cqs.isometrize(current);
                buffer.push(isoquad);
            }
        }

        Ok(())
    }
}

impl Mesher for GreedyMesher {
    type Material = ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>;

    fn build<A, Nb>(&self, access: A, cx: Context<Nb>) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        let varreg = self.registries.get_registry::<VariantRegistry>().unwrap();

        let mut cqs = ChunkQuadSlice::new(Face::North, 0, &access, &cx.neighbors, &varreg).unwrap();
        let mut buffer = Vec::<IsometrizedQuad>::new();

        for face in Face::FACES {
            for layer in 0..Chunk::SIZE {
                cqs.reposition(face, layer).unwrap();

                self.calculate_slice_quads(&cqs, &mut buffer)?;
            }
        }

        let occlusion = self.calculate_occlusion(&access, &cx.neighbors)?;

        todo!()
    }

    fn material(&self) -> Self::Material {
        self.material.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        data::{resourcepath::rpath, tile::Transparency},
        render::meshing::greedy::tests::{testing_registries, TestAccess},
        topo::{
            chunk_ref::ChunkVoxelOutput, neighbors::NeighborsBuilder,
            storage::data_structures::HashmapChunkStorage,
        },
    };

    use super::*;

    #[test]
    fn test_greedy_algo_cqs_empty() {
        let (varreg, texreg) = testing_registries();

        let void_cvo = ChunkVoxelOutput {
            transparency: Transparency::Transparent,
            variant: varreg.get_id(&rpath("void")).unwrap(),
            rotation: None,
        };

        let neighbors = NeighborsBuilder::<TestAccess>::new(void_cvo).build();

        let access = {
            let map = HashmapChunkStorage::<ChunkVoxelOutput>::new();

            TestAccess {
                default: void_cvo,
                map,
            }
        };

        let registries = Registries::new();
        registries.add_registry(varreg);
        registries.add_registry(texreg);

        let varreg = registries.get_registry::<VariantRegistry>().unwrap();
        let mesher = GreedyMesher::new(registries.clone());

        let cqs = ChunkQuadSlice::new(Face::North, 8, &access, &neighbors, &varreg).unwrap();

        let mut buffer = Vec::new();

        mesher.calculate_slice_quads(&cqs, &mut buffer).unwrap();

        assert!(buffer.is_empty());
    }

    #[test]
    fn test_greedy_algo_cqs_populated() {
        let (varreg, texreg) = testing_registries();

        let void_cvo = ChunkVoxelOutput {
            transparency: Transparency::Transparent,
            variant: varreg.get_id(&rpath("void")).unwrap(),
            rotation: None,
        };

        let var1_cvo = ChunkVoxelOutput {
            transparency: Transparency::Opaque,
            variant: varreg.get_id(&rpath("var1")).unwrap(),
            rotation: None,
        };

        let neighbors = NeighborsBuilder::<TestAccess>::new(void_cvo).build();

        let access = {
            let mut map = HashmapChunkStorage::<ChunkVoxelOutput>::new();

            map.set(ivec3(8, 8, 8), var1_cvo).unwrap();
            map.set(ivec3(8, 9, 8), var1_cvo).unwrap();
            map.set(ivec3(8, 9, 9), var1_cvo).unwrap();
            map.set(ivec3(8, 8, 9), var1_cvo).unwrap();

            // this is a lone block, we wanna test cases where no merging is required too
            map.set(ivec3(3, 3, 3), var1_cvo).unwrap();

            TestAccess {
                default: void_cvo,
                map,
            }
        };

        let registries = Registries::new();
        registries.add_registry(varreg);
        registries.add_registry(texreg);

        let varreg = registries.get_registry::<VariantRegistry>().unwrap();
        let mesher = GreedyMesher::new(registries.clone());

        let mut cqs = ChunkQuadSlice::new(Face::North, 8, &access, &neighbors, &varreg).unwrap();

        let mut buffer = Vec::new();

        mesher.calculate_slice_quads(&cqs, &mut buffer).unwrap();

        assert_eq!(1, buffer.len());

        let quad = buffer[0];

        assert_eq!(ivec3(8, 8, 8), quad.isometry.pos_3d());
        assert_eq!(ivec2(9, 9), quad.quad.max());
        assert_eq!(ivec2(8, 8), quad.quad.min());
        assert_eq!(ivec2(2, 2), quad.quad.dataquad.quad.dims());

        cqs.reposition(Face::North, 3).unwrap();

        mesher.calculate_slice_quads(&cqs, &mut buffer).unwrap();

        assert_eq!(2, buffer.len());

        let quad = buffer[1];

        assert_eq!(ivec3(3, 3, 3), quad.isometry.pos_3d());
        assert_eq!(ivec2(3, 3), quad.quad.max());
        assert_eq!(ivec2(3, 3), quad.quad.min());
        assert_eq!(ivec2(1, 1), quad.quad.dataquad.quad.dims());
    }
}
