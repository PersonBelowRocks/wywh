use bevy::{
    core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass, Opaque3dPrepass},
    pbr::MeshPipelineKey,
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_phase::{DrawFunctions, RenderPhase},
        render_resource::{PipelineCache, SpecializedRenderPipelines},
        view::{ExtractedView, VisibleEntities},
    },
};

use crate::render::core::{gpu_chunk::IndirectRenderDataStore, gpu_registries::RegistryBindGroup};

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
    mut views: Query<(
        &ExtractedView,
        &VisibleEntities,
        &mut RenderPhase<Opaque3dPrepass>,
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

    for (
        _view,
        _visible_entities,
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

        let pipeline_id = pipelines.specialize(
            &pipeline_cache,
            &prepass_pipeline,
            IndirectChunkPipelineKey {
                inner: MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList)
                    | view_key,
            },
        );

        phase.add(Opaque3dPrepass {
            entity: Entity::PLACEHOLDER,
            draw_function: draw_function,
            pipeline_id,
            // this asset ID is seemingly just for some sorting stuff bevy does, but we have our own
            // logic so we don't care about what bevy would use this field for, so we set it to the default asset ID
            asset_id: AssetId::default(),
            batch_range: 0..1,
            dynamic_offset: None,
        });
    }
}
