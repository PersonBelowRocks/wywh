use std::sync::atomic::Ordering;

use bevy::{
    core_pipeline::prepass::ViewPrepassTextures,
    ecs::{
        query::QueryItem,
        system::lifetimeless::{Read, SResMut},
    },
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
        render_phase::{BinnedPhaseItem, TrackedRenderPass, ViewSortedRenderPhases},
        render_resource::{
            BindGroupEntries, BufferInitDescriptor, BufferUsages, CommandEncoderDescriptor,
            ComputePassDescriptor, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            StoreOp,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        view::{ViewDepthTexture, ViewTarget, ViewUniformOffset},
    },
};
use bytemuck::cast_slice;

use crate::render::{ChunkBatch, ObserverBatches};

use super::{
    chunk_batches::{PopulateBatchBuffers, PopulateBatchBuffersPipelineId, RenderChunkBatches},
    gpu_chunk::IndirectRenderDataStore,
    phase::{PrepassChunkPhaseItem, RenderChunkPhaseItem},
    DefaultBindGroupLayouts,
};

pub struct CoreGraphPlugin;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, RenderLabel)]
pub enum Nodes {
    Prepass,
    MainPass,
}

fn color_attachments(
    prepass_textures: &ViewPrepassTextures,
) -> Vec<Option<RenderPassColorAttachment>> {
    let mut color_attachments = vec![
        prepass_textures
            .normal
            .as_ref()
            .map(|normals_texture| normals_texture.get_attachment()),
        prepass_textures
            .motion_vectors
            .as_ref()
            .map(|motion_vectors_texture| motion_vectors_texture.get_attachment()),
        // Use None in place of deferred attachments
        None,
        None,
    ];

    // If all color attachments are none: clear the color attachment list so that no fragment shader is required
    if color_attachments.iter().all(Option::is_none) {
        color_attachments.clear();
    }

    color_attachments
}

#[derive(Default)]
pub struct ChunkPrepassNode;

impl ViewNode for ChunkPrepassNode {
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
        let phases = world.resource::<ViewSortedRenderPhases<PrepassChunkPhaseItem>>();
        let Some(phase) = phases.get(&view_entity) else {
            return Ok(());
        };

        let diagnostics = render_context.diagnostic_recorder();

        let color_attachments = color_attachments(&view_prepass_textures);
        let depth_stencil_attachment = Some(view_depth_texture.get_attachment(StoreOp::Store));

        let view_entity = graph.view_entity();
        render_context.add_command_buffer_generation_task(move |gpu| {
            let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("chunk_prepass_cmd_encoder"),
            });

            let pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("chunk_prepass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut pass = TrackedRenderPass::new(&gpu, pass);
            let pass_span = diagnostics.pass_span(&mut pass, "chunk_prepass");

            if let Some(viewport) = camera.viewport.as_ref() {
                pass.set_camera_viewport(viewport);
            }

            phase.render(&mut pass, world, view_entity);

            pass_span.end(&mut pass);
            drop(pass);

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

pub struct ChunkRenderNode;

impl ViewNode for ChunkRenderNode {
    type ViewQuery = (
        Entity,
        Read<ExtractedCamera>,
        Read<ViewTarget>,
        Read<ViewDepthTexture>,
    );

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_entity, camera, view_target, view_depth_texture): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let phases = world.resource::<ViewSortedRenderPhases<RenderChunkPhaseItem>>();

        let Some(phase) = phases.get(&view_entity) else {
            return Ok(());
        };

        let diagnostics = render_context.diagnostic_recorder();

        let color_attachments = [Some(view_target.get_color_attachment())];
        let depth_stencil_attachment = Some(view_depth_texture.get_attachment(StoreOp::Store));

        let view_entity = graph.view_entity();
        render_context.add_command_buffer_generation_task(move |gpu| {
            let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("chunk_render_cmd_encoder"),
            });

            let pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("chunk_render"),
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
            encoder.finish()
        });

        Ok(())
    }
}

#[derive(Component, Copy, Clone, Debug)]
pub struct PopulateObserverBuffers;

pub struct BuildBatchBuffersNode {
    query: QueryState<Read<ChunkBatch>>,
}

impl Node for BuildBatchBuffersNode {
    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world);
    }

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let gpu = world.resource::<RenderDevice>();
        let queue = world.resource::<RenderQueue>();
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();
        let indirect_data = world.resource::<IndirectRenderDataStore>();
        let pipeline_id = world.resource::<PopulateBatchBuffersPipelineId>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let render_chunk_batches = world.resource::<RenderChunkBatches>();
        let populate_batches = world.resource::<PopulateBatchBuffers>();

        // Return early if there's no batches whose buffers need populating
        if populate_batches.is_empty() {
            return Ok(());
        }

        // Encode compute pass
        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("populate_batch_buffers_encoder"),
        });

        let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline_id.0) else {
            error!("Cannot get batch buffer population compute pipeline");
            return Ok(());
        };

        for &batch_entity in populate_batches.iter() {
            let Ok(batch) = self.query.get_manual(world, batch_entity) else {
                continue;
            };

            // Skip if there's no chunks
            if batch.chunks.is_empty() {
                continue;
            }

            // Skip all batches that don't have initialized buffers. We are not allowed to initialize the buffers here due to mutability
            // rules so we are forced to just do whatever the previous render stages tell us.
            let Some(render_batch) = render_chunk_batches.get(batch_entity) else {
                continue;
            };

            let Some(buffers) = &render_batch.buffers else {
                continue;
            };

            let num_chunks = batch.chunks.len();

            // An array of the indices to the chunk metadata on the GPU.
            let chunk_metadata_indices = batch.get_metadata_indices(&indirect_data.chunks);
            let metadata_index_buffer = gpu.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("BBB_chunk_metadata_indices_buffer"),
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                contents: cast_slice(&chunk_metadata_indices),
            });

            let metadata_buffer = &indirect_data.chunks.buffers().metadata;

            // Build bind groups
            // This bind group is for all the data we want to read from.
            let input_bg = gpu.create_bind_group(
                Some("BBB_input_bind_group"),
                &default_layouts.observer_buffers_input_layout,
                &BindGroupEntries::sequential((
                    metadata_buffer.as_entire_binding(),
                    metadata_index_buffer.as_entire_binding(),
                )),
            );

            // This bind group has the buffers that we want to populate.
            let output_bg = gpu.create_bind_group(
                Some("BBB_output_bind_group"),
                &default_layouts.observer_buffers_output_layout,
                &BindGroupEntries::sequential((
                    buffers.instance.as_entire_binding(),
                    buffers.indirect.as_entire_binding(),
                    buffers.count.as_entire_binding(),
                )),
            );

            {
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("BBB_compute_pass"),
                    timestamp_writes: None,
                });

                pass.set_pipeline(&compute_pipeline);

                pass.set_bind_group(0, &input_bg, &[]);
                pass.set_bind_group(1, &output_bg, &[]);

                pass.dispatch_workgroups(1, 1, num_chunks as u32);
            }
        }

        queue.submit([encoder.finish()]);

        Ok(())
    }
}
