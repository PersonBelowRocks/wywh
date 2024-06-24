use bevy::{
    core_pipeline::{
        core_3d::graph::Core3d,
        prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass, ViewPrepassTextures},
    },
    ecs::{query::QueryItem, system::lifetimeless::Read},
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            CommandEncoderDescriptor, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
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

pub struct CoreGraphPlugin;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, RenderLabel)]
pub enum Nodes {
    Prepass,
    MainPass,
}

impl Plugin for CoreGraphPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app
            .add_render_graph_node::<ViewNodeRunner<ChunkPrepassNode>>(Core3d, Nodes::Prepass);

        todo!();
    }
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
            observer,
            camera,
            view_depth_texture,
            view_prepass_textures,
            view_uniform_offset
        ): QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let render_world_observers = world.resource::<RenderWorldObservers>();
        let indirect_render_data_store = world.resource::<IndirectRenderDataStore>();

        // Return early if our indirect data isn't ready to be rendered
        if !indirect_render_data_store.ready || indirect_render_data_store.bind_group.is_none() {
            return Ok(());
        }

        let Some(observer_data) = render_world_observers.get(observer) else {
            return Ok(());
        };

        let Some(ref observer_buffers) = observer_data.buffers else {
            return Ok(());
        };

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

            todo!()
        });

        Ok(())
    }
}
