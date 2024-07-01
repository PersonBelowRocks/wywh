use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::Entity;
use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, SystemParamItem},
    },
    log::error,
    pbr::{SetMeshViewBindGroup, SetPrepassViewBindGroup},
    render::{
        render_phase::{
            PhaseItem, RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
        },
        render_resource::IndexFormat,
    },
};

use crate::render::core::chunk_batches::PreparedChunkBatches;
use crate::render::core::observers::ObserverBatchBuffersStore;
use crate::render::core::{
    gpu_chunk::IndirectRenderDataStore, gpu_registries::SetRegistryBindGroup,
};
use crate::topo::controller::{ChunkBatch, ChunkBatchLod};

pub struct SetIndirectChunkQuads<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetIndirectChunkQuads<I> {
    type Param = SRes<IndirectRenderDataStore>;

    type ViewQuery = ();

    type ItemQuery = Read<ChunkBatchLod>;

    fn render<'w>(
        item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let store = param.into_inner();
        let batch_entity = item.entity();

        let Some(batch_lod) = entity else {
            error!("Couldn't get 'ChunkBatchLod' component for entity {batch_entity}");
            return RenderCommandResult::Failure;
        };

        let batch_lod = batch_lod.0;

        let lod_data = store.lod(batch_lod);
        if !lod_data.is_ready() {
            error!(
                "Indirect chunk data for LOD {:?} is not ready for rendering",
                batch_lod
            );
            return RenderCommandResult::Failure;
        }

        let bind_group = store
            .lod(batch_lod)
            .quad_bind_group()
            .expect("Bind group must be present if the LOD data is ready, which we just checked");

        pass.set_bind_group(I, bind_group, &[]);

        RenderCommandResult::Success
    }
}

pub struct IndirectChunkDraw;
impl<P: PhaseItem> RenderCommand<P> for IndirectChunkDraw {
    type Param = (
        SRes<IndirectRenderDataStore>,
        SRes<ObserverBatchBuffersStore>,
    );

    type ViewQuery = Entity;
    type ItemQuery = Read<ChunkBatchLod>;

    fn render<'w>(
        item: &P,
        view: ROQueryItem<'w, Self::ViewQuery>,
        entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (store, observer_batches) = (param.0.into_inner(), param.1.into_inner());

        let view_entity = view;
        let batch_entity = item.entity();

        let Some(batch_lod) = entity else {
            error!("Couldn't get 'ChunkBatchLod' component for entity {batch_entity}");
            return RenderCommandResult::Failure;
        };

        let batch_lod = batch_lod.0;

        let lod_data = store.lod(batch_lod);
        if !lod_data.is_ready() {
            let lod = batch_lod;
            error!("Indirect chunk data for LOD {lod:?} is not ready for rendering");
            return RenderCommandResult::Failure;
        }

        let Some(observer_batch) = observer_batches.get_batch_gpu_data(view_entity, batch_entity)
        else {
            error!("View {view_entity} did not have any data for batch {batch_entity}");
            return RenderCommandResult::Failure;
        };

        pass.set_index_buffer(lod_data.index_buffer().slice(..), 0, IndexFormat::Uint32);
        pass.set_vertex_buffer(0, lod_data.instance_buffer().slice(..));

        pass.multi_draw_indexed_indirect_count(
            &observer_batch.indirect,
            0,
            &observer_batch.count,
            0,
            observer_batch.num_chunks,
        );

        RenderCommandResult::Success
    }
}

pub type IndirectChunksRender = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetRegistryBindGroup<1>,
    SetIndirectChunkQuads<2>,
    IndirectChunkDraw,
);

pub type IndirectChunksPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetRegistryBindGroup<1>,
    SetIndirectChunkQuads<2>,
    IndirectChunkDraw,
);
