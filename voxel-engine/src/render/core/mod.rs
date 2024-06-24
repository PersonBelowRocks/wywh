mod gpu_chunk;
mod gpu_registries;
mod graph;
mod impls;
mod indirect;
mod observers;
mod phase;
mod shaders;
mod utils;

use bevy::core_pipeline::core_3d::graph::Core3d;
use bevy::render::render_graph::{RenderGraphApp, ViewNodeRunner};
use bevy::render::render_phase::{DrawFunctions, SortedRenderPhasePlugin, ViewSortedRenderPhases};
use bevy::render::render_resource::ShaderSize;
use bevy::{
    app::{App, Plugin},
    core_pipeline::{core_3d::Opaque3d, prepass::Opaque3dPrepass},
    ecs::system::Resource,
    pbr::Shadow,
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
use gpu_chunk::{
    remove_chunk_meshes, update_indirect_chunk_data_dependants, upload_chunk_meshes,
    IndirectRenderDataStore, RemoveChunkMeshes, ShouldUpdateChunkDataDependants,
};
use graph::{ChunkPrepassNode, Nodes};
use indirect::{
    prepass_queue_indirect_chunks, render_queue_indirect_chunks, shadow_queue_indirect_chunks,
    ChunkInstanceData, GpuChunkMetadata, IndexedIndirectArgs, IndirectChunkData,
    IndirectChunkPrepassPipeline, IndirectChunkRenderPipeline, IndirectChunksPrepass,
    IndirectChunksRender,
};
use observers::{
    extract_observer_chunks, populate_observer_multi_draw_buffers, PopulateObserverBuffersPipeline,
};
use phase::PrepassChunkPhaseItem;
use shaders::load_internal_shaders;

use crate::data::{
    systems::{VoxelColorArrayTexture, VoxelNormalArrayTexture},
    texture::GpuFaceTexture,
};
use crate::render::core::observers::RenderWorldObservers;

use self::{
    gpu_chunk::{extract_chunk_mesh_data, UnpreparedChunkMeshes},
    gpu_registries::{
        extract_texreg_faces, prepare_gpu_registry_data, ExtractedTexregFaces, RegistryBindGroup,
    },
    utils::main_world_res_exists,
};

use super::{meshing::controller::ExtractableChunkMeshData, quad::GpuQuad};

pub struct RenderCore;

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        load_internal_shaders(app);

        app.add_plugins((
            ExtractResourcePlugin::<VoxelColorArrayTexture>::default(),
            ExtractResourcePlugin::<VoxelNormalArrayTexture>::default(),
        ));

        // Render app logic
        let render_app = app.sub_app_mut(RenderApp);

        render_app
            .init_resource::<DrawFunctions<PrepassChunkPhaseItem>>()
            .init_resource::<ViewSortedRenderPhases<PrepassChunkPhaseItem>>()
            .init_resource::<SpecializedRenderPipelines<IndirectChunkRenderPipeline>>()
            .init_resource::<SpecializedRenderPipelines<IndirectChunkPrepassPipeline>>()
            .init_resource::<SpecializedComputePipelines<PopulateObserverBuffersPipeline>>()
            .init_resource::<RenderWorldObservers>()
            .init_resource::<ShouldUpdateChunkDataDependants>()
            .init_resource::<RemoveChunkMeshes>()
            .init_resource::<UnpreparedChunkMeshes>();

        render_app
            .add_render_command::<Opaque3dPrepass, IndirectChunksPrepass>()
            .add_render_command::<Opaque3d, IndirectChunksRender>()
            .add_render_command::<Shadow, IndirectChunksPrepass>();

        render_app
            .add_render_graph_node::<ViewNodeRunner<ChunkPrepassNode>>(Core3d, Nodes::Prepass);

        render_app.add_systems(
            ExtractSchedule,
            (
                extract_texreg_faces.run_if(not(resource_exists::<ExtractedTexregFaces>)),
                (
                    extract_chunk_mesh_data
                        .run_if(main_world_res_exists::<ExtractableChunkMeshData>),
                    extract_observer_chunks,
                )
                    .chain(),
            ),
        );
        render_app.add_systems(
            Render,
            (
                (
                    prepare_gpu_registry_data.run_if(not(resource_exists::<RegistryBindGroup>)),
                    (
                        remove_chunk_meshes,
                        upload_chunk_meshes,
                        update_indirect_chunk_data_dependants,
                        populate_observer_multi_draw_buffers
                            .run_if(resource_exists::<IndirectRenderDataStore>),
                    )
                        .chain(),
                )
                    .in_set(RenderSet::PrepareResources),
                (
                    render_queue_indirect_chunks,
                    prepass_queue_indirect_chunks,
                    // TODO: fix up light view entities to use GPU frustum culling like normal cameras
                    // shadow_queue_indirect_chunks,
                )
                    .in_set(RenderSet::Queue),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app.init_resource::<DefaultBindGroupLayouts>();
        render_app.init_resource::<IndirectRenderDataStore>();

        render_app.init_resource::<IndirectChunkRenderPipeline>();
        render_app.init_resource::<IndirectChunkPrepassPipeline>();
        render_app.init_resource::<PopulateObserverBuffersPipeline>();
    }
}

#[derive(Resource, Clone)]
pub(crate) struct DefaultBindGroupLayouts {
    pub registry_bg_layout: BindGroupLayout,
    pub indirect_chunk_bg_layout: BindGroupLayout,
    pub observer_buffers_input_layout: BindGroupLayout,
    pub observer_buffers_output_layout: BindGroupLayout,
}

impl FromWorld for DefaultBindGroupLayouts {
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
            indirect_chunk_bg_layout: gpu.create_bind_group_layout(
                Some("multidraw_chunks_bind_group_layout"),
                &BindGroupLayoutEntries::single(
                    ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    binding_types::storage_buffer_read_only::<GpuQuad>(false),
                ),
            ),
            observer_buffers_input_layout: gpu.create_bind_group_layout(
                Some("observer_buffers_input_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::COMPUTE,
                    (
                        binding_types::storage_buffer_read_only::<GpuChunkMetadata>(false),
                        binding_types::storage_buffer_read_only::<u32>(false),
                    ),
                ),
            ),
            observer_buffers_output_layout: gpu.create_bind_group_layout(
                Some("observer_buffers_output_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::COMPUTE,
                    (
                        binding_types::storage_buffer::<ChunkInstanceData>(false),
                        binding_types::storage_buffer::<IndexedIndirectArgs>(false),
                        binding_types::storage_buffer_sized(false, Some(u32::SHADER_SIZE)),
                    ),
                ),
            ),
        }
    }
}
