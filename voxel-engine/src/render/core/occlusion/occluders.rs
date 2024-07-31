use std::any::type_name;

use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, SystemParamItem},
    },
    prelude::*,
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            Buffer, BufferInitDescriptor, BufferUsages, ShaderSize, StorageBuffer, VertexAttribute,
            VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
        },
        renderer::{RenderDevice, RenderQueue},
    },
};
use bytemuck::cast_slice;

use crate::{
    topo::world::{Chunk, ChunkPos},
    util::ChunkMap,
};

pub const OCCLUDER_BOX_SIZE: i32 = Chunk::SIZE;

pub fn occluder_vertex_buffer_layout(location: u32) -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: IVec3::SHADER_SIZE.into(),
        step_mode: VertexStepMode::Instance,
        attributes: vec![VertexAttribute {
            format: VertexFormat::Sint32x3,
            offset: 0,
            shader_location: location,
        }],
    }
}

#[derive(Resource, Default)]
pub struct OccluderBoxes {
    data: StorageBuffer<Vec<IVec3>>,
}

impl OccluderBoxes {
    pub fn len(&self) -> usize {
        self.data.get().len()
    }

    pub fn clear(&mut self) {
        self.data.get_mut().clear();
    }

    pub fn insert(&mut self, chunk: ChunkPos) {
        self.data.get_mut().push(chunk.as_ivec3());
    }

    pub fn buffer(&self) -> Option<&Buffer> {
        self.data.buffer()
    }

    pub fn queue(&mut self, gpu: &RenderDevice, queue: &RenderQueue) {
        self.data.write_buffer(gpu, queue);
    }
}

/// Extract occluder boxes
pub fn extract_occluders() {
    todo!()
}

pub fn prepare_occluders(
    mut occluders: ResMut<OccluderBoxes>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    occluders.queue(&gpu, &queue);
}

pub struct SetOccluderBuffer<const L: usize>;

impl<const L: usize, P: PhaseItem> RenderCommand<P> for SetOccluderBuffer<L> {
    type Param = SRes<OccluderBoxes>;

    type ViewQuery = Entity;

    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        view: ROQueryItem<'w, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let param = param.into_inner();

        let Some(occluders) = param.buffer() else {
            let view_entity = view;
            error!(
                "Failed to get occluder buffer. phase={} view={view_entity}",
                type_name::<P>()
            );
            return RenderCommandResult::Failure;
        };

        pass.set_vertex_buffer(L, occluders.slice(..));
        RenderCommandResult::Success
    }
}
