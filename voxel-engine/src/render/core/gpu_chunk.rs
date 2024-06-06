use std::mem;

use bevy::{
    ecs::{
        entity::Entity,
        query::{ROQueryItem, With},
        system::{
            lifetimeless::{Read, SRes},
            Commands, Query, Res, ResMut, Resource, SystemParamItem,
        },
        world::Mut,
    },
    log::{debug, warn},
    prelude::{FromWorld, World},
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            BindGroup, BindGroupEntries, Buffer, BufferUsages, BufferVec, StorageBuffer,
            UniformBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        Extract, MainWorld,
    },
};
use hashbrown::hash_map::Entry;
use itertools::Itertools;

use crate::{
    render::{
        meshing::controller::{ChunkMeshData, ChunkMeshStatus, ExtractableChunkMeshData},
        occlusion::ChunkOcclusionMap,
        quad::GpuQuad,
    },
    topo::world::{ChunkEntity, ChunkPos},
    util::ChunkMap,
};

use super::{chunk_multidraw::ChunkMultidrawData, DefaultBindGroupLayouts};

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
        |_world, mut extractable_meshes: Mut<ExtractableChunkMeshData>| {
            let mut extracted = 0;

            extractable_meshes
                .active
                .for_each_entry_mut(|pos, new_mesh| {
                    // Skip unfulfilled and extracted chunks
                    if matches!(
                        new_mesh.data,
                        ChunkMeshStatus::Unfulfilled | ChunkMeshStatus::Extracted
                    ) {
                        return;
                    }

                    let status = mem::replace(&mut new_mesh.data, ChunkMeshStatus::Extracted);

                    match status {
                        // If the new chunk has an empty mesh, remove it from rendering
                        ChunkMeshStatus::Empty => {
                            let Some(existing) = render_meshes.map.get(pos) else {
                                return;
                            };

                            if existing.generation > new_mesh.generation {
                                return;
                            }

                            render_meshes.map.remove(pos);
                            new_mesh.data = ChunkMeshStatus::Extracted;
                        }
                        // Insert the chunk render data if it doesn't exist, and update it
                        // if this is a newer version
                        ChunkMeshStatus::Filled(data) => match render_meshes.map.entry(pos) {
                            Entry::Occupied(mut entry) => {
                                let tcrd = entry.get_mut();
                                if tcrd.generation > new_mesh.generation {
                                    return;
                                }
                                tcrd.generation = new_mesh.generation;
                                tcrd.data = ChunkRenderData::Cpu(data);

                                extracted += 1;
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(TimedChunkRenderData {
                                    data: ChunkRenderData::Cpu(data),
                                    generation: new_mesh.generation,
                                });

                                extracted += 1;
                            }
                        },
                        _ => unreachable!(),
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

            if data.quad_buffer.is_empty() || data.index_buffer.is_empty() {
                warn!("Tried to prepare render data for chunk at position {pos}, but it was missing data!");
                return;
            }

            let quads = {
                let mut buffer = StorageBuffer::from(data.quad_buffer.clone());
                buffer.set_label(Some("chunk_quad_buffer"));
                buffer.write_buffer(gpu, queue);
                buffer
            };

            let index_count = data.index_buffer.len() as u32;
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
                index_count,
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

#[derive(Resource)]
pub struct MultidrawRenderDataStore {
    pub chunks: ChunkMultidrawData,
}

impl FromWorld for MultidrawRenderDataStore {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        Self {
            chunks: ChunkMultidrawData::new(gpu),
        }
    }
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
    pub index_count: u32,
    pub position: Buffer,
    pub quad_buffer: Buffer,
}

pub struct SetChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetChunkBindGroup<I> {
    type Param = SRes<ChunkRenderDataStore>;

    type ViewQuery = ();
    type ItemQuery = (Read<ChunkPos>, Read<ChunkEntity>);

    fn render<'w>(
        _item: &P,
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
