use bevy::{
    core_pipeline::prepass::ViewPrepassTextures,
    ecs::{
        entity::{EntityHash, EntityHashSet},
        query::QueryItem,
        system::lifetimeless::Read,
    },
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
        render_phase::{TrackedRenderPass, ViewSortedRenderPhases},
        render_resource::{
            CommandEncoderDescriptor, ComputePassDescriptor, PipelineCache, RenderPassDescriptor,
            ShaderSize, StoreOp,
        },
        renderer::RenderContext,
        texture::ColorAttachment,
        view::{ViewDepthTexture, ViewUniformOffset},
    },
};

use crate::topo::controller::VisibleBatches;

use super::{
    chunk_batches::{PopulateBatchBuffers, PreparedChunkBatches},
    indirect::IndexedIndirectArgs,
    phase::DeferredBatchPrepass,
    pipelines::{
        BuildBatchBuffersPipelineId, ObserverBatchFrustumCullPipelineId,
        BUILD_BATCH_BUFFERS_WORKGROUP_SIZE, FRUSTUM_CULL_WORKGROUP_SIZE,
    },
    views::ObserverBatchBuffersStore,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, RenderLabel)]
pub enum Nodes {
    BuildBatchBuffers,
    BatchFrustumCulling,
    Prepass,
}

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
        let phases = world.resource::<ViewSortedRenderPhases<DeferredBatchPrepass>>();

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

#[derive(Default)]
pub struct BuildBatchBuffersNode;

impl Node for BuildBatchBuffersNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let pipeline_id = world.resource::<BuildBatchBuffersPipelineId>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let render_chunk_batches = world.resource::<PreparedChunkBatches>();
        let populate_batches = world.resource::<PopulateBatchBuffers>();
        let observer_batch_buf_store = world.resource::<ObserverBatchBuffersStore>();

        // Return early if there's no batches whose buffers need populating
        if populate_batches.is_empty() {
            return Ok(());
        }

        let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline_id.0) else {
            error!("Cannot get batch buffer population compute pipeline");
            return Ok(());
        };

        let mut built = EntityHashSet::with_capacity_and_hasher(
            populate_batches.batches.len(),
            EntityHash::default(),
        );

        // Encode compute pass
        let mut pass = ctx
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("BBB_compute_pass"),
                timestamp_writes: None,
            });

        pass.set_pipeline(&compute_pipeline);

        // Build all the initial batch buffers
        for (&batch_entity, bbb_bind_group) in populate_batches.batches.iter() {
            // Skip all batches that don't have initialized buffers. We are not allowed to initialize the buffers here due to mutability
            // rules so we are forced to just do whatever the previous render stages tell us.
            let Some(render_batch) = render_chunk_batches.get(batch_entity) else {
                error!("Batch buffer was queued for building but the buffer was not initialized");
                continue;
            };

            // Skip if there's no chunks
            if render_batch.num_chunks == 0 {
                continue;
            }

            pass.set_bind_group(0, bbb_bind_group, &[]);
            // Divide by ceiling here, otherwise we might miss out on some chunks
            let workgroups = render_batch
                .num_chunks
                .div_ceil(BUILD_BATCH_BUFFERS_WORKGROUP_SIZE);

            pass.dispatch_workgroups(1, 1, workgroups);

            built.insert(batch_entity);
        }

        drop(pass);

        // Make copies of all the primary batch buffers for each observer that wants to render those batches
        for (observer, visible) in populate_batches.observers.iter() {
            let Some(observer_buffers) = observer_batch_buf_store.get(observer) else {
                error!("Queued observer did not have initialized buffers.");
                continue;
            };

            for batch_entity in visible.iter() {
                if !built.contains(batch_entity) {
                    continue;
                }

                let Some(render_batch) = render_chunk_batches.get(*batch_entity) else {
                    error!("Observer tried to get indirect batch data but the batch didn't have any buffers");
                    continue;
                };

                let Some(dst_buffers) = observer_buffers.get(batch_entity) else {
                    continue;
                };

                ctx.command_encoder().copy_buffer_to_buffer(
                    &render_batch.indirect,
                    0,
                    &dst_buffers.indirect,
                    0,
                    (render_batch.num_chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
                );
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct GpuFrustumCullBatchesNode;

impl ViewNode for GpuFrustumCullBatchesNode {
    type ViewQuery = (Entity, Read<ViewUniformOffset>, Read<VisibleBatches>);

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        ctx: &mut RenderContext<'w>,
        (view_entity, view_uniform_offset, visible_batches): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline_id = world.resource::<ObserverBatchFrustumCullPipelineId>();
        let store = world.resource::<ObserverBatchBuffersStore>();

        let Some(pipeline) = pipeline_cache.get_compute_pipeline(pipeline_id.0) else {
            error!("Couldn't get observer batch frustum cull compute pipeline");
            return Ok(());
        };

        let Some(observer_batches) = store.get(&view_entity) else {
            return Ok(());
        };

        // Clear all the count buffers (sets them to 0).
        for (_, gpu_data) in observer_batches.iter() {
            ctx.command_encoder()
                .clear_buffer(&gpu_data.count, 0, Some(u32::SHADER_SIZE.into()))
        }

        let mut pass = ctx
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("observer_batch_frustum_cull_pass"),
                timestamp_writes: None,
            });

        pass.set_pipeline(pipeline);

        for (batch_entity, gpu_data) in observer_batches.iter() {
            if !visible_batches.contains(batch_entity) {
                continue;
            }

            pass.set_bind_group(0, &gpu_data.cull_bind_group, &[view_uniform_offset.offset]);
            // Divide by ceiling here, otherwise we might miss out on some chunks
            let workgroups = gpu_data.num_chunks.div_ceil(FRUSTUM_CULL_WORKGROUP_SIZE);
            pass.dispatch_workgroups(1, 1, workgroups);
        }

        Ok(())
    }
}
