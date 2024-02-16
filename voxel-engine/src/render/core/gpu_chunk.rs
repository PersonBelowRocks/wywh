use bevy::{
    ecs::{
        entity::Entity,
        query::{ROQueryItem, With},
        system::{lifetimeless::SRes, Local, Query, Res, ResMut, Resource, SystemParamItem},
        world::{FromWorld, World},
    },
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            binding_types, BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
            Buffer, ShaderStages, StorageBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        Extract,
    },
    utils::EntityHashMap,
};

use crate::{
    render::{
        meshing::ecs::ShouldExtract,
        occlusion::ChunkOcclusionMap,
        quad::{ChunkQuads, GpuQuad},
    },
    ChunkEntity,
};

use super::render::ChunkPipeline;

pub fn extract_chunk_render_data(
    mut last_count: Local<usize>,
    mut render_data: ResMut<ChunkRenderDataStore>,
    q_chunks: Extract<
        Query<(Entity, &ChunkQuads, &ChunkOcclusionMap, &ShouldExtract), With<ChunkEntity>>,
    >,
) {
    for (entity, quads, occlusion, should_extract) in q_chunks.iter() {
        if should_extract.get() {
            render_data.map.insert(
                entity,
                ChunkRenderData::Cpu(CpuChunkRenderData {
                    quads: quads.quads.clone(),
                    occlusion: occlusion.clone(),
                }),
            );

            // don't extract this entity again until it updates in the main world
            should_extract.set(false);
        }
    }

    if render_data.map.len() != *last_count {
        *last_count = render_data.map.len();
        // TODO: keep track of this somehow
    }
}

pub fn prepare_chunk_render_data(
    mut chunk_data_store: ResMut<ChunkRenderDataStore>,
    pipeline: Res<ChunkPipeline>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let mut count = 0;

    for data in chunk_data_store.map.values_mut() {
        if data.move_to_gpu(gpu.as_ref(), queue.as_ref(), &pipeline.chunk_layout) {
            count += 1;
        }
    }

    if count > 0 {
        // TODO: keep track of this
    }
}

#[derive(Resource)]
pub struct ChunkRenderDataStore {
    pub map: EntityHashMap<Entity, ChunkRenderData>,
    pub layout: BindGroupLayout,
}

impl FromWorld for ChunkRenderDataStore {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        let layout = gpu.create_bind_group_layout(
            Some("chunk_bind_group_layout"),
            &BindGroupLayoutEntries::with_indices(
                ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                (
                    (0, binding_types::storage_buffer_read_only::<GpuQuad>(false)),
                    (1, binding_types::storage_buffer_read_only::<u32>(false)),
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
    pub quad_buffer: Buffer,
    pub occlusion_buffer: Buffer,
}

pub struct SetChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetChunkBindGroup<I> {
    type Param = SRes<ChunkRenderDataStore>;

    type ViewData = ();

    type ItemData = ();

    fn render<'w>(
        item: &P,
        _view: ROQueryItem<'w, Self::ViewData>,
        _entity: ROQueryItem<'w, Self::ItemData>,
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
