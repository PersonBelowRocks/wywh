use std::mem::size_of;

use bevy::{
    core_pipeline::{
        core_3d::CORE_3D_DEPTH_FORMAT,
        prepass::{prepass_target_descriptors, PreviousViewData},
    },
    pbr::MeshPipelineKey,
    prelude::*,
    render::{
        globals::GlobalsUniform,
        mesh::PrimitiveTopology,
        render_resource::{
            binding_types::uniform_buffer, BindGroupLayout, BindGroupLayoutEntries, BufferAddress,
            CompareFunction, DepthBiasState, DepthStencilState, Face, FragmentState, FrontFace,
            MultisampleState, PolygonMode, PrimitiveState, PushConstantRange,
            RenderPipelineDescriptor, ShaderDefVal, ShaderSize, ShaderStages,
            SpecializedRenderPipeline, StencilState, VertexAttribute, VertexBufferLayout,
            VertexFormat, VertexState, VertexStepMode,
        },
        renderer::RenderDevice,
        view::ViewUniform,
    },
};

use crate::render::core::{utils::add_shader_constants, DefaultBindGroupLayouts};

use super::{
    indirect::ChunkInstanceData, shaders::DEFERRED_INDIRECT_CHUNK_HANDLE,
    utils::add_mesh_pipeline_shader_defs,
};

pub const INDIRECT_CHUNKS_PRIMITIVE_STATE: PrimitiveState = PrimitiveState {
    topology: PrimitiveTopology::TriangleList,
    strip_index_format: None,
    front_face: FrontFace::Ccw,
    cull_mode: Some(Face::Back),
    unclipped_depth: false,
    polygon_mode: PolygonMode::Fill,
    conservative: false,
};

pub fn chunk_indirect_instance_buffer_layout(start_at: u32) -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: ChunkInstanceData::SHADER_SIZE.into(),
        step_mode: VertexStepMode::Instance,
        attributes: vec![
            VertexAttribute {
                format: VertexFormat::Float32x3,
                shader_location: 0 + start_at,
                offset: 0,
            },
            VertexAttribute {
                format: VertexFormat::Uint32,
                shader_location: 1 + start_at,
                offset: size_of::<Vec3>() as BufferAddress,
            },
        ],
    }
}

/// The render pipeline for chunk multidraw
#[derive(Resource, Clone)]
pub struct DeferredIndirectChunkPipeline {
    pub view_layout_motion_vectors: BindGroupLayout,
    pub view_layout_no_motion_vectors: BindGroupLayout,
    pub registry_layout: BindGroupLayout,
    pub indirect_chunk_bg_layout: BindGroupLayout,
    pub shader: Handle<Shader>,
}

impl FromWorld for DeferredIndirectChunkPipeline {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        let layouts = world.resource::<DefaultBindGroupLayouts>();

        let view_layout_motion_vectors = gpu.create_bind_group_layout(
            "prepass_view_layout_motion_vectors",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    // View
                    uniform_buffer::<ViewUniform>(true),
                    // Globals
                    uniform_buffer::<GlobalsUniform>(false),
                    // PreviousViewUniforms
                    uniform_buffer::<PreviousViewData>(true),
                ),
            ),
        );

        let view_layout_no_motion_vectors: BindGroupLayout = gpu.create_bind_group_layout(
            "prepass_view_layout_no_motion_vectors",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    // View
                    uniform_buffer::<ViewUniform>(true),
                    // Globals
                    uniform_buffer::<GlobalsUniform>(false),
                ),
            ),
        );

        Self {
            view_layout_motion_vectors,
            view_layout_no_motion_vectors,
            registry_layout: layouts.registry_bg_layout.clone(),
            indirect_chunk_bg_layout: layouts.icd_quad_bg_layout.clone(),
            shader: DEFERRED_INDIRECT_CHUNK_HANDLE.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deref)]
pub struct ChunkPipelineKey {
    pub inner: MeshPipelineKey,
}

impl SpecializedRenderPipeline for DeferredIndirectChunkPipeline {
    type Key = ChunkPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs: Vec<ShaderDefVal> = vec![
            "MESH_PIPELINE".into(),
            "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
            "PREPASS_FRAGMENT".into(),
        ];

        add_shader_constants(&mut shader_defs);
        add_mesh_pipeline_shader_defs(key.inner, &mut shader_defs);

        let mesh_view_layout = if key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS) {
            self.view_layout_motion_vectors.clone()
        } else {
            self.view_layout_no_motion_vectors.clone()
        };

        let bg_layouts = vec![
            mesh_view_layout,
            self.registry_layout.clone(),
            self.indirect_chunk_bg_layout.clone(),
        ];

        let mut targets = prepass_target_descriptors(
            key.contains(MeshPipelineKey::NORMAL_PREPASS),
            key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS),
            key.contains(MeshPipelineKey::DEFERRED_PREPASS),
        );

        // TODO: is this needed for our custom pipeline?
        if targets.iter().all(Option::is_none) {
            // if no targets are required then clear the list, so that no fragment shader is required
            // (though one may still be used for discarding depth buffer writes)
            targets.clear();
        }

        RenderPipelineDescriptor {
            label: Some("indirect_chunk_render_pipeline".into()),
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: "chunk_vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![chunk_indirect_instance_buffer_layout(0)],
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                entry_point: "chunk_fragment".into(),
                shader_defs: shader_defs.clone(),
                targets,
            }),
            layout: bg_layouts,
            push_constant_ranges: vec![PushConstantRange {
                stages: ShaderStages::VERTEX,
                range: 0..4,
            }],
            primitive: INDIRECT_CHUNKS_PRIMITIVE_STATE,
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        }
    }
}
