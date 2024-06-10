use std::mem;

use bevy::{
    ecs::{
        entity::Entity,
        query::{ROQueryItem, With},
        system::{
            lifetimeless::{Read, SRes},
            Commands, Query, Res, ResMut, Resource, SystemParamItem,
        },
        world::Mut,
    },
    log::{debug, warn},
    prelude::{Deref, DerefMut, FromWorld, World},
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            BindGroup, BindGroupEntries, BindingResource, Buffer, BufferBinding, BufferUsages,
            BufferVec, StorageBuffer, UniformBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        Extract, MainWorld,
    },
};
use hashbrown::hash_map::Entry;
use itertools::Itertools;

use crate::{
    render::{
        meshing::controller::{ChunkMeshData, ChunkMeshStatus, ExtractableChunkMeshData},
        quad::GpuQuad,
    },
    topo::world::{ChunkEntity, ChunkPos},
    util::{ChunkMap, ChunkSet},
};

use super::{indirect::IndirectChunkData, DefaultBindGroupLayouts};

// TODO: remove old code dealing with individual chunks, in favor of the indirect multidraw system

pub fn extract_chunk_mesh_data(
    mut unprepared: ResMut<UnpreparedChunkMeshes>,
    mut remove: ResMut<RemoveChunkMeshes>,
    mut main_world: ResMut<MainWorld>,
) {
    main_world.resource_scope(
        |_world, mut extractable_meshes: Mut<ExtractableChunkMeshData>| {
            let mut extracted = 0;
            let mut removed = 0;

            // Remove meshes from the render world
            for chunk_pos in extractable_meshes.removed.drain() {
                unprepared.remove(chunk_pos);
                remove.set(chunk_pos);
                removed += 1;
            }

            let ExtractableChunkMeshData {
                active,
                added,
                removed: _
            } = extractable_meshes.as_mut();

            // Extract all chunks that were added
            for (chunk_pos, mesh) in added.drain() {
                // Only extract chunks if they were present in the activity tracker
                if let Some(active) = active.get_mut(chunk_pos) {
                    // We avoid empty chunks. The mesh controller should only queue non-empty chunks for extraction but
                    // we do an additional error check here just in case.
                    if active.status == ChunkMeshStatus::Empty {
                        warn!("Can't extract chunk mesh for {chunk_pos} because it was marked as empty.");
                        continue;
                    }

                    unprepared.set(chunk_pos, mesh);

                    // mark the extracted chunk as being extracted
                    active.status = ChunkMeshStatus::Extracted;
                    extracted += 1;
                } else {
                    warn!("Couldn't extract chunk {chunk_pos} because it wasn't marked as an active chunk.")
                }
            }

            if extracted > 0 {
                debug!("Extracted {} chunk meshes to render world", extracted);
            }

            if removed > 0 {
                debug!("Removed {} chunk meshes from render world", removed);
            }
        },
    );
}

/// Untrack chunk meshes in the render world and remove their data on the GPU
pub fn remove_chunk_meshes(
    mut remove: ResMut<RemoveChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut rebuild: ResMut<RebuildChunkQuadBindGroup>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
    // is quite sensitive to running with empty vectors and maps.
    if remove.is_empty() {
        return;
    }

    let remove = mem::replace(&mut remove.0, ChunkSet::default());
    let removed = remove.len();
    indirect_data.chunks.remove_chunks(gpu, queue, remove);

    rebuild.0 = true;

    debug!("Removed {removed} chunks from the render world");
}

/// Upload unprepared chunk meshes to the GPU and track them in the render world
pub fn upload_chunk_meshes(
    mut unprepared: ResMut<UnpreparedChunkMeshes>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    mut rebuild: ResMut<RebuildChunkQuadBindGroup>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    // TODO: every time there's a chunk to upload, we upload it, however this can get very slow and unecessary when we don't care
    // about rendering the updated chunk immediately. we should batch together chunk uploads when they don't need to happen immediately

    let gpu = gpu.as_ref();
    let queue = queue.as_ref();

    // We want to avoid running GPU upload/updating logic with zero chunks and whatnot because a lot of the code
    // is quite sensitive to running with empty vectors and maps.
    if unprepared.is_empty() {
        return;
    }

    let meshes = mem::replace(&mut unprepared.0, ChunkMap::default());
    let added = meshes.len();
    indirect_data.chunks.upload_chunks(gpu, queue, meshes);

    rebuild.0 = true;

    debug!("Uploaded and prepared {added} chunks");
}

pub fn rebuild_chunk_quad_bind_group(
    mut rebuild: ResMut<RebuildChunkQuadBindGroup>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    gpu: Res<RenderDevice>,
) {
    if rebuild.0 {
        let quad_vram_array = &indirect_data.chunks.buffers().quad;

        // we only make a bind group if the buffer is long enough to be bound
        if quad_vram_array.vram_bytes() > 0 {
            let quad_buffer = quad_vram_array.buffer();

            let bg = gpu.create_bind_group(
                "indirect_chunks_bind_group",
                &default_layouts.indirect_chunk_bg_layout,
                &BindGroupEntries::single(quad_buffer.as_entire_buffer_binding()),
            );

            debug!("Rebuilt chunk quad bind group");

            indirect_data.bind_group = Some(bg);
            indirect_data.ready = true;

            rebuild.0 = false;
        }
    }
}

/// A store of unprepared chunk meshes
#[derive(Resource, Default, Deref, DerefMut)]
pub struct UnpreparedChunkMeshes(pub ChunkMap<ChunkMeshData>);

/// A store of chunks that should be removed from the render world
#[derive(Resource, Default, Deref, DerefMut)]
pub struct RemoveChunkMeshes(pub ChunkSet);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct RebuildChunkQuadBindGroup(pub bool);

#[derive(Resource)]
pub struct IndirectRenderDataStore {
    pub chunks: IndirectChunkData,
    pub bind_group: Option<BindGroup>,
    pub ready: bool,
}

impl FromWorld for IndirectRenderDataStore {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        Self {
            chunks: IndirectChunkData::new(gpu),
            bind_group: None,
            ready: false,
        }
    }
}
