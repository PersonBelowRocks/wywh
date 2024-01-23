pub mod mat;

mod gpu_registries;
mod prepass;
mod render;
mod shaders;

use gpu_registries as gpureg;

use bevy::{
    app::{App, Plugin},
    ecs::system::Resource,
    pbr::{ExtendedMaterial, MaterialPlugin, StandardMaterial},
    prelude::*,
    render::{
        render_phase::RenderPhase,
        render_resource::{Buffer, BufferDescriptor},
        renderer::RenderDevice,
        Extract, Render, RenderApp, RenderSet,
    },
};

use mat::VxlChunkMaterial;

use self::gpu_registries::ExtractedTexregFaces;

pub struct RenderCore;

#[derive(Resource)]
pub struct FaceBuffer(pub(crate) Option<Buffer>);

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, VxlChunkMaterial>,
        >::default());
        app.insert_resource(FaceBuffer(None));

        // Render app logic
        let mut render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(
            ExtractSchedule,
            gpureg::extract_texreg_faces.run_if(not(resource_exists::<ExtractedTexregFaces>())),
        );
        render_app.add_systems(
            Render,
            ((gpureg::prepare_gpu_face_texture_buffer).in_set(RenderSet::PrepareResources)),
        );
    }
}
