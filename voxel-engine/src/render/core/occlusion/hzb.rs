use crate::render::core::commands::{DrawDeferredBatch, IndirectBatchDraw, SetIndirectChunkQuads};
use crate::render::core::pipelines::{ChunkPipelineKey, ChunkRenderPipeline};
use crate::render::core::shaders::CONSTRUCT_HZB_LEVEL_HANDLE;
use crate::render::core::utils::u32_shader_def;
use crate::render::core::BindGroupProvider;
use crate::render::lod::LevelOfDetail;
use crate::topo::controller::{ChunkBatchLod, VisibleBatches};
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::system::lifetimeless::Read;
use bevy::pbr::{LightEntity, MeshPipelineKey, SetMeshViewBindGroup};
use bevy::render::camera::Viewport;
use bevy::render::render_phase::{
    CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
    SetItemPipeline, SortedPhaseItem, ViewSortedRenderPhases,
};
use bevy::render::render_resource::{
    BindGroupLayout, CachedComputePipelineId, CachedRenderPipelineId, CompareFunction,
    ComputePipelineDescriptor, DepthBiasState, DepthStencilState, Extent3d, FragmentState, LoadOp,
    MultisampleState, Operations, PipelineCache, PolygonMode, PrimitiveState,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    SpecializedComputePipeline, SpecializedRenderPipeline, SpecializedRenderPipelines,
    StencilState, StoreOp, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::DepthAttachment;
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
    pub fn try_get_view_hzb(&self, view_entity: Entity) -> Option<&CachedHzbMipChain> {
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
