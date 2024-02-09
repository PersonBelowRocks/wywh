pub mod mat;

mod gpu_chunk;
mod gpu_registries;
mod impls;
mod prepass;
mod render;

use gpu_registries as gpureg;

use bevy::{
    app::{App, Plugin},
    core_pipeline::{core_3d::Opaque3d, prepass::Opaque3dPrepass},
    ecs::system::Resource,
    pbr::{ExtendedMaterial, MaterialPlugin, StandardMaterial},
    prelude::*,
    render::{
        extract_component::ExtractComponentPlugin,
        extract_resource::ExtractResourcePlugin,
        mesh::MeshVertexAttribute,
        render_phase::{AddRenderCommand, RenderPhase},
        render_resource::{
            Buffer, BufferDescriptor, ShaderDefVal, SpecializedMeshPipelines, VertexFormat,
        },
        renderer::RenderDevice,
        Extract, Render, RenderApp, RenderSet,
    },
};

use mat::VxlChunkMaterial;

use crate::data::systems::{VoxelColorTextureAtlas, VoxelNormalTextureAtlas};

use self::{
    gpu_chunk::{extract_chunk_render_data, prepare_chunk_render_data, ChunkRenderDataStore},
    gpu_registries::{extract_texreg_faces, prepare_gpu_face_texture_buffer, ExtractedTexregFaces},
    prepass::{queue_prepass_chunks, ChunkPrepassPipeline, DrawVoxelChunkPrepass},
    render::{queue_chunks, DrawVoxelChunk, VoxelChunkPipeline},
};

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
                (prepare_gpu_face_texture_buffer, prepare_chunk_render_data)
                    .in_set(RenderSet::PrepareResources),
                (queue_chunks, queue_prepass_chunks).in_set(RenderSet::QueueMeshes),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app.init_resource::<ChunkRenderDataStore>();

        render_app.init_resource::<VoxelChunkPipeline>();
        render_app.init_resource::<ChunkPrepassPipeline>();
    }
}
