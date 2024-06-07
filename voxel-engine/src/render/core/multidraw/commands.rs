use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, SystemParamItem},
    },
    log::error,
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::IndexFormat,
    },
};

use crate::render::core::gpu_chunk::MultidrawRenderDataStore;

pub struct SetIndirectChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetIndirectChunkBindGroup<I> {
    type Param = SRes<MultidrawRenderDataStore>;

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
    type Param = SRes<MultidrawRenderDataStore>;

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

        if !store.chunks.is_ready() {
            error!("Multidraw render data is not ready and cannot be rendered");
            return RenderCommandResult::Failure;
        }

        let index_buffer = store.chunks.buffers().index.buffer();
        let instance_buffer = &store.chunks.buffers().instance;

        pass.set_index_buffer(index_buffer.slice(..), 0, IndexFormat::Uint32);
        pass.set_vertex_buffer(0, instance_buffer.slice(..));

        let indirect_buffer = &store.chunks.buffers().indirect;

        pass.multi_draw_indexed_indirect(indirect_buffer, 0, 1);

        RenderCommandResult::Success
    }
}
