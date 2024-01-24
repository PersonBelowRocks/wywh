pub mod mat;

mod gpu_chunk;
mod gpu_registries;
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
        render_phase::{AddRenderCommand, RenderPhase},
        render_resource::{Buffer, BufferDescriptor, SpecializedMeshPipelines},
        renderer::RenderDevice,
        Extract, Render, RenderApp, RenderSet,
    },
};

use mat::VxlChunkMaterial;

use self::{
    gpu_chunk::{ExtractedChunkOcclusion, ExtractedChunkQuads},
    gpu_registries::ExtractedTexregFaces,
    prepass::DrawVoxelChunkPrepass,
    render::{DrawVoxelChunk, VoxelChunkPipeline},
};

pub struct RenderCore;

#[derive(Resource)]
pub struct FaceBuffer(pub(crate) Option<Buffer>);

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, VxlChunkMaterial>,
        >::default());
        app.insert_resource(FaceBuffer(None));

        app.add_plugins(ExtractComponentPlugin::<ExtractedChunkQuads>::extract_visible());
        app.add_plugins(ExtractComponentPlugin::<ExtractedChunkOcclusion>::extract_visible());

        // Render app logic
        let render_app = app.sub_app_mut(RenderApp);

        render_app.add_render_command::<Opaque3d, DrawVoxelChunk>();
        render_app.add_render_command::<Opaque3dPrepass, DrawVoxelChunkPrepass>();

        render_app.init_resource::<SpecializedMeshPipelines<VoxelChunkPipeline>>();

        render_app.add_systems(
            ExtractSchedule,
            gpureg::extract_texreg_faces.run_if(not(resource_exists::<ExtractedTexregFaces>())),
        );
        render_app.add_systems(
            Render,
            ((gpureg::prepare_gpu_face_texture_buffer).in_set(RenderSet::PrepareResources)),
        );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app.init_resource::<VoxelChunkPipeline>();
    }
}
