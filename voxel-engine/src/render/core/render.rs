use bevy::{
    asset::{AssetServer, Handle},
    ecs::{
        system::Resource,
        world::{FromWorld, World},
    },
    pbr::{DrawMesh, MeshPipeline, MeshPipelineKey, SetMeshBindGroup, SetMeshViewBindGroup},
    render::{
        mesh::MeshVertexBufferLayout,
        render_phase::SetItemPipeline,
        render_resource::{
            RenderPipelineDescriptor, Shader, SpecializedMeshPipeline, SpecializedMeshPipelineError,
        },
    },
};

use super::{gpu_chunk::SetChunkBindGroup, gpu_registries::SetRegistryBindGroup};

#[derive(Resource)]
pub struct VoxelChunkPipeline {
    pub mesh_pipeline: MeshPipeline,
    pub vert: Handle<Shader>,
    pub frag: Handle<Shader>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoxelChunkPipelineKey {
    pub mesh_key: MeshPipelineKey,
}

impl FromWorld for VoxelChunkPipeline {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();

        Self {
            mesh_pipeline: world.resource::<MeshPipeline>().clone(),
            vert: server.load("shaders/greedy_mesh_vert.wgsl"),
            frag: server.load("shaders/greedy_mesh_frag.wgsl"),
        }
    }
}

impl SpecializedMeshPipeline for VoxelChunkPipeline {
    type Key = VoxelChunkPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayout,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key.mesh_key, layout)?;

        descriptor.vertex.shader = self.vert.clone();
        descriptor.fragment.as_mut().unwrap().shader = self.frag.clone();

        Ok(descriptor)
    }
}

pub type DrawVoxelChunk = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    SetRegistryBindGroup<2>,
    SetChunkBindGroup<3>,
    DrawMesh,
);
