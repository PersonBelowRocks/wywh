use bevy::{
    asset::{AssetId, AssetServer, Handle},
    core_pipeline::{
        core_3d::CORE_3D_DEPTH_FORMAT,
        prepass::{
            DepthPrepass, MotionVectorPrepass, NormalPrepass, Opaque3dPrepass,
            MOTION_VECTOR_PREPASS_FORMAT, NORMAL_PREPASS_FORMAT,
        },
    },
    ecs::{
        query::Has,
        system::{Query, Res, ResMut, Resource},
        world::{FromWorld, World},
    },
    pbr::{MeshPipelineKey, PreviousViewProjection, SetPrepassViewBindGroup},
    render::{
        globals::GlobalsUniform,
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, RenderPhase, SetItemPipeline},
        render_resource::{
            binding_types::uniform_buffer, BindGroupLayout, BindGroupLayoutEntries,
            ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
            Face, FragmentState, FrontFace, MultisampleState, PipelineCache, PolygonMode,
            PrimitiveState, RenderPipelineDescriptor, Shader, ShaderDefVal, ShaderStages,
            SpecializedMeshPipeline, SpecializedRenderPipeline, SpecializedRenderPipelines,
            StencilFaceState, StencilState, VertexState,
        },
        renderer::RenderDevice,
        view::{ExtractedView, ViewUniform, VisibleEntities},
    },
};

use crate::render::core::render::ChunkPipeline;

use super::{
    draw::DrawChunk,
    gpu_chunk::SetChunkBindGroup,
    gpu_registries::SetRegistryBindGroup,
    render::ChunkPipelineKey,
    utils::{add_shader_constants, iter_visible_chunks, ChunkDataParams},
    DefaultBindGroupLayouts,
};

#[derive(Clone, Resource)]
pub struct ChunkPrepassPipeline {
    pub view_layout_motion_vectors: BindGroupLayout,
    pub view_layout_no_motion_vectors: BindGroupLayout,
    pub layouts: DefaultBindGroupLayouts,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

impl FromWorld for ChunkPrepassPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let gpu = world.resource::<RenderDevice>();

        let _mesh_pipeline = world.resource::<ChunkPipeline>();

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
                    uniform_buffer::<PreviousViewProjection>(true),
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
            vert: server.load("shaders/vxl_chunk_vert_prepass.wgsl"),
            frag: server.load("shaders/vxl_chunk_frag_prepass.wgsl"),
        }
    }
}

// most of this code is taken verbatim from
// https://github.com/bevyengine/bevy/blob/d4132f661a8a567fd3f9c3b329c2b4032bb1e05e/crates/bevy_pbr/src/prepass/mod.rs#L297C1-L582C2
impl SpecializedRenderPipeline for ChunkPrepassPipeline {
    type Key = ChunkPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut bind_group_layouts =
            vec![if key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS) {
                self.view_layout_motion_vectors.clone()
            } else {
                self.view_layout_no_motion_vectors.clone()
            }];

        bind_group_layouts.extend_from_slice(&[
            self.layouts.registry_bg_layout.clone(),
            self.layouts.chunk_bg_layout.clone(),
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

        let fragment = fragment_required.then(|| {
            // Use the fragment shader from the material

            FragmentState {
                shader: self.frag.clone(),
                entry_point: "fragment".into(),
                shader_defs: shader_defs.clone(),
                targets,
            }
        });

        RenderPipelineDescriptor {
            label: Some("chunk_prepass_pipeline".into()),
            layout: bind_group_layouts,
            push_constant_ranges: Vec::new(),
            vertex: VertexState {
                shader: self.vert.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![],
            },
            primitive: PrimitiveState {
                topology: key.primitive_topology(),
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
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment,
        }
    }
}

pub fn queue_prepass_chunks(
    functions: Res<DrawFunctions<Opaque3dPrepass>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<ChunkPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    prepass_pipeline: Res<ChunkPrepassPipeline>,
    chunks: ChunkDataParams,
    mut views: Query<(
        &ExtractedView,
        &VisibleEntities,
        &mut RenderPhase<Opaque3dPrepass>,
        Has<DepthPrepass>,
        Has<NormalPrepass>,
        Has<MotionVectorPrepass>,
    )>,
) {
    let draw_function = functions.read().get_id::<DrawVoxelChunkPrepass>().unwrap();

    for (
        _view,
        visible_entities,
        mut phase,
        depth_prepass,
        normal_prepass,
        motion_vector_prepass,
    ) in &mut views
    {
        let mut view_key = MeshPipelineKey::empty();

        if depth_prepass {
            view_key |= MeshPipelineKey::DEPTH_PREPASS;
        }
        if normal_prepass {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }
        if motion_vector_prepass {
            view_key |= MeshPipelineKey::MOTION_VECTOR_PREPASS;
        }

        iter_visible_chunks(visible_entities, &chunks, |entity, _chunk_pos| {
            let pipeline_id = pipelines.specialize(
                &pipeline_cache,
                &prepass_pipeline,
                ChunkPipelineKey {
                    inner: MeshPipelineKey::from_primitive_topology(
                        PrimitiveTopology::TriangleList,
                    ) | view_key,
                },
            );
            phase.add(Opaque3dPrepass {
                entity: entity,
                draw_function: draw_function,
                pipeline_id,
                // this asset ID is seemingly just for some sorting stuff bevy does, but we have our own
                // logic so we don't care about what bevy would use this field for, so we set it to the default asset ID
                asset_id: AssetId::default(),
                batch_range: 0..1,
                dynamic_offset: None,
            });
        });
    }
}

pub type DrawVoxelChunkPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetRegistryBindGroup<1>,
    SetChunkBindGroup<2>,
    DrawChunk,
);
