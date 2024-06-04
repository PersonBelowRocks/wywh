mod chunk_multidraw;
mod draw;
mod gpu_chunk;
mod gpu_registries;
mod impls;
mod prepass;
mod render;
mod shadows;
mod utils;

use bevy::{
    app::{App, Plugin},
    core_pipeline::{core_3d::Opaque3d, prepass::Opaque3dPrepass},
    ecs::system::Resource,
    pbr::Shadow,
    prelude::*,
    render::{
        extract_resource::ExtractResourcePlugin,
        mesh::MeshVertexAttribute,
        render_phase::AddRenderCommand,
        render_resource::{
            binding_types::{self},
            BindGroupLayout, BindGroupLayoutEntries, SamplerBindingType, ShaderStages, ShaderType,
            SpecializedRenderPipelines, TextureSampleType, VertexFormat,
        },
        renderer::RenderDevice,
        Render, RenderApp, RenderSet,
    },
};

use crate::data::{
    systems::{VoxelColorArrayTexture, VoxelNormalArrayTexture},
    texture::GpuFaceTexture,
};

use self::{
    gpu_chunk::{
        extract_chunk_entities, extract_chunk_mesh_data, prepare_chunk_mesh_data,
        ChunkRenderDataStore,
    },
    gpu_registries::{
        extract_texreg_faces, prepare_gpu_registry_data, ExtractedTexregFaces, RegistryBindGroup,
    },
    prepass::{queue_prepass_chunks, ChunkPrepassPipeline, DrawVoxelChunkPrepass},
    render::{queue_chunks, ChunkPipeline, DrawVoxelChunk},
    shadows::queue_shadows,
    utils::main_world_res_exists,
};

use super::{meshing::controller::ExtractableChunkMeshData, quad::GpuQuad};

pub struct RenderCore;

impl RenderCore {
    pub const QUAD_INDEX_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("quad_index_attr", 5099_0, VertexFormat::Uint32);
}

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<VoxelColorArrayTexture>::default());
        app.add_plugins(ExtractResourcePlugin::<VoxelNormalArrayTexture>::default());

        // Render app logic
        let render_app = app.sub_app_mut(RenderApp);

        render_app
            .add_render_command::<Opaque3d, DrawVoxelChunk>()
            .add_render_command::<Opaque3dPrepass, DrawVoxelChunkPrepass>()
            .add_render_command::<Shadow, DrawVoxelChunkPrepass>();

        render_app
            .init_resource::<SpecializedRenderPipelines<ChunkPipeline>>()
            .init_resource::<SpecializedRenderPipelines<ChunkPrepassPipeline>>()
            .init_resource::<ChunkRenderDataStore>();

        render_app.add_systems(
            ExtractSchedule,
            (
                extract_texreg_faces.run_if(not(resource_exists::<ExtractedTexregFaces>)),
                (
                    extract_chunk_entities,
                    extract_chunk_mesh_data
                        .run_if(main_world_res_exists::<ExtractableChunkMeshData>),
                )
                    .chain(),
            ),
        );
        render_app.add_systems(
            Render,
            (
                (
                    prepare_gpu_registry_data.run_if(not(resource_exists::<RegistryBindGroup>)),
                    prepare_chunk_mesh_data,
                )
                    .in_set(RenderSet::PrepareResources),
                (queue_chunks, queue_prepass_chunks, queue_shadows).in_set(RenderSet::QueueMeshes),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app.init_resource::<DefaultBindGroupLayouts>();

        render_app.init_resource::<ChunkPipeline>();
        render_app.init_resource::<ChunkPrepassPipeline>();
    }
}

#[derive(Resource, Clone)]
pub(crate) struct DefaultBindGroupLayouts {
    pub registry_bg_layout: BindGroupLayout,
    pub chunk_bg_layout: BindGroupLayout,
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
            chunk_bg_layout: gpu.create_bind_group_layout(
                Some("chunk_bind_group_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    (
                        binding_types::uniform_buffer_sized(
                            false,
                            Some(<Vec3 as ShaderType>::min_size()),
                        ),
                        binding_types::storage_buffer_read_only::<GpuQuad>(false),
                    ),
                ),
            ),
        }
    }
}
