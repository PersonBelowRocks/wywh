use bevy::ecs::system::lifetimeless::Read;
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

use crate::render::core::chunk_batches::RenderChunkBatches;
use crate::render::core::{
    gpu_chunk::IndirectRenderDataStore, gpu_registries::SetRegistryBindGroup,
};

pub struct SetIndirectChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetIndirectChunkBindGroup<I> {
    type Param = SRes<IndirectRenderDataStore>;

    type ViewQuery = ();

    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let store = param.into_inner();

        let Some(bind_group) = store.bind_group.as_ref() else {
            error!("Bind group hasn't been created for multidraw chunk quads");
            return RenderCommandResult::Failure;
        };

        pass.set_bind_group(I, bind_group, &[]);

        RenderCommandResult::Success
    }
}

pub struct IndirectChunkDraw;
impl<P: PhaseItem> RenderCommand<P> for IndirectChunkDraw {
    type Param = (SRes<IndirectRenderDataStore>, SRes<RenderChunkBatches>);

    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (store, batches) = (param.0.into_inner(), param.1.into_inner());

        if !store.ready {
            error!("Indirect render data is not ready and cannot be rendered");
            return RenderCommandResult::Failure;
        }

        let Some(batch) = batches.get(item.entity()) else {
            error!("Batch entity wasn't present in the global render batch store");
            return RenderCommandResult::Failure;
        };

        let Some(ref buffers) = batch.gpu_data else {
            error!("Chunk batch didn't have initialized buffers");
            return RenderCommandResult::Failure;
        };

        let index_buffer = store.chunks.buffers().index.buffer();

        pass.set_index_buffer(index_buffer.slice(..), 0, IndexFormat::Uint32);
        pass.set_vertex_buffer(0, buffers.instance.slice(..));

        pass.multi_draw_indexed_indirect_count(
            &buffers.indirect,
            0,
            &buffers.count,
            0,
            store.chunks.num_chunks() as _,
        );

        RenderCommandResult::Success
    }
}

pub type IndirectChunksRender = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetRegistryBindGroup<1>,
    SetIndirectChunkBindGroup<2>,
    IndirectChunkDraw,
);

pub type IndirectChunksPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetRegistryBindGroup<1>,
    SetIndirectChunkBindGroup<2>,
    IndirectChunkDraw,
);
