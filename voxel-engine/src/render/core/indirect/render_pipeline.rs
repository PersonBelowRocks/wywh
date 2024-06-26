use std::mem::size_of;

use bevy::{
    asset::{AssetServer, Handle},
    core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT,
    math::Vec3,
    pbr::{
        generate_view_layouts, MeshPipelineKey, MeshPipelineViewLayout, MeshPipelineViewLayoutKey,
        CLUSTERED_FORWARD_STORAGE_BUFFER_COUNT,
    },
    prelude::{Deref, FromWorld, Resource, World},
    render::{
        mesh::PrimitiveTopology,
        render_resource::{
            BindGroupLayout, BufferAddress, ColorTargetState, ColorWrites, CompareFunction,
            DepthBiasState, DepthStencilState, Face, FragmentState, FrontFace, MultisampleState,
            PolygonMode, PrimitiveState, PushConstantRange, RenderPipelineDescriptor, Shader,
            ShaderDefVal, ShaderSize, ShaderStages, SpecializedRenderPipeline, StencilState,
            TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
            VertexStepMode,
        },
        renderer::RenderDevice,
        texture::BevyDefault,
        view::{ViewTarget, VISIBILITY_RANGES_STORAGE_BUFFER_COUNT},
    },
};

use crate::render::core::{
    shaders::SHADER_PATHS,
    utils::{add_mesh_pipeline_shader_defs, add_shader_constants},
    DefaultBindGroupLayouts,
};

use super::ChunkInstanceData;

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
pub struct IndirectChunkRenderPipeline {
    pub view_layouts: [MeshPipelineViewLayout; MeshPipelineViewLayoutKey::COUNT],
    pub registry_layout: BindGroupLayout,
    pub indirect_chunk_bg_layout: BindGroupLayout,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

impl FromWorld for IndirectChunkRenderPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let gpu = world.resource::<RenderDevice>();

        let layouts = world.resource::<DefaultBindGroupLayouts>();

        let clustered_forward_buffer_binding_type =
            gpu.get_supported_read_only_binding_type(CLUSTERED_FORWARD_STORAGE_BUFFER_COUNT);

        let visibility_ranges_buffer_binding_type =
            gpu.get_supported_read_only_binding_type(VISIBILITY_RANGES_STORAGE_BUFFER_COUNT);

        Self {
            view_layouts: generate_view_layouts(
                gpu,
                clustered_forward_buffer_binding_type,
                visibility_ranges_buffer_binding_type,
            ),
            registry_layout: layouts.registry_bg_layout.clone(),
            indirect_chunk_bg_layout: layouts.icd_quad_bg_layout.clone(),
            vert: server.load(SHADER_PATHS.indirect_vert),
            frag: server.load(SHADER_PATHS.indirect_frag),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deref)]
pub struct IndirectChunkPipelineKey {
    pub inner: MeshPipelineKey,
}

impl SpecializedRenderPipeline for IndirectChunkRenderPipeline {
    type Key = IndirectChunkPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs: Vec<ShaderDefVal> = vec![
            "MESH_PIPELINE".into(),
            "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
        ];

        add_shader_constants(&mut shader_defs);
        add_mesh_pipeline_shader_defs(key.inner, &mut shader_defs);

        let mesh_view_layout = {
            let idx = MeshPipelineViewLayoutKey::from(key.inner).bits() as usize;
            self.view_layouts[idx].bind_group_layout.clone()
        };

        let bg_layouts = vec![
            mesh_view_layout,
            self.registry_layout.clone(),
            self.indirect_chunk_bg_layout.clone(),
        ];

        let target_format = if key.contains(MeshPipelineKey::HDR) {
            ViewTarget::TEXTURE_FORMAT_HDR
        } else {
            TextureFormat::bevy_default()
        };

        RenderPipelineDescriptor {
            label: Some("indirect_chunk_render_pipeline".into()),
            vertex: VertexState {
                shader: self.vert.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![chunk_indirect_instance_buffer_layout(0)],
            },
            fragment: Some(FragmentState {
                shader: self.frag.clone(),
                entry_point: "fragment".into(),
                shader_defs: shader_defs.clone(),
                targets: vec![Some(ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
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
