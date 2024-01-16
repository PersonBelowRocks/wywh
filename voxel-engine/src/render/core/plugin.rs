use bevy::{
    app::{App, Plugin},
    ecs::system::Resource,
    pbr::{ExtendedMaterial, MaterialPlugin, StandardMaterial},
    render::{
        render_resource::{Buffer, BufferDescriptor},
        renderer::RenderDevice,
        RenderApp,
    },
};

use super::mat::VxlChunkMaterial;

pub struct RenderCore;

#[derive(Resource)]
pub struct FaceBuffer(pub(crate) Option<Buffer>);

impl Plugin for RenderCore {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, VxlChunkMaterial>,
        >::default());
        app.insert_resource(FaceBuffer(None));
    }
}
