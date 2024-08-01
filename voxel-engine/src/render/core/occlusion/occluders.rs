use std::any::type_name;

use bevy::{
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, SystemParamItem},
    },
    math::vec3,
    pbr::MeshViewBindGroup,
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            BindGroupDescriptor, BindGroupLayout, Buffer, BufferInitDescriptor, BufferUsages,
            CachedRenderPipelineId, CompareFunction, DepthStencilState, Face, FrontFace,
            MultisampleState, PipelineCache, PrimitiveState, RenderPipelineDescriptor, ShaderSize,
            StencilState, StorageBuffer, TextureFormat, VertexAttribute, VertexBufferLayout,
            VertexFormat, VertexState, VertexStepMode,
        },
        renderer::{RenderDevice, RenderQueue},
    },
};
use bytemuck::cast_slice;

use crate::{
    render::core::{shaders::OCCLUDER_DEPTH_HANDLE, BindGroupProvider},
    topo::world::{Chunk, ChunkPos},
    util::ChunkMap,
};

pub const OCCLUDER_BOX_SIZE: f32 = Chunk::SIZE as f32;

// Box model is from https://gist.github.com/MaikKlein/0b6d6bb58772c13593d0a0add6004c1c
#[rustfmt::skip]
pub static OCCLUDER_BOX_VERTICES: &'static [Vec3] = &[
    vec3(1.0, 0.0, 0.0),
    vec3(1.0, 0.0, 1.0),
    vec3(0.0, 0.0, 1.0),
    vec3(0.0, 0.0, 0.0),
    vec3(1.0, 1.0, 0.0),
    vec3(1.0, 1.0, 1.0),
    vec3(0.0, 1.0, 1.0),
    vec3(0.0, 1.0, 0.0)
];

#[rustfmt::skip]
pub static OCCLUDER_BOX_INDICES: &'static [u32] = &[
    1, 2, 3,
    7, 6, 5,
    4, 5, 1,
    5, 6, 2,
    2, 6, 7,
    0, 3, 7,
    0, 1, 3,
    4, 7, 5,
    0, 4, 1,
    1, 5, 2,
    3, 2, 7,
    4, 0, 7,
];

pub fn occluder_instance_buffer_layout(location: u32) -> VertexBufferLayout {
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

pub fn occluder_vertex_buffer_layout(location: u32) -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: Vec3::SHADER_SIZE.into(),
        step_mode: VertexStepMode::Vertex,
        attributes: vec![VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: location,
        }],
    }
}

fn scaled_occluder_vertex_positions() -> Vec<Vec3> {
    OCCLUDER_BOX_VERTICES
        .iter()
        .map(|&pos| pos * OCCLUDER_BOX_SIZE)
        .collect()
}

#[derive(Resource)]
pub struct OccluderModel {
    pub index_buffer: Buffer,
    pub vertex_buffer: Buffer,
}

impl FromWorld for OccluderModel {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();
        let vertices = scaled_occluder_vertex_positions();

        Self {
            index_buffer: gpu.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("occluder_box_indices"),
                contents: cast_slice(OCCLUDER_BOX_INDICES),
                usage: BufferUsages::INDEX,
            }),
            vertex_buffer: gpu.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("occluder_box_vertices"),
                contents: cast_slice(&vertices),
                usage: BufferUsages::VERTEX,
            }),
        }
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

#[derive(Resource)]
pub struct OccluderDepthPipeline {
    pub pipeline_id: CachedRenderPipelineId,
    pub layout: BindGroupLayout,
}

impl FromWorld for OccluderDepthPipeline {
    fn from_world(world: &mut World) -> Self {
        let pipeline_cache = world.resource::<PipelineCache>();
        let bg_provider = world.resource::<BindGroupProvider>();

        let descriptor = RenderPipelineDescriptor {
            label: Some("occluder_depth_pipeline".into()),
            layout: vec![bg_provider.prepass_view_no_mv_bg_layout.clone()],
            push_constant_ranges: vec![],
            fragment: None,
            multisample: default(),
            vertex: VertexState {
                shader: OCCLUDER_DEPTH_HANDLE,
                shader_defs: vec![],
                entry_point: "occluder_depth_vertex".into(),
                buffers: vec![
                    occluder_instance_buffer_layout(0),
                    occluder_vertex_buffer_layout(1),
                ],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                cull_mode: Some(Face::Back),
                front_face: FrontFace::Ccw,

                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Greater,
                stencil: default(),
                bias: default(),
            }),
        };

        let pipeline_id = pipeline_cache.queue_render_pipeline(descriptor);

        Self {
            pipeline_id,
            layout: bg_provider.prepass_view_no_mv_bg_layout.clone(),
        }
    }
}
