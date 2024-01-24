use bevy::{
    ecs::{
        component::Component,
        query::{QueryItem, ROQueryItem},
        system::{lifetimeless::Read, SystemParamItem},
    },
    render::{
        extract_component::ExtractComponent,
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::BindGroup,
    },
};

use crate::render::{
    occlusion::ChunkOcclusionMap,
    quad::{ChunkQuads, GpuQuad},
};

#[derive(Clone, Component)]
pub struct ExtractedChunkQuads {
    pub quads: Vec<GpuQuad>,
}

impl ExtractComponent for ExtractedChunkQuads {
    type Data = Read<ChunkQuads>;
    type Filter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, Self::Data>) -> Option<Self::Out> {
        Some(Self {
            quads: item.quads.clone(),
        })
    }
}

#[derive(Clone, Component)]
pub struct ExtractedChunkOcclusion {
    pub occlusion: Vec<u32>,
}

impl ExtractComponent for ExtractedChunkOcclusion {
    type Data = Read<ChunkOcclusionMap>;
    type Filter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, Self::Data>) -> Option<Self::Out> {
        // this entire operation should be very cheap, so its fine to do in extract
        let buffer = item
            .clone()
            .as_buffer()
            .into_iter()
            .map(u32::from_le_bytes)
            .collect::<Vec<_>>();

        Some(Self { occlusion: buffer })
    }
}

#[derive(Clone, Component)]
pub struct ChunkBindGroup {
    pub bind_group: BindGroup,
}

pub struct SetChunkBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetChunkBindGroup<I> {
    type Param = ();

    type ViewData = ();

    type ItemData = Read<ChunkBindGroup>;

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, Self::ViewData>,
        entity: ROQueryItem<'w, Self::ItemData>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(I, &entity.bind_group, &[]);
        RenderCommandResult::Success
    }
}
