use std::mem;

use bevy::{
    ecs::{
        system::{Res, ResMut, Resource},
        world::Mut,
    },
    log::{debug, warn},
    prelude::{Deref, DerefMut, FromWorld, World},
    render::{
        render_resource::{BindGroup, BindGroupEntries},
        renderer::{RenderDevice, RenderQueue},
        MainWorld,
    },
};

use crate::render::core::observers::RenderWorldObservers;
use crate::{
    render::meshing::controller::{ChunkMeshData, ChunkMeshStatus, ExtractableChunkMeshData},
    util::{ChunkMap, ChunkSet},
};

use super::{indirect::IndirectChunkData, DefaultBindGroupLayouts};

pub fn extract_chunk_mesh_data(
    mut unprepared: ResMut<UnpreparedChunkMeshes>,
    mut remove_meshes: ResMut<RemoveChunkMeshes>,
    mut main_world: ResMut<MainWorld>,
) {
    main_world.resource_scope(
        |_world, mut extractable_meshes: Mut<ExtractableChunkMeshData>| {
            let ExtractableChunkMeshData {
                active,
                added,
                remove,
                should_extract,
            } = extractable_meshes.as_mut();

            if !*should_extract {
                return;
            }

            *should_extract = false;

            let mut extracted = 0;
            let mut removed = 0;

            // Remove meshes from the render world
            for chunk_pos in remove.drain() {
                unprepared.remove(chunk_pos);
                remove_meshes.set(chunk_pos);
                removed += 1;
            }

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
    mut rebuild: ResMut<ShouldUpdateChunkDataDependants>,
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
    mut rebuild: ResMut<ShouldUpdateChunkDataDependants>,
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
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

pub fn update_indirect_chunk_data_dependants(
    mut observers: ResMut<RenderWorldObservers>,
    mut update: ResMut<ShouldUpdateChunkDataDependants>,
    mut indirect_data: ResMut<IndirectRenderDataStore>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    gpu: Res<RenderDevice>,
) {
    if update.0 {
        for data in observers.values_mut() {
            data.buffers = None;
        }

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

            update.0 = false;
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
pub struct ShouldUpdateChunkDataDependants(pub bool);

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
