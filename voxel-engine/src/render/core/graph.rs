use bevy::{
    core_pipeline::prepass::ViewPrepassTextures,
    ecs::{entity::EntityHashMap, query::QueryItem, system::lifetimeless::Read},
    pbr::{LightEntity, LightMeta, ShadowView, ViewLightEntities, ViewLightsUniformOffset},
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
        render_phase::{TrackedRenderPass, ViewSortedRenderPhases},
        render_resource::{
            BindGroup, BindingResource, Buffer, CommandEncoder, CommandEncoderDescriptor,
            ComputePass, ComputePassDescriptor, ComputePipeline, PipelineCache,
            RenderPassDescriptor, ShaderSize, StoreOp,
        },
        renderer::{RenderContext, RenderDevice},
        texture::ColorAttachment,
        view::{ViewDepthTexture, ViewUniformOffset, ViewUniforms},
    },
};

use crate::{
    render::{core::views::IndirectViewBatch, lod::LevelOfDetail},
    topo::controller::{ChunkBatchLod, VisibleBatches},
};

use super::{
    gpu_chunk::IndirectRenderDataStore,
    occlusion::hzb::HzbCache,
    phase::DeferredBatch3d,
    pipelines::{
        ViewBatchLightPreprocessPipelineId, ViewBatchPreprocessPipelineId,
        PREPROCESS_BATCH_WORKGROUP_SIZE,
    },
    views::ViewBatchBuffersStore,
    BindGroupProvider,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, RenderLabel)]
pub enum CoreNode {
    HzbPass,
    PreprocessBatches,
    PreprocessLightBatches,
    Prepass,
}

/////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Deferred rendering

#[derive(Default)]
pub struct DeferredChunkNode;

impl ViewNode for DeferredChunkNode {
    type ViewQuery = (
        Entity,
        Read<ExtractedCamera>,
        Read<ViewDepthTexture>,
        Read<ViewPrepassTextures>,
    );

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_entity, camera, view_depth_texture, view_prepass_textures): QueryItem<
            'w,
            Self::ViewQuery,
        >,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let phases = world.resource::<ViewSortedRenderPhases<DeferredBatch3d>>();

        let Some(phase) = phases.get(&view_entity) else {
            return Ok(());
        };

        let diagnostics = render_context.diagnostic_recorder();

        let mut color_attachments = vec![
            // Normals
            view_prepass_textures
                .normal
                .as_ref()
                .map(ColorAttachment::get_attachment),
            // Motion vectors
            view_prepass_textures
                .motion_vectors
                .as_ref()
                .map(ColorAttachment::get_attachment),
            // Deferred
            view_prepass_textures
                .deferred
                .as_ref()
                .map(ColorAttachment::get_attachment),
            // Lighting pass ID
            view_prepass_textures
                .deferred_lighting_pass_id
                .as_ref()
                .map(ColorAttachment::get_attachment),
        ];

        // If all color attachments are none clear the list so that no fragment shader is required
        if color_attachments.iter().all(Option::is_none) {
            color_attachments.clear();
        }

        let depth_stencil_attachment = Some(view_depth_texture.get_attachment(StoreOp::Store));

        let view_entity = graph.view_entity();
        render_context.add_command_buffer_generation_task(move |gpu| {
            let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("chunk_deferred_render_cmd_encoder"),
            });

            let pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("chunk_deferred_render"),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut pass = TrackedRenderPass::new(&gpu, pass);
            let pass_span = diagnostics.pass_span(&mut pass, "chunk_render");

            if let Some(viewport) = camera.viewport.as_ref() {
                pass.set_camera_viewport(viewport);
            }

            phase.render(&mut pass, world, view_entity);

            pass_span.end(&mut pass);
            drop(pass);

            // After rendering to the view depth texture, copy it to the prepass depth texture
            if let Some(prepass_depth_texture) = &view_prepass_textures.depth {
                encoder.copy_texture_to_texture(
                    view_depth_texture.texture.as_image_copy(),
                    prepass_depth_texture.texture.texture.as_image_copy(),
                    view_prepass_textures.size,
                );
            }

            encoder.finish()
        });

        Ok(())
    }
}

pub fn get_batch_preprocess_pipeline(world: &World) -> Option<&ComputePipeline> {
    let pipeline_cache = world.resource::<PipelineCache>();
    let pipeline_id = world.resource::<ViewBatchPreprocessPipelineId>();

    pipeline_cache.get_compute_pipeline(pipeline_id.0)
}

pub fn get_light_batch_preprocess_pipeline(world: &World) -> Option<&ComputePipeline> {
    let pipeline_cache = world.resource::<PipelineCache>();
    let pipeline_id = world.resource::<ViewBatchLightPreprocessPipelineId>();

    pipeline_cache.get_compute_pipeline(pipeline_id.0)
}

