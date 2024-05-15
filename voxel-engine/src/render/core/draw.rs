use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{
            lifetimeless::{Read, SRes},
            SystemParamItem,
        },
    },
    log::{debug, debug_once},
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::IndexFormat,
    },
};

use crate::{
    render::core::gpu_chunk::ChunkRenderData,
    topo::world::{ChunkEntity, ChunkPos},
};

use super::gpu_chunk::ChunkRenderDataStore;

pub struct DrawChunk;

impl<P: PhaseItem> RenderCommand<P> for DrawChunk {
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

        let Some((&chunk_pos, _)) = entity else {
            return RenderCommandResult::Failure;
        };

        let Some(ChunkRenderData::Gpu(data)) = store.map.get(chunk_pos).map(|d| &d.data) else {
            return RenderCommandResult::Failure;
        };

        pass.set_index_buffer(data.index_buffer.slice(..), 0, IndexFormat::Uint32);
        pass.draw_indexed(0..data.index_count, 0, item.batch_range().clone());

        RenderCommandResult::Success
    }
}
