pub mod mat;

mod gpu_chunk;
mod gpu_registries;
mod impls;
mod prepass;
mod render;

use bevy::{
    app::{App, Plugin},
    core_pipeline::{core_3d::Opaque3d, prepass::Opaque3dPrepass},
    ecs::system::Resource,
    prelude::*,
    render::{
        extract_resource::ExtractResourcePlugin,
        mesh::MeshVertexAttribute,
        render_phase::AddRenderCommand,
        render_resource::{
            binding_types::{sampler, storage_buffer_read_only, texture_2d},
            BindGroupLayout, BindGroupLayoutEntries, SamplerBindingType, ShaderDefVal,
            ShaderStages, SpecializedMeshPipelines, TextureSampleType, VertexFormat,
        },
        renderer::RenderDevice,
        Render, RenderApp, RenderSet,
    },
};

use crate::data::{
    systems::{VoxelColorTextureAtlas, VoxelNormalTextureAtlas},
    texture::GpuFaceTexture,
};

use self::{
    gpu_chunk::{extract_chunk_render_data, prepare_chunk_render_data, ChunkRenderDataStore},
    gpu_registries::{
        extract_texreg_faces, prepare_gpu_registry_data, ExtractedTexregFaces, RegistryBindGroup,
    },
    prepass::{queue_prepass_chunks, ChunkPrepassPipeline, DrawVoxelChunkPrepass},
    render::{queue_chunks, DrawVoxelChunk, VoxelChunkPipeline},
};

use super::quad::GpuQuad;

pub(crate) fn u32_shader_def(name: &str, value: u32) -> ShaderDefVal {
    ShaderDefVal::UInt(name.into(), value)
}

pub struct RenderCore;

impl RenderCore {
    pub const QUAD_INDEX_ATTR: MeshVertexAttribute =
        MeshVertexAttribute::new("quad_index_attr", 5099_0, VertexFormat::Uint32);
}

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<VoxelColorTextureAtlas>::default());
        app.add_plugins(ExtractResourcePlugin::<VoxelNormalTextureAtlas>::default());

        // Render app logic
        let render_app = app.sub_app_mut(RenderApp);

        render_app.add_render_command::<Opaque3d, DrawVoxelChunk>();
        render_app.add_render_command::<Opaque3dPrepass, DrawVoxelChunkPrepass>();

        render_app.init_resource::<SpecializedMeshPipelines<VoxelChunkPipeline>>();
        render_app.init_resource::<SpecializedMeshPipelines<ChunkPrepassPipeline>>();

        render_app.add_systems(
            ExtractSchedule,
            (
                extract_texreg_faces.run_if(not(resource_exists::<ExtractedTexregFaces>())),
                extract_chunk_render_data,
            ),
        );
        render_app.add_systems(
            Render,
            (
                (
                    prepare_gpu_registry_data.run_if(not(resource_exists::<RegistryBindGroup>())),
                    prepare_chunk_render_data,
                )
                    .in_set(RenderSet::PrepareResources),
                (queue_chunks, queue_prepass_chunks).in_set(RenderSet::QueueMeshes),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app.init_resource::<ChunkRenderDataStore>();

        render_app.init_resource::<DefaultBindGroupLayouts>();

        render_app.init_resource::<VoxelChunkPipeline>();
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
                        storage_buffer_read_only::<GpuFaceTexture>(false),
                        texture_2d(TextureSampleType::default()),
                        sampler(SamplerBindingType::Filtering),
                        texture_2d(TextureSampleType::default()),
                        sampler(SamplerBindingType::Filtering),
                    ),
                ),
            ),
            chunk_bg_layout: gpu.create_bind_group_layout(
                Some("registry_bind_group_layout"),
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    (
                        storage_buffer_read_only::<GpuQuad>(false),
                        storage_buffer_read_only::<u32>(false),
                    ),
                ),
            ),
        }
    }
}
