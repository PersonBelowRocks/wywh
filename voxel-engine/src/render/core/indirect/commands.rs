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

use crate::render::core::observers::{ChunkBatch, RenderWorldObservers};
use crate::render::core::{
    gpu_chunk::IndirectRenderDataStore, gpu_registries::SetRegistryBindGroup,
};
use crate::topo::controller::ObserverId;

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
    type Param = (SRes<IndirectRenderDataStore>, SRes<RenderWorldObservers>);

    type ViewQuery = Read<ObserverId>;
    type ItemQuery = Read<ChunkBatch>;

    fn render<'w>(
        _item: &P,
        view: ROQueryItem<'w, Self::ViewQuery>,
        entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (store, observers) = (param.0.into_inner(), param.1.into_inner());
        let id = view;
        let Some(lod) = entity.map(|e| e.lod) else {
            error!("Cannot run this render command on an entity without a 'ChunkBatch' component.");
            return RenderCommandResult::Failure;
        };

        if !store.ready {
            error!("Indirect render data is not ready and cannot be rendered");
            return RenderCommandResult::Failure;
        }

        let Some(batches) = observers.get(id) else {
            error!("View entity {id:?} wasn't present in the render world observer store");
            return RenderCommandResult::Failure;
        };

        let Some(ref batch) = batches.get(lod) else {
            error!("Observer didn't have data for this chunk batch's LOD");
            return RenderCommandResult::Failure;
        };

        let Some(ref buffers) = batch.buffers else {
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
