use crate::render::core::commands::{DrawDeferredBatch, IndirectBatchDraw, SetIndirectChunkQuads};
use crate::render::core::occlusion::occluders::{
    OccluderBoxes, OccluderDepthPipeline, OccluderModel, OCCLUDER_BOX_INDICES,
};
use crate::render::core::pipelines::{ChunkPipelineKey, ChunkRenderPipeline};
use crate::render::core::shaders::CONSTRUCT_HZB_LEVEL_HANDLE;
use crate::render::core::utils::u32_shader_def;
use crate::render::core::BindGroupProvider;
use crate::render::lod::LevelOfDetail;
use crate::topo::controller::{ChunkBatchLod, VisibleBatches};
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::system::lifetimeless::Read;
use bevy::pbr::{LightEntity, MeshPipelineKey, PrepassViewBindGroup, SetMeshViewBindGroup};
use bevy::render::camera::Viewport;
use bevy::render::render_graph::RunSubGraphError;
use bevy::render::render_phase::{
    CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
    SetItemPipeline, SortedPhaseItem, ViewSortedRenderPhases,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupLayout, Buffer, CachedComputePipelineId, CachedRenderPipelineId,
    CompareFunction, ComputePipelineDescriptor, DepthBiasState, DepthStencilState, Extent3d,
    FragmentState, IndexFormat, LoadOp, MultisampleState, Operations, PipelineCache, PolygonMode,
    PrimitiveState, RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, SpecializedComputePipeline, SpecializedRenderPipeline,
    SpecializedRenderPipelines, StencilState, StoreOp, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::DepthAttachment;
use bevy::render::view::{ExtractedView, ViewUniformOffset};
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
use std::cmp::{max, min};

pub const HZB_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

#[derive(Component, Debug, Copy, Clone)]
pub struct ChunkHzbOcclusionCulling;

pub struct CachedHzbMipChain {
    pub depth_attachment: DepthAttachment,
    pub texture: Texture,
    pub dims: UVec2,
}

impl CachedHzbMipChain {
    pub fn mip_levels(&self) -> u32 {
        min(self.dims.x, self.dims.y).ilog2()
    }
}

#[derive(Clone, Debug, te::Error)]
pub enum HzbCacheError {
    #[error("HZB viewport contained an odd component: {0}")]
    OddDimensions(UVec2),
    #[error("HZB viewport must be square, but was {0}")]
    NonSquare(UVec2),
}

/// Create an HZB texture with the given dimensions. Number of mip levels is the log2 of the smallest dimension.
/// Label is for debugging only.
pub fn create_hzb_texture_desc(label: &str, dimensions: UVec2) -> TextureDescriptor {
    let mip_level_count = min(dimensions.x, dimensions.y).ilog2();

    TextureDescriptor {
        label: Some(label),
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
    }
}

/// Create a texture view for an HZB. Label is for debugging only.
pub fn create_hzb_texture_view_desc(label: &str) -> TextureViewDescriptor {
    TextureViewDescriptor {
        label: Some(label),
        dimension: Some(TextureViewDimension::D2),
        array_layer_count: Some(1),
        format: Some(TextureFormat::Depth32Float),
        mip_level_count: None,
        aspect: TextureAspect::All,
        base_mip_level: 0,
        base_array_layer: 0,
    }
}

/// Caches HZBs for views
#[derive(Resource, Default)]
pub struct HzbCache(EntityHashMap<CachedHzbMipChain>);

impl HzbCache {
    pub fn get_view_hzb(&self, view_entity: Entity) -> Option<&CachedHzbMipChain> {
        self.0.get(&view_entity)
    }

    pub fn create_view_hzb(
        &mut self,
        gpu: &RenderDevice,
        view_entity: Entity,
        dimensions: UVec2,
    ) -> Result<(), HzbCacheError> {
        // Need the dimensions to be even, at least for now. Maybe in the future we'll support odd dimensions
        if dimensions.x.is_odd() || dimensions.y.is_odd() {
            return Err(HzbCacheError::OddDimensions(dimensions));
        }

        if dimensions.x != dimensions.y {
            return Err(HzbCacheError::NonSquare(dimensions));
        }

        self.0.entry(view_entity).or_insert_with(|| {
            let label = format!("hzb/{view_entity}");

            let texture = gpu.create_texture(&create_hzb_texture_desc(&label, dimensions));
            let texture_view = texture.create_view(&create_hzb_texture_view_desc(&label));

            CachedHzbMipChain {
                texture,
                depth_attachment: DepthAttachment::new(texture_view, Some(0.0)),
                dims: dimensions,
            }
        });

        Ok(())
    }
}

#[derive(Resource)]
pub struct QueuedHzbViews(Vec<QueuedHzb>);

#[derive(Clone, Debug)]
pub struct QueuedHzb {
    pub view_entity: Entity,
    pub mip_level_compute_chain: Vec<CachedComputePipelineId>,
}

pub fn viewport_mip_levels(viewport: UVec2) -> u32 {
    min(viewport.x, viewport.y).ilog2()
}

pub fn prepare_hzbs(
    mut cache: ResMut<HzbCache>,
    mut queued: ResMut<QueuedHzbViews>,
    gpu: Res<RenderDevice>,
    q_views: Query<(Entity, &ExtractedView), With<ChunkHzbOcclusionCulling>>,
) {
    queued.0.clear();

    for (entity, view) in &q_views {
        let result = cache.create_view_hzb(&gpu, entity, view.viewport.zw());

        if let Err(error) = result {
            error!("Error creating HZB for view {entity}: {error}");
            continue;
        }

        queued.0.push(QueuedHzb {
            view_entity: entity,
            mip_level_compute_chain: vec![],
        });
    }
}

#[derive(Resource)]
pub struct HzbLevelPipeline {
    layout: BindGroupLayout,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for HzbLevelPipeline {
    fn from_world(world: &mut World) -> Self {
        let provider = world.resource::<BindGroupProvider>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let layout = provider.construct_hzb_level_bg_layout.clone();

        let descriptor = RenderPipelineDescriptor {
            label: Some(format!("construct_hzb_mip_level").into()),
            push_constant_ranges: vec![],
            primitive: PrimitiveState::default(),
            vertex: fullscreen_shader_vertex_state(),
            multisample: MultisampleState::default(),
            layout: vec![layout.clone()],
            fragment: Some(FragmentState {
                shader: CONSTRUCT_HZB_LEVEL_HANDLE,
                shader_defs: vec![],
                entry_point: "construct_hzb_level".into(),
                targets: vec![],
            }),
            depth_stencil: Some(DepthStencilState {
                format: HZB_DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Always,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
        };

        let pipeline_id = pipeline_cache.queue_render_pipeline(descriptor);

        Self {
            layout,
            pipeline_id,
        }
    }
}

pub struct HzbConstructionNode {
    q_views: QueryState<Read<ViewUniformOffset>>,
}

impl FromWorld for HzbConstructionNode {
    fn from_world(world: &mut World) -> Self {
        Self {
            q_views: QueryState::from_world(world),
        }
    }
}

fn get_occluder_depth_pipeline(world: &World) -> Option<&RenderPipeline> {
    let pipeline_cache = world.resource::<PipelineCache>();
    let pipeline_id = world.resource::<OccluderDepthPipeline>().pipeline_id;

    pipeline_cache.get_render_pipeline(pipeline_id)
}

fn get_hzb_level_pipeline(world: &World) -> Option<&RenderPipeline> {
    let pipeline_cache = world.resource::<PipelineCache>();
    let pipeline_id = world.resource::<HzbLevelPipeline>().pipeline_id;

    pipeline_cache.get_render_pipeline(pipeline_id)
}

fn get_prepass_view_bind_group(world: &World) -> Option<&BindGroup> {
    let prepass_view_bind_group = world.resource::<PrepassViewBindGroup>();
    prepass_view_bind_group.no_motion_vectors.as_ref()
}

fn hzb_depth_pass<'w>(
    world: &World,
    ctx: &mut RenderContext<'w>,
    q_views: &QueryState<Read<ViewUniformOffset>>,
    occluder_model: &OccluderModel,
    occluders: &OccluderBoxes,
    queued_hzbs: &QueuedHzbViews,
    hzb_cache: &HzbCache,
    occluder_depth_pipeline: &RenderPipeline,
    prepass_view_bind_group: &BindGroup,
) -> bool {
    let Some(occluder_instance_buffer) = occluders.buffer() else {
        error!("Could not get occluder instance buffer");
        return false;
    };

    let num_indices = OCCLUDER_BOX_INDICES.len() as u32;
    let num_instances = occluders.len() as u32;

    for queued in &queued_hzbs.0 {
        let Some(view_offset) = q_views.get_manual(world, queued.view_entity).ok() else {
            error!("Can't get view offset for {}", queued.view_entity);
            continue;
        };

        let Some(hzb) = hzb_cache.get_view_hzb(queued.view_entity) else {
            error!(
                "View {} was queued for HZB building but didn't have an HZB",
                queued.view_entity
            );
            continue;
        };

        let depth_attachment = hzb.depth_attachment.get_attachment(StoreOp::Store);

        let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("hzb_depth_pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(depth_attachment),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_render_pipeline(occluder_depth_pipeline);

        // Set the view bind group
        pass.set_bind_group(0, prepass_view_bind_group, &[view_offset.offset]);

        // Set the instance buffer
        pass.set_vertex_buffer(0, occluder_instance_buffer.slice(..));

        let index_buffer = occluder_model.index_buffer.slice(..);
        pass.set_index_buffer(index_buffer, 0, IndexFormat::Uint32);

        let vertex_buffer = occluder_model.vertex_buffer.slice(..);
        pass.set_vertex_buffer(1, vertex_buffer);

        pass.draw_indexed(0..num_indices, 0, 0..num_instances);
    }

    true
}

impl Node for HzbConstructionNode {
    fn update(&mut self, world: &mut World) {
        self.q_views.update_archetypes(world);
    }

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(occluder_depth_pipeline) = get_occluder_depth_pipeline(world) else {
            error!("Could not get occluder depth pipeline");
            return Ok(());
        };

        let Some(prepass_view_bind_group) = get_prepass_view_bind_group(world) else {
            error!("Could not get prepass view bind group");
            return Ok(());
        };

        let success = hzb_depth_pass(
            world,
            ctx,
            &self.q_views,
            world.resource::<OccluderModel>(),
            world.resource::<OccluderBoxes>(),
            world.resource::<QueuedHzbViews>(),
            world.resource::<HzbCache>(),
            occluder_depth_pipeline,
            prepass_view_bind_group,
        );

        if !success {
            return Ok(());
        }

        let Some(hzb_level_pipeline) = get_hzb_level_pipeline(world) else {
            error!("Could not get HZB level pipeline");
            return Ok(());
        };

        todo!()
    }
}
