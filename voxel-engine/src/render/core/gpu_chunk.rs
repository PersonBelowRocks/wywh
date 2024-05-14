use std::{mem, num::NonZeroU64, sync::Arc};

use bevy::{
    ecs::{
        entity::{Entity, EntityHashMap},
        query::{ROQueryItem, With},
        system::{
            lifetimeless::{self, Read, SRes},
            Commands, Local, Query, Res, ResMut, Resource, SystemParamItem,
        },
        world::{FromWorld, Mut, World},
    },
    log::debug,
    prelude::Deref,
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            binding_types, BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
            Buffer, BufferUsages, BufferVec, ShaderStages, StorageBuffer, UniformBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        view::Visibility,
        Extract, MainWorld,
    },
};
use hashbrown::hash_map::Entry;
use itertools::Itertools;

use crate::{
    render::{
        meshing::controller::{
            ChunkMeshData, ChunkMeshStatus, ChunkRenderPermits, ExtractableChunkMeshData,
            TimedChunkMeshData,
        },
        occlusion::ChunkOcclusionMap,
        quad::{ChunkQuads, GpuQuad},
    },
    topo::world::{ChunkEntity, ChunkPos},
    util::{ChunkMap, SyncChunkMap},
};

use super::{render::ChunkPipeline, DefaultBindGroupLayouts};

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
    mut main_world: ResMut<MainWorld>,
) {
    main_world.resource_scope(
        |world, mut extractable_meshes: Mut<ExtractableChunkMeshData>| {
            let mut extracted = 0;

            extractable_meshes
                .active
                .for_each_entry_mut(|pos, timed_mesh_data| {
                    // We only care about the filled chunk meshes here in the render world.
                    if matches!(timed_mesh_data.data, ChunkMeshStatus::Filled(_)) {
                        let ChunkMeshStatus::Filled(data) =
                            mem::replace(&mut timed_mesh_data.data, ChunkMeshStatus::Extracted)
                        else {
                            // We just checked that the ChunkMeshStatus enum matched above
                            unreachable!();
                        };

                        // Insert the chunk render data if it doesn't exist, and update it
                        // if this is a newer version
                        match render_meshes.map.entry(pos) {
                            Entry::Occupied(mut entry) => {
                                let tcrd = entry.get_mut();
                                if tcrd.generation < timed_mesh_data.generation {
                                    tcrd.generation = timed_mesh_data.generation;
                                    tcrd.data = ChunkRenderData::Cpu(data);

                                    extracted += 1;
                                }
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(TimedChunkRenderData {
                                    data: ChunkRenderData::Cpu(data),
                                    generation: timed_mesh_data.generation,
                                });

                                extracted += 1;
                            }
                        }
                    }
                });

            let mut removed = 0;

            // Remove meshes from the render world
            for &chunk_pos in &extractable_meshes.removed {
                render_meshes.map.remove(chunk_pos);
                removed += 1;
            }

            // Clear the removed mesh buffer
            extractable_meshes.removed.clear();

            if extracted > 0 {
                debug!("Extracted {} chunk meshes to render world", extracted);
            }

            if removed > 0 {
                debug!("Removed {} chunk meshes from render world", removed);
            }
        },
    );
}

pub fn prepare_chunk_mesh_data(
    mut chunk_data_store: ResMut<ChunkRenderDataStore>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    let mut total = 0;

    chunk_data_store.map.for_each_entry_mut(|pos, timed_data| {
        if matches!(timed_data.data, ChunkRenderData::Cpu(_)) {
            let ChunkRenderData::Cpu(ref data) = timed_data.data else {
                unreachable!();
            };

            let quads = {
                let mut buffer = StorageBuffer::from(data.quads.quads.clone());
                buffer.set_label(Some("chunk_quad_buffer"));
                buffer.write_buffer(gpu, queue);
                buffer
            };

            let indices = {
                let mut buffer =
                    BufferVec::<u32>::new(BufferUsages::COPY_DST | BufferUsages::INDEX);
                buffer.set_label(Some("chunk_index_buffer"));
                buffer.extend(data.index_buffer.iter().copied());
                buffer.write_buffer(gpu, queue);
                buffer
            };

            let position = {
                let mut buffer = UniformBuffer::from(pos.as_vec3());
                buffer.set_label(Some("chunk_position_buffer"));
                buffer.write_buffer(gpu, queue);
                buffer
            };

            let bind_group = gpu.create_bind_group(
                Some("chunk_bind_group"),
                &default_layouts.chunk_bg_layout,
                &BindGroupEntries::sequential((
                    position.binding().unwrap(),
                    quads.binding().unwrap(),
                )),
            );

            timed_data.data = ChunkRenderData::Gpu(GpuChunkMeshData {
                bind_group,
                position: position.buffer().unwrap().clone(),
                index_buffer: indices.buffer().unwrap().clone(),
                quad_buffer: quads.buffer().unwrap().clone(),
            });

            total += 1;
        }
    });

    if total > 0 {
        debug!("Uploaded {total} chunks to the GPU");
    }
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
