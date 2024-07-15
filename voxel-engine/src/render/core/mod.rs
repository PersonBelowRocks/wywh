mod chunk_batches;
mod commands;
mod gpu_chunk;
mod gpu_registries;
mod graph;
mod indirect;
mod lights;
mod phase;
mod pipelines;
mod queue;
mod shaders;
mod utils;
mod views;

use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::pbr::graph::NodePbr;
use bevy::pbr::Shadow;
use bevy::render::render_graph::{RenderGraphApp, ViewNodeRunner};
use bevy::render::render_phase::{DrawFunctions, ViewSortedRenderPhases};
use bevy::render::render_resource::{BindGroup, BindGroupEntries, BindingResource, ShaderSize};
use bevy::render::view::ViewUniform;
use bevy::{
    app::{App, Plugin},
    ecs::system::Resource,
    prelude::*,
    render::{
        extract_resource::ExtractResourcePlugin,
        render_phase::AddRenderCommand,
        render_resource::{
            binding_types, BindGroupLayout, BindGroupLayoutEntries, SamplerBindingType,
            ShaderStages, SpecializedComputePipelines, SpecializedRenderPipelines,
            TextureSampleType,
        },
        renderer::RenderDevice,
        Render, RenderApp, RenderSet,
    },
};
use cb::channel::Receiver;
use chunk_batches::{extract_batches_with_lods, initialize_and_queue_batch_buffers};
use commands::DrawDeferredBatch;
use gpu_chunk::{
    update_gpu_mesh_data, update_indirect_mesh_data_dependants, IndirectRenderDataStore,
    RemoveChunkMeshes, UpdateIndirectLODs,
};
use graph::{DeferredChunkNode, Nodes, PreprocessViewBatchesNode};
use indirect::{ChunkInstanceData, GpuChunkMetadata, IndexedIndirectArgs, IndirectChunkData};
use lights::{
    inherit_parent_light_batches, initialize_and_queue_light_batch_buffers, queue_chunk_shadows,
};
use phase::DeferredBatch3d;
use pipelines::{create_pipelines, DeferredIndirectChunkPipeline, ViewBatchPreprocessPipeline};
use queue::queue_deferred_chunks;
use shaders::load_internal_shaders;
use utils::InspectChunks;
use views::{
    extract_chunk_camera_phases, extract_visible_batches, IndirectViewBatch, ViewBatchBuffersStore,
};

use crate::data::{
    systems::{VoxelColorArrayTexture, VoxelNormalArrayTexture},
    texture::GpuFaceTexture,
};
use crate::render::lod::LevelOfDetail;
use crate::topo::world::ChunkPos;
use crate::VoxelPlugin;

use self::{
    gpu_chunk::{extract_chunk_mesh_data, AddChunkMeshes},
    gpu_registries::{
        extract_texreg_faces, prepare_gpu_registry_data, ExtractedTexregFaces, RegistryBindGroup,
    },
    utils::main_world_res_exists,
};

use super::{meshing::controller::ExtractableChunkMeshData, quad::GpuQuad};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum CoreSet {
    Extract,
    ManageViews,
    PrepareRegistryData,
    PrepareIndirectMeshData,
    UpdateIndirectMeshDataDependants,
    PrepareIndirectBuffers,
    Queue,
}

#[derive(Clone, Resource)]
pub struct RenderCoreDebug {
    pub clear_inpsection: Receiver<()>,
    pub inspect_chunks: Receiver<ChunkPos>,
}

