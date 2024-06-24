use bevy::{
    core_pipeline::{
        core_3d::CORE_3D_DEPTH_FORMAT,
        prepass::{PreviousViewData, MOTION_VECTOR_PREPASS_FORMAT, NORMAL_PREPASS_FORMAT},
    },
    pbr::MeshPipelineKey,
    prelude::*,
    render::{
        globals::GlobalsUniform,
        render_resource::{
            binding_types::uniform_buffer, BindGroupLayout, BindGroupLayoutEntries,
            ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
            FragmentState, MultisampleState, RenderPipelineDescriptor, ShaderDefVal, ShaderStages,
            SpecializedRenderPipeline, StencilState, VertexState,
        },
        renderer::RenderDevice,
        view::ViewUniform,
    },
};

use crate::render::core::{
    shaders::SHADER_PATHS, utils::add_shader_constants, DefaultBindGroupLayouts,
};

use super::{
    chunk_indirect_instance_buffer_layout, IndirectChunkPipelineKey,
    INDIRECT_CHUNKS_PRIMITIVE_STATE,
};

#[derive(Clone, Resource)]
pub struct IndirectChunkPrepassPipeline {
    pub view_layout_motion_vectors: BindGroupLayout,
    pub view_layout_no_motion_vectors: BindGroupLayout,
    pub layouts: DefaultBindGroupLayouts,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

impl FromWorld for IndirectChunkPrepassPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let gpu = world.resource::<RenderDevice>();

        let view_layout_motion_vectors = gpu.create_bind_group_layout(
            "chunk_prepass_view_layout_motion_vectors",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    // View
                    uniform_buffer::<ViewUniform>(true),
                    // Globals
                    uniform_buffer::<GlobalsUniform>(false),
                    // PreviousViewProjection
                    uniform_buffer::<PreviousViewData>(true),
                ),
            ),
        );

        let view_layout_no_motion_vectors = gpu.create_bind_group_layout(
            "chunk_prepass_view_layout_no_motion_vectors",
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
            layouts: world.resource::<DefaultBindGroupLayouts>().clone(),
            vert: server.load(SHADER_PATHS.indirect_prepass_vert),
            frag: server.load(SHADER_PATHS.indirect_prepass_frag),
        }
    }
}

// most of this code is taken verbatim from
// https://github.com/bevyengine/bevy/blob/d4132f661a8a567fd3f9c3b329c2b4032bb1e05e/crates/bevy_pbr/src/prepass/mod.rs#L297C1-L582C2
impl SpecializedRenderPipeline for IndirectChunkPrepassPipeline {
    type Key = IndirectChunkPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut bind_group_layouts =
            vec![if key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS) {
                self.view_layout_motion_vectors.clone()
            } else {
                self.view_layout_no_motion_vectors.clone()
            }];

        bind_group_layouts.extend_from_slice(&[
            self.layouts.registry_bg_layout.clone(),
            self.layouts.indirect_chunk_bg_layout.clone(),
        ]);

        let mut shader_defs: Vec<ShaderDefVal> = vec![
            "PREPASS_PIPELINE".into(),
            "VERTEX_UVS".into(),
            "VERTEX_NORMALS".into(),
            "VERTEX_TANGENTS".into(),
        ];

        add_shader_constants(&mut shader_defs);

        if key.contains(MeshPipelineKey::DEPTH_PREPASS) {
            shader_defs.push("DEPTH_PREPASS".into());
        }

        if key.contains(MeshPipelineKey::NORMAL_PREPASS) {
            shader_defs.push("NORMAL_PREPASS".into());
        }

        if key.intersects(MeshPipelineKey::NORMAL_PREPASS | MeshPipelineKey::DEFERRED_PREPASS) {
            shader_defs.push("NORMAL_PREPASS_OR_DEFERRED_PREPASS".into());
        }

        if key
            .intersects(MeshPipelineKey::MOTION_VECTOR_PREPASS | MeshPipelineKey::DEFERRED_PREPASS)
        {
            shader_defs.push("MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS".into());
        }

        if key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS) {
            shader_defs.push("MOTION_VECTOR_PREPASS".into());
        }

        if key.intersects(
            MeshPipelineKey::NORMAL_PREPASS
                | MeshPipelineKey::MOTION_VECTOR_PREPASS
                | MeshPipelineKey::DEFERRED_PREPASS,
        ) {
            shader_defs.push("PREPASS_FRAGMENT".into());
        }

        if key.contains(MeshPipelineKey::DEPTH_CLAMP_ORTHO) {
            shader_defs.push("DEPTH_CLAMP_ORTHO".into());
            // PERF: This line forces the "prepass fragment shader" to always run in
            // common scenarios like "directional light calculation". Doing so resolves
            // a pretty nasty depth clamping bug, but it also feels a bit excessive.
            // We should try to find a way to resolve this without forcing the fragment
            // shader to run.
            // https://github.com/bevyengine/bevy/pull/8877
            shader_defs.push("PREPASS_FRAGMENT".into());
        }

        let mut targets = vec![
            key.contains(MeshPipelineKey::NORMAL_PREPASS)
                .then_some(ColorTargetState {
                    format: NORMAL_PREPASS_FORMAT,
                    // BlendState::REPLACE is not needed here, and None will be potentially much faster in some cases.
                    blend: None,
                    write_mask: ColorWrites::ALL,
                }),
            key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS)
                .then_some(ColorTargetState {
                    format: MOTION_VECTOR_PREPASS_FORMAT,
                    // BlendState::REPLACE is not needed here, and None will be potentially much faster in some cases.
                    blend: None,
                    write_mask: ColorWrites::ALL,
                }),
            // these 2 render targets are normally for the deferred prepass, but we dont support
            // deferred rendering for chunks yet so we just leave these as None for now
            None,
            None,
        ];

        if targets.iter().all(Option::is_none) {
            // if no targets are required then clear the list, so that no fragment shader is required
            // (though one may still be used for discarding depth buffer writes)
            targets.clear();
        }

        let fragment_required = !targets.is_empty()
            || key.contains(MeshPipelineKey::DEPTH_CLAMP_ORTHO)
            || key.contains(MeshPipelineKey::MAY_DISCARD);

        let fragment = fragment_required.then(|| FragmentState {
            shader: self.frag.clone(),
            entry_point: "fragment".into(),
            shader_defs: shader_defs.clone(),
            targets,
        });

        RenderPipelineDescriptor {
            label: Some("indirect_chunk_prepass_pipeline".into()),
            layout: bind_group_layouts,
            push_constant_ranges: Vec::new(),
            vertex: VertexState {
                shader: self.vert.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![chunk_indirect_instance_buffer_layout(0)],
            },
            primitive: INDIRECT_CHUNKS_PRIMITIVE_STATE,
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment,
        }
    }
}
