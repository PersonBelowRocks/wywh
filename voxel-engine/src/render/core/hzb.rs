use crate::render::core::commands::{DrawDeferredBatch, IndirectBatchDraw, SetIndirectChunkQuads};
use crate::render::core::pipelines::{ChunkPipelineKey, ChunkRenderPipeline};
use crate::render::lod::LevelOfDetail;
use crate::topo::controller::{ChunkBatchLod, VisibleBatches};
use bevy::pbr::{LightEntity, MeshPipelineKey, SetMeshViewBindGroup};
use bevy::render::render_phase::{
    CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
    SetItemPipeline, SortedPhaseItem, ViewSortedRenderPhases,
};
use bevy::render::render_resource::{
    CachedRenderPipelineId, PipelineCache, SpecializedRenderPipelines,
};
use bevy::{
    ecs::entity::EntityHashMap,
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext},
        render_resource::{Texture, TextureView},
        renderer::RenderContext,
    },
};
use std::ops::Range;

#[derive(Default)]
pub struct BuildHzbNode;

#[derive(Clone, Debug)]
pub struct CachedHzbMipChain {
    pub texture: Texture,
    pub view: TextureView,
    pub dims: UVec2,
}

#[derive(Resource, Clone, Debug)]
pub struct HzbCache(EntityHashMap<CachedHzbMipChain>);

#[derive(Clone, Debug)]
pub struct HzbPhase {
    pub view_entity: Entity,
    pub batch_entity: Entity,
    pub batch_lod: LevelOfDetail,
    pub pipeline_id: CachedRenderPipelineId,
    pub draw_function_id: DrawFunctionId,
}

impl PhaseItem for HzbPhase {
    fn entity(&self) -> Entity {
        self.batch_entity
    }

    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function_id
    }

    fn batch_range(&self) -> &Range<u32> {
        unimplemented!()
    }

    fn batch_range_mut(&mut self) -> &mut Range<u32> {
        unimplemented!()
    }

    fn extra_index(&self) -> PhaseItemExtraIndex {
        unimplemented!()
    }

    fn batch_range_and_extra_index_mut(&mut self) -> (&mut Range<u32>, &mut PhaseItemExtraIndex) {
        unimplemented!()
    }
}

impl SortedPhaseItem for HzbPhase {
    type SortKey = LevelOfDetail;

    fn sort_key(&self) -> Self::SortKey {
        self.batch_lod
    }
}

impl CachedRenderPipelinePhaseItem for HzbPhase {
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline_id
    }
}

impl Node for BuildHzbNode {
    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let cached = world.resource::<HzbCache>();

        Ok(())
    }
}

pub fn queue_directional_light_hzbs(
    q_directional_lights: Query<(Entity, &VisibleBatches), With<LightEntity>>,
    q_batches: Query<&ChunkBatchLod>,
    mut phases: ResMut<ViewSortedRenderPhases<HzbPhase>>,
    pipeline: Res<ChunkRenderPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<ChunkRenderPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    draw_functions: Res<DrawFunctions<HzbPhase>>,
) {
    let draw_function_id = draw_functions.read().id::<DrawDirectionalLightDepth>();

    for (entity, visible_batches) in &q_directional_lights {
        let Some(phase) = phases.get_mut(&entity) else {
            continue;
        };

        for &batch_entity in visible_batches.iter() {
            let Some(lod) = q_batches.get(batch_entity).ok() else {
                continue;
            };

            let lod = lod.0;

            let pipeline_id = pipelines.specialize(
                &pipeline_cache,
                &pipeline,
                ChunkPipelineKey {
                    // Specializing with only the depth prepass key lets us create a pipeline without
                    // a fragment shader, speeding things up a bit.
                    inner: MeshPipelineKey::DEPTH_PREPASS,
                    shadow_pass: false,
                },
            );

            phase.add(HzbPhase {
                view_entity: entity,
                batch_entity,
                batch_lod: lod,
                pipeline_id,
                draw_function_id,
            });
        }
    }
}

pub type DrawDirectionalLightDepth = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetIndirectChunkQuads<1>,
    IndirectBatchDraw,
);
