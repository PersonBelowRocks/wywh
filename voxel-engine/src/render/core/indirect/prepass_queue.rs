use bevy::{
    core_pipeline::prepass::{
        DepthPrepass, MotionVectorPrepass, NormalPrepass, Opaque3dPrepass, OpaqueNoLightmap3dBinKey,
    },
    pbr::MeshPipelineKey,
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, ViewBinnedRenderPhases},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
    },
};

use crate::render::core::{gpu_chunk::IndirectRenderDataStore, gpu_registries::RegistryBindGroup};
use crate::{render::core::observers::RenderWorldObservers, topo::controller::ObserverId};

use super::{
    prepass_pipeline::IndirectChunkPrepassPipeline, IndirectChunkPipelineKey, IndirectChunksPrepass,
};

pub fn prepass_queue_indirect_chunks(
    registry_bg: Option<Res<RegistryBindGroup>>,
    indirect_data: Res<IndirectRenderDataStore>,
    functions: Res<DrawFunctions<Opaque3dPrepass>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<IndirectChunkPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    prepass_pipeline: Res<IndirectChunkPrepassPipeline>,
    observers: Res<RenderWorldObservers>,
    mut phases: ResMut<ViewBinnedRenderPhases<Opaque3dPrepass>>,
    views: Query<(
        Entity,
        &ObserverId,
        Has<DepthPrepass>,
        Has<NormalPrepass>,
        Has<MotionVectorPrepass>,
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

    let draw_function = functions.read().get_id::<IndirectChunksPrepass>().unwrap();

    for (view_entity, id, depth_prepass, normal_prepass, motion_vector_prepass) in &views {
        let Some(phase) = phases.get_mut(&view_entity) else {
            continue;
        };

        if !observers
            .get(id)
            .and_then(|data| data.buffers.as_ref())
            .is_some()
        {
            continue;
        }

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

        let pipeline_id = pipelines.specialize(
            &pipeline_cache,
            &prepass_pipeline,
            IndirectChunkPipelineKey {
                inner: MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList)
                    | view_key,
            },
        );

        let phase_item_key = OpaqueNoLightmap3dBinKey {
            pipeline: pipeline_id,
            draw_function,
            asset_id: AssetId::default(),
            material_bind_group_id: None,
        };

        phase.add(phase_item_key, Entity::PLACEHOLDER, false);
    }
}
