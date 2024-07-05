use bevy::{
    core_pipeline::{
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
        render_phase::{DrawFunctions, PhaseItemExtraIndex, ViewSortedRenderPhases},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
        view::ExtractedView,
    },
};

use crate::topo::controller::{ChunkBatchLod, VisibleBatches};

use super::{
    commands::{IndirectChunksPrepass, IndirectChunksRender},
    gpu_chunk::IndirectRenderDataStore,
    gpu_registries::RegistryBindGroup,
    phase::{PrepassChunkPhaseItem, RenderChunkPhaseItem},
    prepass_pipeline::IndirectChunkPrepassPipeline,
    render_pipeline::{IndirectChunkPipelineKey, IndirectChunkRenderPipeline},
};

/// Queue chunks for the render phase
pub fn render_queue_chunks(
    //////////////////////////////////////////////////////////////////////////
    q_views: Query<(
        Entity,
        &ExtractedView,
        &VisibleBatches,
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
    q_batches: Query<&ChunkBatchLod>,
    mut phases: ResMut<ViewSortedRenderPhases<RenderChunkPhaseItem>>,

    //////////////////////////////////////////////////////////////////////////
    functions: Res<DrawFunctions<RenderChunkPhaseItem>>,
    pipeline: Res<IndirectChunkRenderPipeline>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<IndirectChunkRenderPipeline>>,
    //////////////////////////////////////////////////////////////////////////
    registry_bg: Option<Res<RegistryBindGroup>>,
    mesh_data: Res<IndirectRenderDataStore>,
) {
    // While we don't use the registry bind group in this system, we do use it in our draw function.
    // It's also possible for this system to run before the registry bind group is prepared, leading to
    // an error down the line in the draw function. To avoid this we only queue our indirect chunks if the
    // registry bind group is prepared.
    if registry_bg.is_none() {
        return;
    }

    let function = functions.read().id::<IndirectChunksRender>();

    for (
        view_entity,
        view,
        visible_batches,
        tonemapping,
        dither,
        shadow_filter_method,
        projection,
        (normal_prepass, depth_prepass, motion_vector_prepass, deferred_prepass),
        ssao,
        temporal_jitter,
    ) in q_views.iter()
    {
        // Create the pipeline key
        //////////////////////////////////////////////////////////////////////////

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
            ShadowFilteringMethod::Gaussian => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_GAUSSIAN;
            }
            ShadowFilteringMethod::Temporal => {
                view_key |= MeshPipelineKey::SHADOW_FILTER_METHOD_TEMPORAL;
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

        // Queue the batches in the phase
        //////////////////////////////////////////////////////////////////////////

        let Some(phase) = phases.get_mut(&view_entity) else {
            continue;
        };

        let pipeline_id = pipelines.specialize(
            pipeline_cache.as_ref(),
            pipeline.as_ref(),
            IndirectChunkPipelineKey {
                inner: view_key
                    | MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList),
            },
        );

        for &batch_entity in visible_batches.iter() {
            let Ok(lod) = q_batches.get(batch_entity) else {
                warn!(
                    "Observer view entity had a visible batch that didn't have an LOD component."
                );
                continue;
            };

            let lod = lod.0;

            // Don't queue if the mesh data for this LOD isn't ready.
            if !mesh_data.lod(lod).is_ready() {
                continue;
            }

            let phase_item = RenderChunkPhaseItem {
                pipeline: pipeline_id,
                draw_function: function,
                entity: batch_entity,
                lod,
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex(0),
            };

            phase.add(phase_item);
        }
    }
}

/// Queue chunks for the prepass phase
pub fn prepass_queue_chunks(
    //////////////////////////////////////////////////////////////////////////
    q_views: Query<(
        Entity,
        &VisibleBatches,
        Has<NormalPrepass>,
        Has<DepthPrepass>,
        Has<MotionVectorPrepass>,
    )>,
    q_batches: Query<&ChunkBatchLod>,
    mut phases: ResMut<ViewSortedRenderPhases<PrepassChunkPhaseItem>>,

    //////////////////////////////////////////////////////////////////////////
    functions: Res<DrawFunctions<PrepassChunkPhaseItem>>,
    pipeline: Res<IndirectChunkPrepassPipeline>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<IndirectChunkPrepassPipeline>>,
    //////////////////////////////////////////////////////////////////////////
    registry_bg: Option<Res<RegistryBindGroup>>,
    mesh_data: Res<IndirectRenderDataStore>,
) {
    // While we don't use the registry bind group in this system, we do use it in our draw function.
    // It's also possible for this system to run before the registry bind group is prepared, leading to
    // an error down the line in the draw function. To avoid this we only queue our indirect chunks if the
    // registry bind group is prepared.
    if registry_bg.is_none() {
        return;
    }

    let function = functions.read().id::<IndirectChunksPrepass>();

    for (view_entity, visible_batches, normal_prepass, depth_prepass, motion_vector_prepass) in
        q_views.iter()
    {
        // Create the pipeline key
        //////////////////////////////////////////////////////////////////////////

        let mut view_key = MeshPipelineKey::empty();

        if normal_prepass {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }

        if depth_prepass {
            view_key |= MeshPipelineKey::DEPTH_PREPASS;
        }

        if motion_vector_prepass {
            view_key |= MeshPipelineKey::MOTION_VECTOR_PREPASS;
        }

        // Queue the batches in the phase
        //////////////////////////////////////////////////////////////////////////

        let Some(phase) = phases.get_mut(&view_entity) else {
            continue;
        };

        let pipeline_id = pipelines.specialize(
            pipeline_cache.as_ref(),
            pipeline.as_ref(),
            IndirectChunkPipelineKey {
                inner: view_key
                    | MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList),
            },
        );

        for &batch_entity in visible_batches.iter() {
            let Ok(lod) = q_batches.get(batch_entity) else {
                warn!(
                    "Observer view entity had a visible batch that didn't have an LOD component."
                );
                continue;
            };

            let lod = lod.0;

            // Don't queue if the mesh data for this LOD isn't ready.
            if !mesh_data.lod(lod).is_ready() {
                continue;
            }

            let phase_item = PrepassChunkPhaseItem {
                pipeline: pipeline_id,
                draw_function: function,
                entity: batch_entity,
                lod,
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex(0),
            };

            phase.add(phase_item);
        }
    }
}
