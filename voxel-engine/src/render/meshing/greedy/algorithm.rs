use bevy::ecs::system::Resource;
use bevy::math::ivec2;
use bevy::math::ivec3;
use bevy::math::vec2;

use bevy::math::Vec2;
use bevy::math::Vec3;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::default;
use bevy::prelude::Color;

use bevy::prelude::StandardMaterial;
use bevy::render::mesh::Indices;
use bevy::render::mesh::Mesh;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;

use crate::data::registries::block::BlockVariantRegistry;
use crate::data::registries::Registries;
use crate::data::registries::Registry;
use crate::data::tile::Face;

use crate::render::core::RenderCore;
use crate::render::meshing::error::MesherError;
use crate::render::meshing::error::MesherResult;
use crate::render::meshing::Context;
use crate::render::meshing::Mesher;
use crate::render::meshing::MesherOutput;
use crate::render::occlusion::ChunkOcclusionMap;
use crate::render::quad::isometric::IsometrizedQuad;
use crate::render::quad::isometric::PositionedQuad;
use crate::render::quad::project_to_3d;
use crate::render::quad::ChunkQuads;
use crate::render::quad::GpuQuad;
use crate::render::quad::GpuQuadBitfields;
use crate::topo::access::ChunkAccess;
use crate::topo::access::WriteAccess;
use crate::topo::chunk::Chunk;

use crate::topo::ivec_project_to_3d;
use crate::topo::neighbors::Neighbors;

use super::error::CqsError;
use super::greedy_mesh::ChunkSliceMask;
use super::material::GreedyMeshMaterial;
use super::ChunkQuadSlice;

#[derive(Clone, Resource)]
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
    fn build<A, Nb>(&self, _access: A, _cx: Context<Nb>) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        todo!()
    }
}

#[derive(Clone, Resource)]
pub struct GreedyMesher {}

impl GreedyMesher {
    pub fn new() -> Self {
        Self {}
    }

    fn calculate_occlusion<A, Nb>(
        &self,
        access: &A,
        neighbors: &Neighbors<Nb>,
        registries: &Registries,
    ) -> Result<ChunkOcclusionMap, MesherError<A::ReadErr, Nb::ReadErr>>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        let mut occlusion = ChunkOcclusionMap::new();
        // let varreg = registries.get_registry::<BlockVariantRegistry>().unwrap();
        Ok(occlusion)

        // occlusion for the actual chunk
        // for x in 0..Chunk::SIZE {
        //     for y in 0..Chunk::SIZE {
        //         for z in 0..Chunk::SIZE {
        //             let ls_pos = ivec3(x, y, z);

        //             let cvo = access
        //                 .get(ls_pos)
        //                 .map_err(|e| MesherError::AccessError(e))?;

        //             let variant = varreg.get_by_id(cvo.variant);
        //             if let Some(model) = variant.model {
        //                 todo!()
        //             }
        //         }
        //     }
        // }

        // // occlusion for the neighbor chunks
        // for face in Face::FACES {
        //     for x in -1..=Chunk::SIZE {
        //         for y in -1..=Chunk::SIZE {
        //             let pos_on_face = ivec2(x, y);

        //             let cvo = neighbors
        //                 .get(face, pos_on_face)
        //                 .map_err(MesherError::NeighborAccessError)?;

        //             let variant = varreg.get_by_id(cvo.variant);
        //             if let Some(model) = variant.model {
        //                 let ls_pos = {
        //                     let mut mag = face.axis_direction();
        //                     if mag > 0 {
        //                         mag = Chunk::SIZE;
        //                     }

        //                     ivec_project_to_3d(pos_on_face, face, mag)
        //                 };

        //                 todo!()
        //             }
        //         }
        //     }
        // }

        // Ok(occlusion)
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
                debug_assert!(current.height() > 0);
                debug_assert!(current.width() > 0);

