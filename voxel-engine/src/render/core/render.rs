use bevy::{
    asset::{AssetId, AssetServer, Handle},
    core_pipeline::{
        core_3d::{Opaque3d, CORE_3D_DEPTH_FORMAT},
        prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
    },
    ecs::{
        query::Has,
        system::{Query, Res, ResMut, Resource},
        world::{FromWorld, World},
    },
    log::debug,
    pbr::{
        generate_view_layouts, MeshPipelineKey, MeshPipelineViewLayout, MeshPipelineViewLayoutKey,
        ScreenSpaceAmbientOcclusionSettings, SetMeshViewBindGroup, ShadowFilteringMethod,
        CLUSTERED_FORWARD_STORAGE_BUFFER_COUNT,
    },
    prelude::Deref,
    render::{
        camera::{Projection, TemporalJitter},
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, RenderPhase, SetItemPipeline},
        render_resource::{
            BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
            DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PipelineCache,
            PolygonMode, PrimitiveState, PushConstantRange, RenderPipelineDescriptor, Shader,
            ShaderDefVal, ShaderStages, SpecializedMeshPipeline, SpecializedRenderPipeline,
            SpecializedRenderPipelines, StencilFaceState, StencilState, TextureFormat, VertexState,
        },
        renderer::RenderDevice,
        texture::BevyDefault,
        view::{ExtractedView, ViewTarget, VisibleEntities},
    },
};

use crate::render::core::utils::add_mesh_pipeline_shader_defs;

use super::{
    draw::DrawChunk,
    gpu_chunk::SetChunkBindGroup,
    gpu_registries::SetRegistryBindGroup,
    utils::{add_shader_constants, iter_visible_chunks, ChunkDataParams},
    DefaultBindGroupLayouts,
};

#[derive(Resource, Clone)]
pub struct ChunkPipeline {
    pub mesh_pipeline_view_layouts: [MeshPipelineViewLayout; MeshPipelineViewLayoutKey::COUNT],
    pub registry_layout: BindGroupLayout,
    pub chunk_layout: BindGroupLayout,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deref)]
pub struct ChunkPipelineKey {
    pub inner: MeshPipelineKey,
}

impl FromWorld for ChunkPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let gpu = world.resource::<RenderDevice>();

        let layouts = world.resource::<DefaultBindGroupLayouts>();

        let clustered_forward_buffer_binding_type =
            gpu.get_supported_read_only_binding_type(CLUSTERED_FORWARD_STORAGE_BUFFER_COUNT);

        Self {
            mesh_pipeline_view_layouts: generate_view_layouts(
                gpu,
                clustered_forward_buffer_binding_type,
            ),
            registry_layout: layouts.registry_bg_layout.clone(),
            chunk_layout: layouts.chunk_bg_layout.clone(),
            vert: server.load("shaders/vxl_chunk_vert.wgsl"),
            frag: server.load("shaders/vxl_chunk_frag.wgsl"),
        }
    }
}

impl SpecializedRenderPipeline for ChunkPipeline {
    type Key = ChunkPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs: Vec<ShaderDefVal> = vec![
            "MESH_PIPELINE".into(),
            "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
        ];

        add_shader_constants(&mut shader_defs);
        add_mesh_pipeline_shader_defs(key.inner, &mut shader_defs);