#[derive(Default)]
pub struct RenderCore {
    pub debug: Option<RenderCoreDebug>,
}

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        info!("Initializing render core");

        load_internal_shaders(app);

        app.add_plugins((
            ExtractResourcePlugin::<VoxelColorArrayTexture>::default(),
            ExtractResourcePlugin::<VoxelNormalArrayTexture>::default(),
        ));

        // Render app logic
        let render_app = app.sub_app_mut(RenderApp);

        render_app.configure_sets(
            Render,
            (
                (
                    CoreSet::PrepareIndirectMeshData,
                    CoreSet::UpdateIndirectMeshDataDependants,
                    CoreSet::PrepareIndirectBuffers,
                )
                    .chain()
                    .in_set(RenderSet::Prepare)
                    // Systems in these sets depend on bevy's own internal manipulation of the world. Notable
                    // bevy adds the "ViewUniformOffset" component to views in the PrepareResources set and we
                    // need access to that offset when we initialize and build buffers for the visible batches in a view.
                    .after(RenderSet::PrepareResources),
                CoreSet::PrepareRegistryData.in_set(RenderSet::Prepare),
                CoreSet::Queue.in_set(RenderSet::Queue),
                CoreSet::ManageViews
                    .after(RenderSet::ManageViews)
                    .before(RenderSet::Queue),
            ),
        );

        render_app
            // Draw functions
            .init_resource::<DrawFunctions<DeferredBatch3d>>()
            // Render phases
            .init_resource::<ViewSortedRenderPhases<DeferredBatch3d>>()
            // Pipeline stores
            .init_resource::<SpecializedRenderPipelines<DeferredIndirectChunkPipeline>>()
            .init_resource::<SpecializedComputePipelines<ViewBatchPreprocessPipeline>>()
            // Misc
            .init_resource::<InspectChunks>()
            .init_resource::<ViewBatchBuffersStore>()
            .init_resource::<UpdateIndirectLODs>()
            .init_resource::<RemoveChunkMeshes>()
            .init_resource::<AddChunkMeshes>();

        render_app
            .add_render_command::<DeferredBatch3d, DrawDeferredBatch>()
            .add_render_command::<Shadow, DrawDeferredBatch>()
            .add_render_graph_node::<ViewNodeRunner<DeferredChunkNode>>(Core3d, Nodes::Prepass)
            .add_render_graph_node::<ViewNodeRunner<PreprocessViewBatchesNode>>(
                Core3d,
                Nodes::BatchFrustumCulling,
            )
            .add_render_graph_edges(
                Core3d,
                (
                    Nodes::BuildBatchBuffers,
                    Nodes::BatchFrustumCulling,
                    Node3d::Prepass,
                    Nodes::Prepass,
                ),
            )
            .add_render_graph_edges(
                Core3d,
                (
                    Nodes::BuildBatchBuffers,
                    Nodes::LightBatchFrustumCulling,
                    NodePbr::ShadowPass,
                ),
            );

        if let Some(debug) = self.debug.clone() {
            info!("Setting up render core inspection");
            render_app.insert_resource(debug);
            render_app.add_systems(ExtractSchedule, set_inspection.before(CoreSet::Extract));
        }

        render_app.add_systems(
            ExtractSchedule,
            (
                (
                    extract_batches_with_lods,
                    // We have to insert apply_deferred here manually, not sure why bevy doesn't do it automatically.
                    apply_deferred,
                    extract_visible_batches,
                )
                    .chain(),
                extract_chunk_camera_phases,
                extract_texreg_faces.run_if(not(resource_exists::<ExtractedTexregFaces>)),
                extract_chunk_mesh_data.run_if(main_world_res_exists::<ExtractableChunkMeshData>),
            )
                .in_set(CoreSet::Extract),
        );

        render_app.add_systems(
            Render,
            (
                inherit_parent_light_batches.in_set(CoreSet::ManageViews),
                // We only need to create the compute pipelines once
                create_pipelines
                    .run_if(run_once())
                    .in_set(RenderSet::Prepare),
                // This system creates the RegistryBindGroup resource if it runs successfully, and if
                // it runs successfully we don't need to run it again (registry data can't change during runtime).
                prepare_gpu_registry_data
                    .run_if(not(resource_exists::<RegistryBindGroup>))
                    .in_set(CoreSet::PrepareRegistryData),
                // Here we prepare the index and instance buffers for the chunks.
                update_gpu_mesh_data.in_set(CoreSet::PrepareIndirectMeshData),
                // This updates all the data that depends on the state of the index and instance buffers,
                // which is mainly the indirect buffers
                update_indirect_mesh_data_dependants
                    .in_set(CoreSet::UpdateIndirectMeshDataDependants),
                // Prepare the indirect buffers.
                (
                    initialize_and_queue_batch_buffers,
                    initialize_and_queue_light_batch_buffers,
                )
                    .chain()
                    .in_set(CoreSet::PrepareIndirectBuffers),
                // Queue the chunks
                (queue_deferred_chunks, queue_chunk_shadows).in_set(CoreSet::Queue),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app
            .init_resource::<BindGroupProvider>()
            .init_resource::<IndirectRenderDataStore>()
            .init_resource::<DeferredIndirectChunkPipeline>()
            .init_resource::<ViewBatchPreprocessPipeline>();
    }
}

