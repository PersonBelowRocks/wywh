use bevy::{
    core_pipeline::{
        core_3d::Opaque3d,
        prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
    },
    pbr::{
        tonemapping_pipeline_key, MeshPipelineKey, ScreenSpaceAmbientOcclusionSettings,
        ShadowFilteringMethod,
    },
    prelude::*,
    render::{
        camera::TemporalJitter,
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, RenderPhase},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
        view::{ExtractedView, VisibleEntities},
    },
};

use crate::render::core::{gpu_chunk::IndirectRenderDataStore, gpu_registries::RegistryBindGroup};

use super::{IndirectChunkPipelineKey, IndirectChunkRenderPipeline, IndirectChunksRender};

pub fn render_queue_indirect_chunks(
    registry_bg: Option<Res<RegistryBindGroup>>,
    indirect_data: Res<IndirectRenderDataStore>,
    functions: Res<DrawFunctions<Opaque3d>>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<IndirectChunkRenderPipeline>>,
    pipeline: Res<IndirectChunkRenderPipeline>,
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
    // While we don't use the registry bind group in this system, we do use it in our draw function.
    // It's also possible for this system to run before the registry bind group is prepared, leading to
    // an error down the line in the draw function. To avoid this we only queue our indirect chunks if the
    // registry bind group is prepared.
    // We also only want to run the draw function if our indirect data is ready to be rendered.
    if registry_bg.is_none() || !indirect_data.ready {
        return;
    }

    let func = functions.read().id::<IndirectChunksRender>();

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

        let pipeline_id = pipelines.specialize(
            pipeline_cache.as_ref(),
            pipeline.as_ref(),
            IndirectChunkPipelineKey {
                inner: view_key
                    | MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList),
            },
        );

        phase.add(Opaque3d {
            pipeline: pipeline_id,
            asset_id: AssetId::default(),
            entity: Entity::PLACEHOLDER,
            draw_function: func,
            batch_range: 0..1,
            dynamic_offset: None,
        })
    }
}