use bevy::{
    pbr::{DrawMesh, SetMeshBindGroup, SetPrepassViewBindGroup},
    render::render_phase::SetItemPipeline,
};

use super::{gpu_chunk::SetChunkBindGroup, gpu_registries::SetRegistryBindGroup};

pub type DrawVoxelChunkPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetMeshBindGroup<1>,
    SetRegistryBindGroup<2>,
    SetChunkBindGroup<3>,
    DrawMesh,
);
