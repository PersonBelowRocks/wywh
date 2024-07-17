use bevy::{
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
    pbr::MeshPipelineKey,
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, PhaseItemExtraIndex, ViewSortedRenderPhases},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
    },
};

use crate::topo::controller::{ChunkBatchLod, VisibleBatches};

use super::{
    commands::DrawDeferredBatch,
    gpu_chunk::IndirectRenderDataStore,
    gpu_registries::RegistryBindGroup,
    phase::DeferredBatch3d,
    pipelines::{ChunkPipelineKey, DeferredIndirectChunkPipeline},
};

/// Queue chunks for the render phase
pub fn queue_deferred_chunks(
    //////////////////////////////////////////////////////////////////////////
    q_views: Query<(
        Entity,
        &VisibleBatches,
        Option<&NormalPrepass>,
        Option<&DepthPrepass>,
        Option<&MotionVectorPrepass>,
        Option<&DeferredPrepass>,
    )>,
    q_batches: Query<&ChunkBatchLod>,
    mut phases: ResMut<ViewSortedRenderPhases<DeferredBatch3d>>,

    //////////////////////////////////////////////////////////////////////////
    functions: Res<DrawFunctions<DeferredBatch3d>>,
    pipeline: Res<DeferredIndirectChunkPipeline>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<DeferredIndirectChunkPipeline>>,
    //////////////////////////////////////////////////////////////////////////
    registry_bg: Option<Res<RegistryBindGroup>>,
    mesh_data: Res<IndirectRenderDataStore>,
    msaa: Res<Msaa>,
) {
    // While we don't use the registry bind group in this system, we do use it in our draw function.
    // It's also possible for this system to run before the registry bind group is prepared, leading to
    // an error down the line in the draw function. To avoid this we only queue our indirect chunks if the
    // registry bind group is prepared.
    if registry_bg.is_none() {
        return;
    }

    let function = functions.read().id::<DrawDeferredBatch>();

    for (
        view_entity,
        visible_batches,
        normal_prepass,
        depth_prepass,
        motion_vector_prepass,
        deferred_prepass,
    ) in q_views.iter()
    {
        if deferred_prepass.is_none() {
            continue;
        }

        // Create the pipeline key
        //////////////////////////////////////////////////////////////////////////

        let mut view_key = MeshPipelineKey::from_msaa_samples(msaa.samples());
        view_key |= MeshPipelineKey::DEFERRED_PREPASS;

        if depth_prepass.is_some() {
            view_key |= MeshPipelineKey::DEPTH_PREPASS;
        }
        if normal_prepass.is_some() {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }
        if motion_vector_prepass.is_some() {
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
            ChunkPipelineKey {
                inner: view_key
                    | MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList),
                shadow_pass: false,
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

            let phase_item = DeferredBatch3d {
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
