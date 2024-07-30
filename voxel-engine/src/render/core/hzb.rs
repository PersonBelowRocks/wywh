use crate::render::core::commands::{DrawDeferredBatch, IndirectBatchDraw, SetIndirectChunkQuads};
use crate::render::core::pipelines::{ChunkPipelineKey, ChunkRenderPipeline};
use crate::render::lod::LevelOfDetail;
use crate::topo::controller::{ChunkBatchLod, VisibleBatches};
use bevy::ecs::system::lifetimeless::Read;
use bevy::pbr::{LightEntity, MeshPipelineKey, SetMeshViewBindGroup};
use bevy::render::camera::Viewport;
use bevy::render::render_phase::{
    CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
    SetItemPipeline, SortedPhaseItem, ViewSortedRenderPhases,
};
use bevy::render::render_resource::{
    CachedComputePipelineId, CachedRenderPipelineId, Extent3d, LoadOp, Operations, PipelineCache,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, SpecializedRenderPipelines, StoreOp,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::ExtractedView;
use bevy::{
    ecs::entity::EntityHashMap,
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext},
        render_resource::{Texture, TextureView},
        renderer::RenderContext,
    },
};
use num::Integer;
use std::cmp::max;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct CachedHzbMipChain {
    pub state: CachedHzbState,
    pub texture: Texture,
    pub view: TextureView,
    pub dims: UVec2,
}

impl CachedHzbMipChain {
    pub fn mip_levels(&self) -> u32 {
        max(self.dims.x, self.dims.y).ilog2()
    }
}

#[derive(Clone, Debug)]
pub enum CachedHzbState {
    New,
    Existing,
}

#[derive(Clone, Debug, te::Error)]
pub enum HzbCacheError {
    #[error("Can't create HZB for dimensions {0}")]
    InvalidDimensions(UVec2),
}

#[derive(Resource, Default, Clone, Debug)]
pub struct HzbCache(EntityHashMap<CachedHzbMipChain>);

impl HzbCache {
    pub fn try_get_view_hzb(&self, view_entity: Entity) -> Option<&CachedHzbMipChain> {
        self.0.get(&view_entity)
    }

    pub fn view_hzb(
        &mut self,
        gpu: &RenderDevice,
        view_entity: Entity,
        dimensions: UVec2,
    ) -> Result<&CachedHzbMipChain, HzbCacheError> {
        // Need the dimensions to be even, at least for now. Maybe in the future we'll support odd dimensions
        if dimensions.x.is_odd() || dimensions.y.is_odd() {
            return Err(HzbCacheError::InvalidDimensions(dimensions));
        }

        if dimensions.x != dimensions.y {
            todo!("only square HZB dimensions are supported at the moment");
        }

        Ok(self.0.entry(view_entity).or_insert_with(|| {
            let mip_level_count = max(dimensions.x, dimensions.y).ilog2();

            let texture = gpu.create_texture(&TextureDescriptor {
                label: Some(&format!("hzb_mip_chain/{view_entity}")),
                size: Extent3d {
                    width: dimensions.x,
                    height: dimensions.y,
                    depth_or_array_layers: 1,
                },
                mip_level_count,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::TEXTURE_BINDING
                    | TextureUsages::STORAGE_BINDING,
                view_formats: &[TextureFormat::Depth32Float],
            });

            let texture_view = texture.create_view(&TextureViewDescriptor {
                label: Some(&format!("root_hzb_mip_chain_view/{view_entity}")),
                dimension: Some(TextureViewDimension::D2),
                array_layer_count: Some(1),
                format: Some(TextureFormat::Depth32Float),
                mip_level_count: None,
                aspect: TextureAspect::All,
                base_mip_level: 0,
                base_array_layer: 0,
            });

            CachedHzbMipChain {
                state: CachedHzbState::New,
                texture,
                view: texture_view,
                dims: dimensions,
            }
        }))
    }
}

#[derive(Clone, Debug)]
pub struct HzbPhase {
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

pub struct BuildHzbNode {
    q_views: QueryState<Read<ExtractedView>>,
}

impl FromWorld for BuildHzbNode {
    fn from_world(world: &mut World) -> Self {
        Self {
            q_views: QueryState::from_world(world),
        }
    }
}

impl Node for BuildHzbNode {
    fn update(&mut self, world: &mut World) {
        self.q_views.update_archetypes(world);
    }

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let phases = world.resource::<ViewSortedRenderPhases<HzbPhase>>();
        let cached_hzbs = world.resource::<HzbCache>();

        for (&view_entity, phase) in phases.iter() {
            let Some(cached_hzb) = cached_hzbs.try_get_view_hzb(view_entity) else {
                continue;
            };

            let Some(extracted_view) = self.q_views.get_manual(world, view_entity).ok() else {
                continue;
            };

            let mut render_pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some(&format!("hzb_root_depth_populate_pass/{view_entity}")),
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &cached_hzb.view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    }),
                    stencil_ops: Some(todo!()), // TODO: gotta figure this out
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            phase.render(&mut render_pass, world, view_entity);
        }

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
