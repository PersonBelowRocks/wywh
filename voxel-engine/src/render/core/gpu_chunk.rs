use std::{num::NonZeroU64, sync::Arc};

use bevy::{
    ecs::{
        entity::{Entity, EntityHashMap},
        query::{ROQueryItem, With},
        system::{
            lifetimeless::{self, Read, SRes},
            Commands, Local, Query, Res, ResMut, Resource, SystemParamItem,
        },
        world::{FromWorld, World},
    },
    prelude::Deref,
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            binding_types, BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
            Buffer, ShaderStages, StorageBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        view::Visibility,
        Extract,
    },
};
use itertools::Itertools;

use crate::{
    render::{
        meshing::controller::{
            ChunkMeshData, ChunkRenderPermits, ExtractableChunkMeshData, TimedChunkMeshData,
        },
        occlusion::ChunkOcclusionMap,
        quad::{ChunkQuads, GpuQuad},
    },
    topo::world::{ChunkEntity, ChunkPos},
    util::{ChunkMap, SyncChunkMap},
};

use super::render::ChunkPipeline;

pub fn extract_chunk_entities(
    mut cmds: Commands,
    chunks: Extract<Query<(Entity, &ChunkPos), With<ChunkEntity>>>,
) {
    let positions = chunks
        .iter()
        .map(|(entity, &pos)| (entity, pos))
        .collect_vec();

    cmds.insert_or_spawn_batch(
        positions
            .into_iter()
            .map(|(entity, pos)| (entity, (pos, ChunkEntity))),
    )
}

pub fn extract_chunk_mesh_data(
    mut render_meshes: ResMut<ChunkRenderDataStore>,
    main_world_meshes: Extract<Option<Res<ExtractableChunkMeshData>>>,
) {
    if let Some(meshes) = main_world_meshes.as_ref() {
        for pos in meshes.map.keys().into_iter() {
            if let Some(main_world_mesh) = meshes.map.remove(pos) {
                match render_meshes.map.get(pos) {
                    // Insert the mesh if it didn't exist before
                    None => {
                        render_meshes.map.set(
                            pos,
                            TimedChunkRenderData {
                                data: ChunkRenderData::Cpu(main_world_mesh.data),
                                generation: main_world_mesh.generation,
                            },
                        );
                    }
                    // Overwrite the existing mesh if the new mesh is of a later generation
                    Some(render_mesh) if main_world_mesh.generation > render_mesh.generation => {
                        render_meshes.map.set(
                            pos,
                            TimedChunkRenderData {
                                data: ChunkRenderData::Cpu(main_world_mesh.data),
                                generation: main_world_mesh.generation,
                            },
                        );
                    }

                    _ => (),
                }
            }
        }
    }
}

pub fn prepare_chunk_mesh_data(
    mut chunk_data_store: ResMut<ChunkRenderDataStore>,
    pipeline: Res<ChunkPipeline>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    todo!()
}

#[derive(Resource, Default)]
pub struct ChunkRenderDataStore {
    pub map: ChunkMap<TimedChunkRenderData>,
}

#[derive(Clone)]
pub enum ChunkRenderData {
    /// Raw chunk data in CPU memory, should be uploaded to GPU memory
    Cpu(ChunkMeshData),
    /// Handle to a bind group with the render data for this chunk
    Gpu(GpuChunkMeshData),
}

pub struct TimedChunkRenderData {
    pub data: ChunkRenderData,
    pub generation: u64,
}

impl ChunkRenderData {
    pub fn move_to_gpu(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        layout: &BindGroupLayout,
    ) -> bool {
        // let Self::Cpu(data) = self else {
        //     return false;
        // };

        // let quads = {
        //     // TODO: figure out a way to not do a clone here
        //     let mut buffer = StorageBuffer::from(data.quads.clone());
        //     buffer.set_label(Some("chunk_quad_buffer"));
        //     buffer.write_buffer(gpu, queue);
        //     buffer
        // };

        // let occlusion = {
        //     // TODO: figure out a way to not do a clone here
        //     let mut buffer = StorageBuffer::from(
        //         data.occlusion
        //             .clone()
        //             .as_buffer()
        //             .into_iter()
        //             .map(u32::from_le_bytes)
        //             .collect::<Vec<_>>(),
        //     );
        //     buffer.set_label(Some("chunk_occlusion_buffer"));
        //     buffer.write_buffer(gpu, queue);
        //     buffer
        // };

        // let bind_group = gpu.create_bind_group(
        //     Some("chunk_bind_group"),
        //     layout,
        //     &BindGroupEntries::sequential((quads.binding().unwrap(), occlusion.binding().unwrap())),
        // );

        // *self = Self::BindGroup(GpuChunkMeshData {
        //     bind_group,
        //     index_buffer: todo!(),
        //     position: todo!(),
        //     quad_buffer: quads.buffer().unwrap().clone(),
        // });

        // return true;

        todo!()
    }
}

#[derive(Clone)]
pub struct CpuChunkRenderData {
    pub quads: Vec<GpuQuad>,
    pub occlusion: ChunkOcclusionMap,
}

#[derive(Clone)]
pub struct GpuChunkMeshData {
    pub bind_group: BindGroup,
    pub index_buffer: Buffer,
    pub position: Buffer,
    pub quad_buffer: Buffer,
}

pub struct SetChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetChunkBindGroup<I> {
    type Param = SRes<ChunkRenderDataStore>;

    type ViewQuery = ();
    type ItemQuery = (Read<ChunkPos>, Read<ChunkEntity>);

    fn render<'w>(
        item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let store = param.into_inner();

        if let Some((&chunk_pos, _)) = entity {
            if let Some(ChunkRenderData::Gpu(data)) = store.map.get(chunk_pos).map(|d| &d.data) {
                pass.set_bind_group(I, &data.bind_group, &[]);
                RenderCommandResult::Success
            } else {
                RenderCommandResult::Failure
            }
        } else {
            RenderCommandResult::Failure
        }
    }
}
