use bevy::{
    pbr::{DrawMesh, SetMeshBindGroup, SetMeshViewBindGroup},
    render::render_phase::SetItemPipeline,
};

use super::gpu_registries::SetRegistryBindGroup;

pub type DrawVoxelChunk = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    SetRegistryBindGroup<2>,
    DrawMesh,
);
