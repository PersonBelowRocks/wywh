use bevy::{
    asset::{AssetServer, Handle},
    core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT,
    pbr::{
        generate_view_layouts, MeshPipelineKey, MeshPipelineViewLayout, MeshPipelineViewLayoutKey,
        CLUSTERED_FORWARD_STORAGE_BUFFER_COUNT,
    },
    prelude::{Deref, FromWorld, Resource, World},
    render::{
        mesh::PrimitiveTopology,
        render_resource::{
            BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
            DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PolygonMode,
            PrimitiveState, PushConstantRange, RenderPipelineDescriptor, Shader, ShaderDefVal,
            ShaderSize, ShaderStages, SpecializedRenderPipeline, StencilFaceState, StencilState,
            TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
            VertexStepMode,
        },
        renderer::RenderDevice,
        texture::BevyDefault,
        view::ViewTarget,
    },
};

use crate::render::core::{
    utils::{add_mesh_pipeline_shader_defs, add_shader_constants},
    DefaultBindGroupLayouts,
};

use super::ChunkInstanceData;

#[derive(Resource, Clone)]
pub struct MultidrawChunkPipeline {
    pub view_layouts: [MeshPipelineViewLayout; MeshPipelineViewLayoutKey::COUNT],
    pub registry_layout: BindGroupLayout,
    pub chunk_layout: BindGroupLayout,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

impl FromWorld for MultidrawChunkPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let gpu = world.resource::<RenderDevice>();

        let layouts = world.resource::<DefaultBindGroupLayouts>();

        let clustered_forward_buffer_binding_type =
            gpu.get_supported_read_only_binding_type(CLUSTERED_FORWARD_STORAGE_BUFFER_COUNT);

        todo!("need to write the shaders");

        Self {
            view_layouts: generate_view_layouts(gpu, clustered_forward_buffer_binding_type),
            registry_layout: layouts.registry_bg_layout.clone(),
            chunk_layout: layouts.chunk_bg_layout.clone(),
            vert: server.load("TODO"),
            frag: server.load("TODO"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deref)]
pub struct MultidrawChunkPipelineKey {
    pub inner: MeshPipelineKey,
}

impl SpecializedRenderPipeline for MultidrawChunkPipeline {
    type Key = MultidrawChunkPipelineKey;

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
            self.chunk_layout.clone(),
        ];

        let target_format = if key.contains(MeshPipelineKey::HDR) {
            ViewTarget::TEXTURE_FORMAT_HDR
        } else {
            TextureFormat::bevy_default()
        };

        RenderPipelineDescriptor {
            label: Some("multidraw_chunk_render_pipeline".into()),
            vertex: VertexState {
                shader: self.vert.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![VertexBufferLayout {
                    array_stride: ChunkInstanceData::SHADER_SIZE.into(),
                    step_mode: VertexStepMode::Instance,
                    attributes: vec![todo!()],
                }],
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
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        }
    }
}