fn set_inspection(
    debug: Res<RenderCoreDebug>,
    mut inspections: ResMut<InspectChunks>,
    icd_store: Res<IndirectRenderDataStore>,
) {
    let mut clear = false;
    while let Ok(_) = debug.clear_inpsection.try_recv() {
        clear = true;
    }

    if clear {
        inspections.clear();
        info!("Cleared all current inspections");
    }

    while let Ok(chunk_pos) = debug.inspect_chunks.try_recv() {
        info!("Inspecting chunk {chunk_pos}");
        inspections.set(chunk_pos);

        for lod in LevelOfDetail::LODS {
            let Some(metadata) = icd_store.lod(lod).get_chunk_metadata(chunk_pos) else {
                continue;
            };

            info!("Metadata for {chunk_pos} at LOD {lod:?}: {metadata:#?}");
        }
    }
}

#[derive(Resource, Clone)]
pub(crate) struct BindGroupProvider {
    pub registry_bg_layout: BindGroupLayout,
    pub icd_quad_bg_layout: BindGroupLayout,
    pub preprocess_mesh_metadata_bg_layout: BindGroupLayout,
    pub preprocess_batch_data_bg_layout: BindGroupLayout,
    pub preprocess_view_bg_layout: BindGroupLayout,
}

impl FromWorld for BindGroupProvider {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        Self {
            registry_bg_layout: gpu.create_bind_group_layout(
                Some("registry_bind_group_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    (
                        binding_types::storage_buffer_read_only::<GpuFaceTexture>(false),
                        binding_types::texture_2d_array(TextureSampleType::default()),
                        binding_types::sampler(SamplerBindingType::NonFiltering),
                        binding_types::texture_2d_array(TextureSampleType::default()),
                        binding_types::sampler(SamplerBindingType::NonFiltering),
                    ),
                ),
            ),
            icd_quad_bg_layout: gpu.create_bind_group_layout(
                Some("ICD_quad_bind_group_layout"),
                &BindGroupLayoutEntries::single(
                    ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    binding_types::storage_buffer_read_only::<GpuQuad>(false),
                ),
            ),
            preprocess_mesh_metadata_bg_layout: gpu.create_bind_group_layout(
                Some("preprocess_mesh_metadata_bg_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::COMPUTE,
                    (
                        binding_types::storage_buffer_read_only::<GpuChunkMetadata>(false),
                        binding_types::storage_buffer_read_only::<ChunkInstanceData>(false),
                    ),
                ),
            ),
            preprocess_batch_data_bg_layout: gpu.create_bind_group_layout(
                Some("preprocess_batch_data_bg_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::COMPUTE,
                    (
                        binding_types::storage_buffer_read_only::<u32>(false),
                        binding_types::storage_buffer::<IndexedIndirectArgs>(false),
                        binding_types::storage_buffer_sized(false, Some(u32::SHADER_SIZE)),
                    ),
                ),
            ),
            preprocess_view_bg_layout: gpu.create_bind_group_layout(
                Some("preprocess_view_bg_layout"),
                &BindGroupLayoutEntries::single(
                    ShaderStages::COMPUTE,
                    binding_types::uniform_buffer::<ViewUniform>(true),
                ),
            ),
        }
    }
}

impl BindGroupProvider {
    pub fn preprocess_view(
        &self,
        gpu: &RenderDevice,
        view_uniforms_binding: BindingResource,
    ) -> BindGroup {
        gpu.create_bind_group(
            Some("preprocess_view_bind_group"),
            &self.preprocess_view_bg_layout,
            &BindGroupEntries::single(view_uniforms_binding),
        )
    }

    pub fn preprocess_mesh_metadata(
        &self,
        gpu: &RenderDevice,
        icd: &IndirectChunkData,
    ) -> BindGroup {
        gpu.create_bind_group(
            Some("preprocess_mesh_metadata_bind_group"),
            &self.preprocess_mesh_metadata_bg_layout,
            &BindGroupEntries::sequential((
                icd.metadata_buffer().as_entire_binding(),
                icd.index_buffer().as_entire_binding(),
            )),
        )
    }

    pub fn preprocess_batch_data(
        &self,
        gpu: &RenderDevice,
        gpu_data: &IndirectViewBatch,
    ) -> BindGroup {
        gpu.create_bind_group(
            Some("preprocess_batch_data_bind_group"),
            &self.preprocess_batch_data_bg_layout,
            &BindGroupEntries::sequential((
                gpu_data.metadata_index_buffer.as_entire_binding(),
                gpu_data.indirect_buffer.as_entire_binding(),
                gpu_data.count_buffer.as_entire_binding(),
            )),
        )
    }
}