                // widen
                let mut widen_by = 0;
                for dx in 1..(Chunk::SIZE - x) {
                    let candidate_pos = fpos + ivec2(dx, 0);

                    if mask.is_masked(candidate_pos).unwrap() {
                        break;
                    }

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
                debug_assert!(current.width() > 0);

                // heighten
                let mut heighten_by = 0;
                'heighten: for dy in 1..(Chunk::SIZE - y) {
                    // sweep the width of the quad to test if all quads at this Y are the same
                    // if the sweep stumbles into a quad at this Y that doesn't equal the current quad, it
                    // will terminate the outer loop since we've heightened by as much as we can
                    for hx in (current.min().x)..=(current.max().x) {
                        let candidate_pos = ivec2(hx, dy + fpos.y);

                        if mask.is_masked(candidate_pos).unwrap() {
                            break 'heighten;
                        }

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
                debug_assert!(current.height() > 0);

                // mask_region will return false if any of the positions provided are outside of the
                // chunk bounds, so we do a little debug mode sanity check here to make sure thats
                // not the case, and catch the error early
                let result = mask.mask_region(current.min(), current.max());
                debug_assert!(result);

                let isoquad = cqs.isometrize(current);

                buffer.push(isoquad);
            }
        }

        Ok(())
    }
}

impl Mesher for GreedyMesher {
    fn build<A, Nb>(&self, access: A, cx: Context<Nb>) -> MesherResult<A::ReadErr, Nb::ReadErr>
    where
        A: ChunkAccess,
        Nb: ChunkAccess,
    {
        let varreg = cx
            .registries
            .get_registry::<BlockVariantRegistry>()
            .unwrap();

        let mut cqs = ChunkQuadSlice::new(Face::North, 0, &access, &cx.neighbors, &varreg).unwrap();
        let mut quads = Vec::<IsometrizedQuad>::new();

        for face in Face::FACES {
            for layer in 0..Chunk::SIZE {
                cqs.reposition(face, layer).unwrap();

                self.calculate_slice_quads(&cqs, &mut quads)?;
            }
        }

        let occlusion = self.calculate_occlusion(&access, &cx.neighbors, &cx.registries)?;

        let mut gpu_quads = Vec::<GpuQuad>::with_capacity(quads.len());
        for i in 0..quads.len() {
            let quad = quads[i];

            let bitfields = GpuQuadBitfields::new()
                .with_rotation(quad.quad.dataquad.texture.rotation)
                .with_face(quad.isometry.face);

            let magnitude = if quad.isometry.face.axis_direction() > 0 {
                quad.isometry.magnitude() + 1
            } else {
                quad.isometry.magnitude()
            };

            let gpu_quad = GpuQuad {
                min: quad.min_2d().as_vec2(),
                max: quad.max_2d().as_vec2() + Vec2::ONE,
                texture_id: quad.quad.dataquad.texture.id.as_u32(),
                bitfields,
                magnitude,
            };

            gpu_quads.push(gpu_quad);
        }

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());

        // The index buffer
        let mut vertex_indices = Vec::<u32>::with_capacity(gpu_quads.len() * 6);
        // Vertex attribute for what quad the vertex is a part of
        let mut quad_indices = Vec::<u32>::with_capacity(gpu_quads.len() * 6);
        let mut positions = Vec::<Vec3>::with_capacity(gpu_quads.len() * 6);

        let mut current_idx = 0;
        for (i, quad) in gpu_quads.iter().enumerate() {
            // 0---1
            // |   |
            // 2---3
            const VERTEX_INDICES: [u32; 6] = [0, 1, 2, 2, 1, 3];

            vertex_indices.extend_from_slice(&VERTEX_INDICES.map(|idx| idx + current_idx));
            quad_indices.extend_from_slice(&[i as u32; 4]);

            for vi in 0..4 {
                let pos_2d = match vi {
                    0 => vec2(quad.min.x, quad.max.y),
                    1 => vec2(quad.max.x, quad.max.y),
                    2 => vec2(quad.min.x, quad.min.y),
                    3 => vec2(quad.max.x, quad.min.y),
                    _ => unreachable!(),
                };

                let face = quad.bitfields.get_face();
                let layer = quad.magnitude as f32;

                positions.push(project_to_3d(pos_2d, face, layer));
            }

            current_idx += 4;
        }

        mesh.insert_indices(Indices::U32(vertex_indices));
        mesh.insert_attribute(RenderCore::QUAD_INDEX_ATTR, quad_indices);
        // mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

        Ok(MesherOutput {
            mesh,
            quads: ChunkQuads { quads: gpu_quads },
            occlusion,
        })
    }
}

#[cfg(test)]
mod tests {}
