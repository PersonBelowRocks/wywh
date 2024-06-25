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
        render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
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

use crate::topo::controller::ObserverId;

use super::{
    gpu_chunk::IndirectRenderDataStore,
    observers::{PopulateObserverBuffersPipelineId, RenderWorldObservers},
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

#[derive(Default)]
pub struct ObserverBufferBuilderNode;

impl ViewNode for ObserverBufferBuilderNode {
    type ViewQuery = (Entity, Read<ObserverId>, Has<PopulateObserverBuffers>);

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (entity, observer_id, should_populate_buffers): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        if !should_populate_buffers {
            return Ok(());
        }

        let gpu = world.resource::<RenderDevice>();
        let queue = world.resource::<RenderQueue>();
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();
        let observers = world.resource::<RenderWorldObservers>();
        let indirect_data = world.resource::<IndirectRenderDataStore>();
        let pipeline_id = world.resource::<PopulateObserverBuffersPipelineId>();
        let pipeline_cache = world.resource::<PipelineCache>();

        // Skip this observer if it's not present in the global map.
        let Some(observer_lods) = observers.get(observer_id) else {
            return Ok(());
        };

        let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline_id.0) else {
            error!("Cannot get buffer population compute pipeline");
            return Ok(());
        };

        // Populate each LOD individually
        for (lod, batch) in observer_lods.iter() {
            let Some(ref buffers) = batch.buffers else {
                continue;
            };

            // If the buffers are already populated then we bail
            if buffers.ready.load(Ordering::Relaxed) {
                continue;
            }

            // Skip if there's no chunks
            if batch.chunks.is_empty() {
                continue;
            }

            let num_chunks = batch.chunks.len();
            let chunk_metadata_indices = batch.get_metadata_indices(&indirect_data.chunks);

            let metadata_index_buffer = gpu.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("observer_chunks_metadata_indices_buffer"),
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                contents: cast_slice(&chunk_metadata_indices),
            });

            let metadata_buffer = &indirect_data.chunks.buffers().metadata;

            // Build bind groups
            let input_bg = gpu.create_bind_group(
                Some("observer_population_input_bind_group"),
                &default_layouts.observer_buffers_input_layout,
                &BindGroupEntries::sequential((
                    metadata_buffer.as_entire_binding(),
                    metadata_index_buffer.as_entire_binding(),
                )),
            );

            let output_bg = gpu.create_bind_group(
                Some("observer_population_output_bind_group"),
                &default_layouts.observer_buffers_output_layout,
                &BindGroupEntries::sequential((
                    buffers.instance.as_entire_binding(),
                    buffers.indirect.as_entire_binding(),
                    buffers.count.as_entire_binding(),
                )),
            );

            // Encode compute pass
            let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("populate_observer_buffers_encoder"),
            });

            {
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("populate_observer_buffers_compute_pass"),
                    timestamp_writes: None,
                });

                pass.set_pipeline(&compute_pipeline);

                pass.set_bind_group(0, &input_bg, &[]);
                pass.set_bind_group(1, &output_bg, &[]);

                pass.dispatch_workgroups(1, 1, num_chunks as u32);
            }

            queue.submit([encoder.finish()]);
            buffers.ready.store(true, Ordering::Relaxed);
        }

        Ok(())
    }
}