pub fn begin_batch_preprocess_compute_pass<'a>(encoder: &'a mut CommandEncoder) -> ComputePass<'a> {
    encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("chunk_batch_preprocess_compute_pass"),
        timestamp_writes: None,
    })
}

pub fn begin_light_batch_preprocess_compute_pass<'a>(
    encoder: &'a mut CommandEncoder,
) -> ComputePass<'a> {
    encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("chunk_light_batch_preprocess_compute_pass"),
        timestamp_writes: None,
    })
}

pub fn clear_count_buffer(encoder: &mut CommandEncoder, count_buffer: &Buffer) {
    encoder.clear_buffer(count_buffer, 0, Some(u32::SHADER_SIZE.into()));
}

pub fn create_batch_data_bind_groups<'a>(
    gpu: &RenderDevice,
    provider: &BindGroupProvider,
    batches: impl Iterator<Item = (&'a Entity, &'a IndirectViewBatch)>,
) -> EntityHashMap<BindGroup> {
    let mut bind_groups = EntityHashMap::default();

    for (&entity, gpu_data) in batches {
        let bind_group = provider.preprocess_batch_data(gpu, gpu_data);
        bind_groups.insert(entity, bind_group);
    }

    bind_groups
}

/////////////////////////////////////////////////////////////////////////////////////////////////////////////
// View preprocessing

pub struct PreprocessBatchesNode {
    q_batches: QueryState<Read<ChunkBatchLod>>,
}

impl PreprocessBatchesNode {
    pub fn get_batch_lod(&self, world: &World, entity: Entity) -> Option<LevelOfDetail> {
        match self.q_batches.get_manual(world, entity) {
            Ok(lod) => Some(lod.0),
            Err(error) => {
                error!("Error getting LOD component for batch {entity}: {error}");
                None
            }
        }
    }
}

impl FromWorld for PreprocessBatchesNode {
    fn from_world(world: &mut World) -> Self {
        Self {
            q_batches: QueryState::from_world(world),
        }
    }
}

impl ViewNode for PreprocessBatchesNode {
    type ViewQuery = (Entity, Read<VisibleBatches>, Read<ViewUniformOffset>);

    fn update(&mut self, world: &mut World) {
        self.q_batches.update_archetypes(world);
    }

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        (view_entity, visible_batches, view_uniform_offset): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let bg_provider = world.resource::<BindGroupProvider>();
        let view_batches_store = world.resource::<ViewBatchBuffersStore>();
        let chunk_mesh_store = world.resource::<IndirectRenderDataStore>();
        let uniforms = world.resource::<ViewUniforms>();
        let gpu = world.resource::<RenderDevice>();

        let Some(view_uniforms_binding) = uniforms.uniforms.binding() else {
            return Ok(());
        };

        let Some(preprocess_pipeline) = get_batch_preprocess_pipeline(world) else {
            error!("Couldn't get batch preprocessing pipeline");
            return Ok(());
        };

        let Some(view_batches) = view_batches_store.get_batches(view_entity) else {
            return Ok(());
        };

        let view_bind_group = bg_provider.preprocess_view(gpu, view_uniforms_binding);
        let batch_data_bind_groups =
            create_batch_data_bind_groups(gpu, bg_provider, view_batches.iter());

        let command_encoder = ctx.command_encoder();

        // Clear the count buffers
        for buf in view_batches.values().map(|d| &d.count_buffer) {
            clear_count_buffer(command_encoder, buf);
        }

        let mut pass = begin_batch_preprocess_compute_pass(command_encoder);
        pass.set_pipeline(preprocess_pipeline);

        for (batch_entity, gpu_data) in view_batches.iter() {
            if !visible_batches.contains(batch_entity) {
                continue;
            }

            let Some(lod) = self.get_batch_lod(world, *batch_entity) else {
                error!("Can't preprocess batch entity without LOD component: {batch_entity}, view: {view_entity}");
                continue;
            };

            let Some(mesh_metadata_bind_group) =
                chunk_mesh_store.lod(lod).preprocess_metadata_bind_group()
            else {
                continue;
            };

            let batch_data_bind_group = batch_data_bind_groups.get(batch_entity).unwrap();

            pass.set_bind_group(0, &mesh_metadata_bind_group, &[]);
            pass.set_bind_group(1, &view_bind_group, &[view_uniform_offset.offset]);
            pass.set_bind_group(2, &batch_data_bind_group, &[]);

            // Divide by ceiling here, otherwise we might miss out on some chunks
            let workgroups = gpu_data
                .num_chunks
                .div_ceil(PREPROCESS_BATCH_WORKGROUP_SIZE);
            pass.dispatch_workgroups(1, 1, workgroups);
        }

        // Drop this pass since we need to borrow the encoder again.
        drop(pass);

