use crate::render::core::lights::get_parent_light;
use crate::render::core::occlusion::occluders::{
    OccluderBoxes, OccluderDepthPipeline, OccluderModel, OCCLUDER_BOX_INDICES,
};
use crate::render::core::shaders::CONSTRUCT_HZB_LEVEL_HANDLE;
use crate::render::core::BindGroupProvider;
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::system::lifetimeless::Read;
use bevy::pbr::{LightEntity, PrepassViewBindGroup};
use bevy::render::camera::Viewport;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{
    BindGroup, CachedComputePipelineId, CachedRenderPipelineId, CompareFunction, DepthBiasState,
    DepthStencilState, Extent3d, FragmentState, ImageSubresourceRange, IndexFormat, LoadOp,
    MultisampleState, Operations, PipelineCache, PrimitiveState, RenderPassDepthStencilAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, StencilState, StoreOp,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::DepthAttachment;
use bevy::render::view::{ExtractedView, ViewUniformOffset};
use bevy::{
    ecs::entity::EntityHashMap,
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext},
        render_resource::Texture,
        renderer::RenderContext,
    },
};
use num::Integer;
use std::cmp::min;

pub const HZB_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

#[derive(Component, ExtractComponent, Debug, Copy, Clone)]
pub struct ChunkHzbOcclusionCulling;

pub fn inherit_parent_light_hzb_culling_marker(
    q_light_entities: Query<(Entity, &LightEntity)>,
    q_hzb_culling_enabled: Query<(), With<ChunkHzbOcclusionCulling>>,
    mut cmds: Commands,
) {
    for (entity, light) in &q_light_entities {
        let parent_light = get_parent_light(light);

        if q_hzb_culling_enabled.contains(parent_light) {
            cmds.get_or_spawn(entity).insert(ChunkHzbOcclusionCulling);
        }
    }
}

pub struct CachedHzbMipChain {
    pub depth_attachment: DepthAttachment,
    pub texture: Texture,
    pub dims: UVec2,
}

impl CachedHzbMipChain {
    pub fn mip_level_dimensions(&self, mip_level: u32) -> UVec2 {
        self.dims / (2u32.pow(mip_level))
    }

    pub fn mip_levels(&self) -> u32 {
        min(self.dims.x, self.dims.y).ilog2()
    }

    pub fn mip_level_view(&self, mip_level: u32) -> Option<TextureView> {
        if mip_level > self.mip_levels() {
            return None;
        }

        let view = self.texture.create_view(&TextureViewDescriptor {
            label: Some(&format!("hzb_mip_view/{mip_level}")),
            format: Some(TextureFormat::Depth32Float),
            dimension: Some(TextureViewDimension::D2),
            aspect: TextureAspect::All,
            base_array_layer: 0,
            array_layer_count: None,
            base_mip_level: mip_level,
            mip_level_count: Some(1),
        });

        Some(view)
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
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
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
        mip_level_count: Some(1),
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

#[derive(Resource, Default)]
pub struct QueuedViewHzbs(Vec<QueuedHzb>);

#[derive(Clone, Debug)]
pub struct QueuedHzb {
    pub view_entity: Entity,
}

pub fn viewport_mip_levels(viewport: UVec2) -> u32 {
    min(viewport.x, viewport.y).ilog2()
}

pub fn prepare_view_hzbs(
    mut cache: ResMut<HzbCache>,
    mut queued: ResMut<QueuedViewHzbs>,
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
        });
    }
}

#[derive(Resource)]
pub struct HzbLevelPipeline {
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
            primitive: PrimitiveState {
                unclipped_depth: true,
                ..default()
            },
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

        Self { pipeline_id }
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

/// Render the initial high detail depth buffer for HZBs
fn hzb_depth_pass<'w>(
    world: &World,
    ctx: &mut RenderContext<'w>,
    q_views: &QueryState<Read<ViewUniformOffset>>,
    occluder_model: &OccluderModel,
    occluders: &OccluderBoxes,
    queued_hzbs: &QueuedViewHzbs,
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

        // Need to clear the depth buffer
        ctx.command_encoder().clear_texture(
            &hzb.texture,
            &ImageSubresourceRange {
                base_mip_level: 0,
                mip_level_count: None,
                aspect: TextureAspect::All,
                base_array_layer: 0,
                array_layer_count: None,
            },
        );

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

/// Downsample HZBs from the lowest (highest detail) mip level.
fn hzb_downsample<'w>(
    ctx: &mut RenderContext<'w>,
    queued_views: &QueuedViewHzbs,
    hzb_cache: &HzbCache,
    bg_provider: &BindGroupProvider,
    downsample_pipeline: &RenderPipeline,
) {
    for view in &queued_views.0 {
        let Some(hzb) = hzb_cache.get_view_hzb(view.view_entity) else {
            error!("Failed to get cached HZB for queued view");
            continue;
        };

        for next_mip_level in 1..hzb.mip_levels() {
            let previous_mip_level = next_mip_level - 1;

            let previous_mip = hzb.mip_level_view(previous_mip_level).unwrap();
            let next_mip = hzb.mip_level_view(next_mip_level).unwrap();

            let depth_attachment = RenderPassDepthStencilAttachment {
                // Render downsampled depth to the next mip level
                view: &next_mip,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            };

            let previous_depth_bg = bg_provider.hzb_level_bg(ctx.render_device(), &previous_mip);

            let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some(&format!(
                    "hzb_downsample_pass {previous_mip_level}->{next_mip_level}"
                )),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_render_pipeline(downsample_pipeline);

            pass.set_camera_viewport(&Viewport {
                physical_position: UVec2::ZERO,
                physical_size: hzb.mip_level_dimensions(next_mip_level),
                depth: 0.0..1.0,
            });

            pass.set_bind_group(0, &previous_depth_bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }
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

        let queued_views = world.resource::<QueuedViewHzbs>();
        let hzb_cache = world.resource::<HzbCache>();

        let success = hzb_depth_pass(
            world,
            ctx,
            &self.q_views,
            world.resource::<OccluderModel>(),
            world.resource::<OccluderBoxes>(),
            queued_views,
            hzb_cache,
            occluder_depth_pipeline,
            prepass_view_bind_group,
        );

        if !success {
            error!("Failed to run HZB depth pass");
            return Ok(());
        }

        let Some(downsample_pipeline) = get_hzb_level_pipeline(world) else {
            error!("Could not get HZB level pipeline");
            return Ok(());
        };

        hzb_downsample(
            ctx,
            queued_views,
            hzb_cache,
            world.resource::<BindGroupProvider>(),
            downsample_pipeline,
        );

        Ok(())
    }
}
