use std::ops::Range;

use bevy::{
    prelude::Entity,
    render::{
        render_phase::{
            CachedRenderPipelinePhaseItem, DrawFunctionId, PhaseItem, PhaseItemExtraIndex,
            SortedPhaseItem,
        },
        render_resource::CachedRenderPipelineId,
    },
};

use crate::render::lod::LevelOfDetail;

#[derive(Clone)]
pub struct DeferredBatch3d {
    pub pipeline: CachedRenderPipelineId,
    pub draw_function: DrawFunctionId,
    pub entity: Entity,
    pub lod: LevelOfDetail,
    pub batch_range: Range<u32>,
    pub extra_index: PhaseItemExtraIndex,
}

impl PhaseItem for DeferredBatch3d {
    const AUTOMATIC_BATCHING: bool = false;

    fn batch_range(&self) -> &Range<u32> {
        &self.batch_range
    }

    fn batch_range_mut(&mut self) -> &mut Range<u32> {
        &mut self.batch_range
    }

    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function
    }

    fn entity(&self) -> Entity {
        self.entity
    }

    fn extra_index(&self) -> PhaseItemExtraIndex {
        self.extra_index
    }

    fn batch_range_and_extra_index_mut(&mut self) -> (&mut Range<u32>, &mut PhaseItemExtraIndex) {
        (&mut self.batch_range, &mut self.extra_index)
    }
}

impl SortedPhaseItem for DeferredBatch3d {
    type SortKey = LevelOfDetail;

    fn sort_key(&self) -> Self::SortKey {
        self.lod
    }
}

impl CachedRenderPipelinePhaseItem for DeferredBatch3d {
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}
