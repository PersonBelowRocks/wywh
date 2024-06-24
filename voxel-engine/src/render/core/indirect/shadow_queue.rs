use bevy::{
    pbr::{
        CascadesVisibleEntities, CubemapVisibleEntities, ExtractedDirectionalLight,
        ExtractedPointLight, LightEntity, MeshPipelineKey, Shadow, ViewLightEntities,
    },
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_phase::DrawFunctions,
        render_resource::{PipelineCache, SpecializedRenderPipelines},
        view::VisibleEntities,
    },
};

use crate::render::core::{gpu_chunk::IndirectRenderDataStore, gpu_registries::RegistryBindGroup};

use super::{IndirectChunkPipelineKey, IndirectChunkPrepassPipeline, IndirectChunksPrepass};

// TODO: implement shadows for the GPU driven chunk renderer
pub fn shadow_queue_indirect_chunks() {
    todo!()
}
