use std::ops::Range;

use bevy::{
    core_pipeline::{
        core_3d::graph::Core3d,
        prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass, ViewPrepassTextures},
    },
    ecs::{query::QueryItem, system::lifetimeless::Read},
    pbr::MeshPipelineKey,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_phase::{
            BinnedPhaseItem, BinnedRenderPhasePlugin, DrawFunctionId, DrawFunctions, PhaseItem,
            PhaseItemExtraIndex, SortedPhaseItem, SortedRenderPhasePlugin, TrackedRenderPass,
            ViewSortedRenderPhases,
        },
        render_resource::{
            CachedRenderPipelineId, CommandEncoderDescriptor, RenderPassColorAttachment,
            RenderPassDescriptor, StoreOp,
        },
        renderer::RenderContext,
        view::{ViewDepthTexture, ViewUniformOffset},
        RenderApp,
    },
};

use crate::{
    render::core::{gpu_chunk::IndirectRenderDataStore, observers::RenderWorldObservers},
    topo::controller::ObserverId,
};

use super::phase::PrepassChunkPhaseItem;

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
        Read<ObserverId>,
        Read<ExtractedCamera>,
        Read<ViewDepthTexture>,
        Read<ViewPrepassTextures>,
        Read<ViewUniformOffset>,
    );

    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (
            view_entity,
            observer,
            camera,
            view_depth_texture,
            view_prepass_textures,
            view_uniform_offset,
        ): QueryItem<'w, Self::ViewQuery>,
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
