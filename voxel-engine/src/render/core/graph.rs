use bevy::{
    core_pipeline::prepass::ViewPrepassTextures,
    ecs::{entity::EntityHashMap, query::QueryItem, system::lifetimeless::Read},
    pbr::ViewLightEntities,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
        render_phase::{TrackedRenderPass, ViewSortedRenderPhases},
        render_resource::{
            BindGroup, BindGroupEntries, Buffer, CommandEncoder, CommandEncoderDescriptor,
            ComputePass, ComputePassDescriptor, ComputePipeline, PipelineCache,
            RenderPassDescriptor, ShaderSize, StoreOp,
        },
        renderer::{RenderContext, RenderDevice},
        texture::ColorAttachment,
        view::{ViewDepthTexture, ViewUniformOffset, ViewUniforms},
    },
};
use hb::HashMap;

use crate::{
    render::{core::views::IndirectViewBatch, lod::LodMap},
    topo::controller::{ChunkBatchLod, VisibleBatches},
};

use super::{
    gpu_chunk::IndirectRenderDataStore,
    phase::DeferredBatch3d,
    pipelines::{ViewBatchPreprocessPipelineId, PREPROCESS_BATCH_WORKGROUP_SIZE},
    views::ViewBatchBuffersStore,
    BindGroupProvider,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, RenderLabel)]
pub enum Nodes {
    PreprocessBatches,
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

pub fn get_batch_frustum_cull_pipeline(world: &World) -> Option<&ComputePipeline> {
    let pipeline_cache = world.resource::<PipelineCache>();
    let pipeline_id = world.resource::<ViewBatchPreprocessPipelineId>();

    pipeline_cache.get_compute_pipeline(pipeline_id.0)
}

pub fn begin_frustum_cull_compute_pass<'a>(encoder: &'a mut CommandEncoder) -> ComputePass<'a> {
    encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("chunk_batch_frustum_cull_compute_pass"),
        timestamp_writes: None,
    })
}

pub fn clear_count_buffer(encoder: &mut CommandEncoder, count_buffer: &Buffer) {
    encoder.clear_buffer(count_buffer, 0, Some(u32::SHADER_SIZE.into()));
}

pub fn clear_count_buffers<'a>(
    encoder: &mut CommandEncoder,
    buffers: impl Iterator<Item = &'a Buffer>,
) {
    for buffer in buffers {
        clear_count_buffer(encoder, buffer);
    }
}

pub struct PreprocessViewBatchesNode {
    q_batches: QueryState<Read<ChunkBatchLod>>,
}

impl FromWorld for PreprocessViewBatchesNode {
    fn from_world(world: &mut World) -> Self {
        Self {
            q_batches: QueryState::from_world(world),
        }
    }
}

impl ViewNode for PreprocessViewBatchesNode {
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

        let Some(frustum_cull_pipeline) = get_batch_frustum_cull_pipeline(world) else {
            error!("Couldn't get view batch frustum cull compute pipeline");
            return Ok(());
        };

        let Some(view_batches) = view_batches_store.get_batches(view_entity) else {
            return Ok(());
        };

        let view_bind_group = bg_provider.preprocess_view(gpu, view_uniforms_binding);
        // TODO: dont create these here, we can create/recreate them every time chunk meshes are uploaded
        let mut mesh_metadata_bind_groups = LodMap::<BindGroup>::new();
        let mut batch_data_bind_groups = EntityHashMap::<BindGroup>::default();

        for (batch_entity, gpu_data) in view_batches.iter() {
            if !visible_batches.contains(batch_entity) {
                continue;
            }

            let Some(lod) = self.q_batches.get_manual(&world, *batch_entity).ok() else {
                error!("Can't preprocess batch entity without LOD component: {batch_entity}, view: {view_entity}");
                continue;
            };

            if !mesh_metadata_bind_groups.contains(lod.0) {
                let icd = chunk_mesh_store.lod(lod.0);
                let mesh_metadata_bind_group = bg_provider.preprocess_mesh_metadata(gpu, icd);

                mesh_metadata_bind_groups.insert(lod.0, mesh_metadata_bind_group);
            }

            let batch_data_bind_group = bg_provider.preprocess_batch_data(gpu, gpu_data);
            batch_data_bind_groups.insert(*batch_entity, batch_data_bind_group);
        }

        let count_buffers = view_batches.values().map(|d| &d.count_buffer);

        let command_encoder = ctx.command_encoder();

        clear_count_buffers(command_encoder, count_buffers);
        let mut pass = begin_frustum_cull_compute_pass(command_encoder);

        // TODO: the shaders
        pass.set_pipeline(frustum_cull_pipeline);

        for (batch_entity, gpu_data) in view_batches.iter() {
            if !visible_batches.contains(batch_entity) {
                continue;
            }

            let Some(lod) = self.q_batches.get_manual(&world, *batch_entity).ok() else {
                error!("Can't preprocess batch entity without LOD component: {batch_entity}, view: {view_entity}");
                continue;
            };

            let mesh_metadata_bind_group = mesh_metadata_bind_groups.get(lod.0).unwrap();
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

        drop(pass);
        drop(mesh_metadata_bind_groups);

        Ok(())
    }
}

pub fn get_view_lights_gpu_data<'a, 'b: 'a>(
    view_light_entities: impl IntoIterator<Item = &'a Entity> + 'b,
    store: &'b ViewBatchBuffersStore,
) -> impl Iterator<Item = &'b IndirectViewBatch> + 'a {
    view_light_entities
        .into_iter()
        .filter_map(|&light_entity| store.get_batches(light_entity))
        .flat_map(HashMap::values)
}

// #[derive(Default)]
// pub struct FrustumCullLightBatchesNode;

// impl ViewNode for FrustumCullLightBatchesNode {
//     type ViewQuery = Read<ViewLightEntities>;

//     fn run<'w>(
//         &self,
//         _graph: &mut RenderGraphContext,
//         ctx: &mut RenderContext<'w>,
//         view_light_entities: QueryItem<'w, Self::ViewQuery>,
//         world: &'w World,
//     ) -> Result<(), NodeRunError> {
//         let command_encoder = ctx.command_encoder();
//         let store = world.resource::<ViewBatchBuffersStore>();

//         let Some(frustum_cull_pipeline) = get_batch_frustum_cull_pipeline(world) else {
//             error!("Couldn't get view batch frustum cull compute pipeline");
//             return Ok(());
//         };

//         // Need to do this before we make a compute pass due to lifetimes.
//         for cull_data in get_view_lights_cull_data(&view_light_entities.lights, store) {
//             clear_count_buffer(command_encoder, &cull_data.count);
//         }

//         let mut pass = begin_frustum_cull_compute_pass(command_encoder);
//         pass.set_pipeline(frustum_cull_pipeline);

//         for gpu_data in get_view_lights_gpu_data(&view_light_entities.lights, store) {
//             let Some(cull_data) = gpu_data.cull_data() else {
//                 continue;
//             };

//             pass.set_bind_group(0, &cull_data.bind_group, &[cull_data.uniform_offset]);
//             // Divide by ceiling here, otherwise we might miss out on some chunks
//             let workgroups = gpu_data.num_chunks.div_ceil(FRUSTUM_CULL_WORKGROUP_SIZE);
//             pass.dispatch_workgroups(1, 1, workgroups);
//         }

//         Ok(())
//     }
// }