        Ok(())
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Light preprocessing

pub struct PreprocessLightBatchesNode {
    q_batches: QueryState<Read<ChunkBatchLod>>,
    // While we don't use the LightEntity in this node, we keep it here in the query so that it only matches
    // entities that have a LightEntity component, potentially catching a few errors early
    q_lights: QueryState<(
        Read<LightEntity>,
        Read<ShadowView>,
        Option<Read<VisibleBatches>>,
    )>,
}

impl PreprocessLightBatchesNode {
    pub fn get_batch_lod(&self, world: &World, entity: Entity) -> Option<LevelOfDetail> {
        match self.q_batches.get_manual(world, entity) {
            Ok(lod) => Some(lod.0),
            Err(error) => {
                error!("Error getting LOD component for batch {entity}: {error}");
                None
            }
        }
    }
}

impl FromWorld for PreprocessLightBatchesNode {
    fn from_world(world: &mut World) -> Self {
        Self {
            q_batches: QueryState::from_world(world),
            q_lights: QueryState::from_world(world),
        }
    }
}

impl ViewNode for PreprocessLightBatchesNode {
    type ViewQuery = (Read<ViewLightEntities>, Read<ViewLightsUniformOffset>);

    fn update(&mut self, world: &mut World) {
        self.q_batches.update_archetypes(world);
        self.q_lights.update_archetypes(world);
    }

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        (view_light_entities, view_lights_uniform_offset): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        // Preprocess lights
        let gpu = world.resource::<RenderDevice>();
        let bg_provider = world.resource::<BindGroupProvider>();
        let view_batches_store = world.resource::<ViewBatchBuffersStore>();
        let chunk_mesh_store = world.resource::<IndirectRenderDataStore>();
        let light_meta = world.resource::<LightMeta>();
        let hzb_cache = world.resource::<HzbCache>();

        let Some(preprocess_pipeline) = get_light_batch_preprocess_pipeline(world) else {
            error!("Couldn't get light batch preprocessing pipeline");
            return Ok(());
        };

        let Some(view_light_uniforms_binding) = light_meta.view_gpu_lights.binding() else {
            return Ok(());
        };

        let command_encoder = ctx.command_encoder();

        for &view_light in &view_light_entities.lights {
            // Attempt to get the components from this view light
            let (
                // Unused, just used for filtering
                _light_entity,
                shadow_view,
                visible_batches,
            ) = match self.q_lights.get_manual(world, view_light) {
                Ok(query_result) => query_result,
                Err(error) => {
                    error!("Could not get components from light entity {view_light}: {error}");
                    continue;
                }
            };

            let Some(visible_batches) = visible_batches else {
                continue;
            };

            let hzb_view = hzb_cache
                .get_view_hzb(view_light)
                .unwrap()
                .mip_level_view(0)
                .unwrap();

            // Bind group for the light's view
            let light_view_bind_group = bg_provider.preprocess_light_view(
                gpu,
                view_light_uniforms_binding.clone(),
                hzb_view,
            );

            let Some(view_batches) = view_batches_store.get_batches(view_light) else {
                continue;
            };

            let batch_bind_groups =
                create_batch_data_bind_groups(gpu, bg_provider, view_batches.iter());

            // Clear the count buffers
            for buf in view_batches.values().map(|d| &d.count_buffer) {
                clear_count_buffer(command_encoder, buf);
            }

            let mut pass = begin_light_batch_preprocess_compute_pass(command_encoder);
            pass.set_pipeline(preprocess_pipeline);

            for (batch_entity, gpu_data) in view_batches.iter() {
                if !visible_batches.contains(batch_entity) {
                    continue;
                }

                let Some(lod) = self.get_batch_lod(world, *batch_entity) else {
                    error!("Can't preprocess batch entity without LOD component: {batch_entity}, light view: {view_light}");
                    continue;
                };

                let Some(mesh_metadata_bind_group) =
                    chunk_mesh_store.lod(lod).preprocess_metadata_bind_group()
                else {
                    continue;
                };

                // Get the bind group for the batch in the light's view this time around
                let batch_data_bind_group = batch_bind_groups.get(batch_entity).unwrap();

                pass.set_bind_group(0, &mesh_metadata_bind_group, &[]);
                // Uniform offset for the lights
                pass.set_bind_group(
                    1,
                    &light_view_bind_group,
                    &[view_lights_uniform_offset.offset],
                );
                pass.set_bind_group(2, &batch_data_bind_group, &[]);

                // Divide by ceiling here, otherwise we might miss out on some chunks
                let workgroups = gpu_data
                    .num_chunks
                    .div_ceil(PREPROCESS_BATCH_WORKGROUP_SIZE);
                pass.dispatch_workgroups(1, 1, workgroups);
            }
        }

        Ok(())
    }
}
