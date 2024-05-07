use std::{num::NonZeroU64, sync::Arc};

use bevy::{
    ecs::{
        entity::{Entity, EntityHashMap},
        query::{ROQueryItem, With},
        system::{
            lifetimeless::SRes, Commands, Local, Query, Res, ResMut, Resource, SystemParamItem,
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
        Extract,
    },
};

use crate::{
    render::{
        meshing::controller::{ChunkRenderPermits, TimedChunkMeshData},
        occlusion::ChunkOcclusionMap,
        quad::{ChunkQuads, GpuQuad},
    },
    topo::world::{ChunkEntity, ChunkPos},
    util::SyncChunkMap,
};

use super::render::ChunkPipeline;

#[derive(Resource, Default, Deref)]
pub struct ChunkMeshDataMap(Arc<SyncChunkMap<TimedChunkMeshData>>);

pub fn extract_chunk_mesh_data(
    mut cmds: Commands,
    permits: Extract<Option<Res<ChunkRenderPermits>>>,
) {
    if let Some(permits) = permits.as_ref() {
        cmds.insert_resource(ChunkMeshDataMap(permits.filled_permit_map().clone()))
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

#[derive(Resource)]
pub struct ChunkRenderDataStore {
    pub map: EntityHashMap<ChunkRenderData>,
    pub layout: BindGroupLayout,
}

impl FromWorld for ChunkRenderDataStore {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        let vec3f_size = NonZeroU64::new(4 * 3).unwrap();

        let layout = gpu.create_bind_group_layout(
            Some("chunk_bind_group_layout"),
            &BindGroupLayoutEntries::with_indices(
                ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                (
                    (
                        0,
                        binding_types::uniform_buffer_sized(false, Some(vec3f_size)),
                    ),
                    (1, binding_types::storage_buffer_read_only::<GpuQuad>(false)),
                    (2, binding_types::storage_buffer_read_only::<u32>(false)),
                ),
            ),
        );

        Self {
            map: EntityHashMap::default(),
            layout,
        }
    }
}

#[derive(Clone)]
pub enum ChunkRenderData {
    /// Raw chunk data in CPU memory, should be uploaded to GPU memory
    Cpu(CpuChunkRenderData),
    /// Handle to a bind group with the render data for this chunk
    BindGroup(ChunkBindGroup),
}

impl ChunkRenderData {
    pub fn move_to_gpu(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        layout: &BindGroupLayout,
    ) -> bool {
        let Self::Cpu(data) = self else {
            return false;
        };

        let quads = {
            // TODO: figure out a way to not do a clone here
            let mut buffer = StorageBuffer::from(data.quads.clone());
            buffer.set_label(Some("chunk_quad_buffer"));
            buffer.write_buffer(gpu, queue);
            buffer
        };

        let occlusion = {
            // TODO: figure out a way to not do a clone here
            let mut buffer = StorageBuffer::from(
                data.occlusion
                    .clone()
                    .as_buffer()
                    .into_iter()
                    .map(u32::from_le_bytes)
                    .collect::<Vec<_>>(),
            );
            buffer.set_label(Some("chunk_occlusion_buffer"));
            buffer.write_buffer(gpu, queue);
            buffer
        };

        let bind_group = gpu.create_bind_group(
            Some("chunk_bind_group"),
            layout,
            &BindGroupEntries::sequential((quads.binding().unwrap(), occlusion.binding().unwrap())),
        );

        *self = Self::BindGroup(ChunkBindGroup {
            bind_group,
            position: todo!(),
            quad_buffer: quads.buffer().unwrap().clone(),
            occlusion_buffer: quads.buffer().unwrap().clone(),
        });

        return true;
    }
}

#[derive(Clone)]
pub struct CpuChunkRenderData {
    pub quads: Vec<GpuQuad>,
    pub occlusion: ChunkOcclusionMap,
}

#[derive(Clone)]
pub struct ChunkBindGroup {
    pub bind_group: BindGroup,
    pub position: Buffer,
    pub quad_buffer: Buffer,
    pub occlusion_buffer: Buffer,
}

pub struct SetChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetChunkBindGroup<I> {
    type Param = SRes<ChunkRenderDataStore>;

    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let store = param.into_inner();

        match store.map.get(&item.entity()) {
            Some(ChunkRenderData::BindGroup(data)) => {
                pass.set_bind_group(I, &data.bind_group, &[]);
                RenderCommandResult::Success
            }
            _ => RenderCommandResult::Failure,
        }
    }
}