        let mesh_view_layout = {
            let idx = MeshPipelineViewLayoutKey::from(key.inner).bits() as usize;
            self.mesh_pipeline_view_layouts[idx]
                .bind_group_layout
                .clone()
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
            label: Some("chunk_render_pipeline".into()),
            vertex: VertexState {
                shader: self.vert.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![],
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

pub const fn tonemapping_pipeline_key(tonemapping: Tonemapping) -> MeshPipelineKey {
    match tonemapping {
        Tonemapping::None => MeshPipelineKey::TONEMAP_METHOD_NONE,
        Tonemapping::Reinhard => MeshPipelineKey::TONEMAP_METHOD_REINHARD,
        Tonemapping::ReinhardLuminance => MeshPipelineKey::TONEMAP_METHOD_REINHARD_LUMINANCE,
        Tonemapping::AcesFitted => MeshPipelineKey::TONEMAP_METHOD_ACES_FITTED,
        Tonemapping::AgX => MeshPipelineKey::TONEMAP_METHOD_AGX,
        Tonemapping::SomewhatBoringDisplayTransform => {
            MeshPipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM
        }
        Tonemapping::TonyMcMapface => MeshPipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE,
        Tonemapping::BlenderFilmic => MeshPipelineKey::TONEMAP_METHOD_BLENDER_FILMIC,
    }
}

pub fn queue_chunks(
    functions: Res<DrawFunctions<Opaque3d>>,
    pipeline: Res<ChunkPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<ChunkPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    chunks: ChunkDataParams,
    mut views: Query<(
        &ExtractedView,
        &VisibleEntities,
        &mut RenderPhase<Opaque3d>,
        Option<&Tonemapping>,
        Option<&DebandDither>,
        Option<&ShadowFilteringMethod>,
        Option<&Projection>,
        (
            Has<NormalPrepass>,
            Has<DepthPrepass>,
            Has<MotionVectorPrepass>,
            Has<DeferredPrepass>,
        ),
        Has<ScreenSpaceAmbientOcclusionSettings>,
        Has<TemporalJitter>,
    )>,
) {
    let draw_chunk = functions.read().id::<DrawVoxelChunk>();

    for (
        view,
        visible_entities,
        mut phase,
        tonemapping,
        dither,
        shadow_filter_method,
        projection,
        (normal_prepass, depth_prepass, motion_vector_prepass, deferred_prepass),
        ssao,
        temporal_jitter,
    ) in views.iter_mut()
    {
        let mut view_key = MeshPipelineKey::from_hdr(view.hdr);

        if normal_prepass {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }

        if depth_prepass {
            view_key |= MeshPipelineKey::DEPTH_PREPASS;
        }

        if motion_vector_prepass {
            view_key |= MeshPipelineKey::MOTION_VECTOR_PREPASS;
        }

        if deferred_prepass {
            view_key |= MeshPipelineKey::DEFERRED_PREPASS;
        }

        if temporal_jitter {
            view_key |= MeshPipelineKey::TEMPORAL_JITTER;
        }

        if ssao {
            view_key |= MeshPipelineKey::SCREEN_SPACE_AMBIENT_OCCLUSION;
        }

        if let Some(projection) = projection {
            view_key |= match projection {
                Projection::Perspective(_) => MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE,
                Projection::Orthographic(_) => MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC,
            };
        }

        match shadow_filter_method.unwrap_or(&ShadowFilteringMethod::default()) {
            ShadowFilteringMethod::Hardware2x2 => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_HARDWARE_2X2;
            }
            ShadowFilteringMethod::Castano13 => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_CASTANO_13;
            }
            ShadowFilteringMethod::Jimenez14 => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_JIMENEZ_14;
            }
        }

        if !view.hdr {
            if let Some(tonemapping) = tonemapping {
                view_key |= MeshPipelineKey::TONEMAP_IN_SHADER;
                view_key |= tonemapping_pipeline_key(*tonemapping);
            }
            if let Some(DebandDither::Enabled) = dither {
                view_key |= MeshPipelineKey::DEBAND_DITHER;
            }
        }

        iter_visible_chunks(visible_entities, &chunks, |entity, _chunk_pos| {
            let pipeline_id = pipelines.specialize(
                pipeline_cache.as_ref(),
                pipeline.as_ref(),
                ChunkPipelineKey {
                    inner: view_key
                        | MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList),
                },
            );

            // queue this entity for rendering
            phase.add(Opaque3d {
                entity: entity,
                draw_function: draw_chunk,
                pipeline: pipeline_id,
                asset_id: AssetId::default(),
                batch_range: 0..1,
                dynamic_offset: None,
            });
        });
    }
}

pub type DrawVoxelChunk = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetRegistryBindGroup<1>,
    SetChunkBindGroup<2>,
    DrawChunk,
);
