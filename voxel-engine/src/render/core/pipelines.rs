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
            CachedComputePipelineId, CompareFunction, ComputePipelineDescriptor, DepthBiasState,
            DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PipelineCache,
            PolygonMode, PrimitiveState, PushConstantRange, RenderPipelineDescriptor, ShaderDefVal,
            ShaderSize, ShaderStages, SpecializedComputePipeline, SpecializedComputePipelines,
            SpecializedRenderPipeline, StencilState, VertexAttribute, VertexBufferLayout,
            VertexFormat, VertexState, VertexStepMode,
        },
        renderer::RenderDevice,
        view::ViewUniform,
    },
};

use crate::render::core::{utils::add_shader_constants, BindGroupProvider};

use super::{
    indirect::ChunkInstanceData,
    shaders::{
        DEFERRED_INDIRECT_CHUNK_HANDLE, PREPROCESS_BATCH_HANDLE, PREPROCESS_LIGHT_BATCH_HANDLE,
    },
    utils::{add_mesh_pipeline_shader_defs, u32_shader_def},
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

#[derive(Resource, Clone, Debug)]
pub struct ViewBatchPreprocessPipelineId(pub CachedComputePipelineId);

pub const PREPROCESS_BATCH_WORKGROUP_SIZE: u32 = 64;

/// Pipeline for preprocessing batches visible to a non-light view. Builds the indirect buffers
/// (count + args) and does frustum culling of chunks.
#[derive(Resource, Clone, Debug)]
pub struct ViewBatchPreprocessPipeline {
    pub shader: Handle<Shader>,
    pub mesh_metadata_layout: BindGroupLayout,
    pub view_layout: BindGroupLayout,
    pub batch_data_layout: BindGroupLayout,
}

impl FromWorld for ViewBatchPreprocessPipeline {
    fn from_world(world: &mut World) -> Self {
        let provider = world.resource::<BindGroupProvider>();

        Self {
            shader: PREPROCESS_BATCH_HANDLE,
            view_layout: provider.preprocess_view_bg_layout.clone(),
            mesh_metadata_layout: provider.preprocess_mesh_metadata_bg_layout.clone(),
            batch_data_layout: provider.preprocess_batch_data_bg_layout.clone(),
        }
    }
}

impl SpecializedComputePipeline for ViewBatchPreprocessPipeline {
    type Key = ();

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let mut shader_defs = vec![];
        add_shader_constants(&mut shader_defs);
        shader_defs.push(u32_shader_def(
            "WORKGROUP_SIZE",
            PREPROCESS_BATCH_WORKGROUP_SIZE,
        ));

        ComputePipelineDescriptor {
            label: Some("preprocess_batch_pipeline".into()),
            entry_point: "preprocess_batch".into(),
            shader: self.shader.clone(),
            shader_defs,
            layout: vec![
                self.mesh_metadata_layout.clone(),
                self.view_layout.clone(),
                self.batch_data_layout.clone(),
            ],
            push_constant_ranges: vec![],
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ViewBatchLightPreprocessPipelineId(pub CachedComputePipelineId);

/// Pipeline for preprocessing the batches visible by a light so that it can be rendered.
/// Builds the indirect buffers (count + args) and does a crude occlusion cull of chunks.
#[derive(Resource, Clone, Debug)]
pub struct ViewBatchLightPreprocessPipeline {
    shader: Handle<Shader>,
    pub mesh_metadata_layout: BindGroupLayout,
    pub light_view_layout: BindGroupLayout,
    pub batch_data_layout: BindGroupLayout,
}

impl FromWorld for ViewBatchLightPreprocessPipeline {
    fn from_world(world: &mut World) -> Self {
        let provider = world.resource::<BindGroupProvider>();

        Self {
            shader: PREPROCESS_LIGHT_BATCH_HANDLE,
            light_view_layout: provider.preprocess_light_view_bg_layout.clone(),
            mesh_metadata_layout: provider.preprocess_mesh_metadata_bg_layout.clone(),
            batch_data_layout: provider.preprocess_batch_data_bg_layout.clone(),
        }
    }
}

impl SpecializedComputePipeline for ViewBatchLightPreprocessPipeline {
    type Key = ();

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let mut shader_defs = vec![];
        add_shader_constants(&mut shader_defs);
        shader_defs.push(u32_shader_def(
            "WORKGROUP_SIZE",
            PREPROCESS_BATCH_WORKGROUP_SIZE,
        ));

        ComputePipelineDescriptor {
            label: Some("preprocess_light_batch_pipeline".into()),
            entry_point: "preprocess_light_batch".into(),
            shader: self.shader.clone(),
            shader_defs,
            layout: vec![
                self.mesh_metadata_layout.clone(),
                self.light_view_layout.clone(),
                self.batch_data_layout.clone(),
            ],
            push_constant_ranges: vec![],
        }
    }
}

pub fn create_pipelines(
    cache: Res<PipelineCache>,
    preprocess_pipeline: Res<ViewBatchPreprocessPipeline>,
    light_preprocess_pipeline: Res<ViewBatchLightPreprocessPipeline>,
    mut preprocess_pipelines: ResMut<SpecializedComputePipelines<ViewBatchPreprocessPipeline>>,
    mut light_preprocess_pipelines: ResMut<
        SpecializedComputePipelines<ViewBatchLightPreprocessPipeline>,
    >,
    mut cmds: Commands,
) {
    let id = preprocess_pipelines.specialize(&cache, &preprocess_pipeline, ());
    cmds.insert_resource(ViewBatchPreprocessPipelineId(id));
    let id = light_preprocess_pipelines.specialize(&cache, &light_preprocess_pipeline, ());
    cmds.insert_resource(ViewBatchLightPreprocessPipelineId(id));
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

        let layouts = world.resource::<BindGroupProvider>();

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

        let mut frag_required = true;
        // TODO: is this needed for our custom pipeline?
        if targets.iter().all(Option::is_none) {
            // if no targets are required then clear the list, so that no fragment shader is required
            // (though one may still be used for discarding depth buffer writes)
            targets.clear();
            frag_required = false;
        }

        RenderPipelineDescriptor {
            label: Some("indirect_chunk_render_pipeline".into()),
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: "chunk_vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![chunk_indirect_instance_buffer_layout(0)],
            },
            fragment: frag_required.then(|| FragmentState {
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
