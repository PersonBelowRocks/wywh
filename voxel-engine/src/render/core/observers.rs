use bevy::{
    prelude::*,
    render::{
        render_resource::{Buffer, BufferUsages, ShaderSize},
        renderer::{RenderDevice, RenderQueue},
        Extract,
    },
};

use crate::topo::{controller::RenderableObserverChunks, world::ChunkPos};

use super::indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData};

#[derive(Clone)]
pub struct ObserverIndirectBuffers {
    pub indirect_buffer: Buffer,
    pub instance_buffer: Buffer,
    pub count: Buffer,
}

#[derive(Component, Clone, Default)]
pub struct ExtractedObserverChunks {
    pub in_range: Vec<ChunkPos>,
    pub buffers: Option<ObserverIndirectBuffers>,
}

impl ExtractedObserverChunks {
    pub fn new(chunks: Vec<ChunkPos>) -> Self {
        Self {
            in_range: chunks,
            buffers: None,
        }
    }
}

pub fn extract_observer_chunks(
    mut cmds: Commands,
    observers: Extract<
        Query<(Entity, &RenderableObserverChunks), Changed<RenderableObserverChunks>>,
    >,
    mut existing: Query<&mut ExtractedObserverChunks>,
) {
    for (entity, ob_chunks) in &observers {
        if !ob_chunks.should_extract {
            continue;
        }

        match existing.get_mut(entity).ok() {
            Some(mut existing_ob_chunks) => {
                existing_ob_chunks.in_range.clear();
                existing_ob_chunks
                    .in_range
                    .extend(ob_chunks.in_range.iter());

                existing_ob_chunks.buffers = None;
            }
            None => {
                let chunks = ob_chunks.in_range.iter().collect::<Vec<_>>();
                cmds.get_or_spawn(entity)
                    .insert(ExtractedObserverChunks::new(chunks));
            }
        }
    }
}

pub fn prepare_observer_multi_draw_buffers(
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    indirect_data: Res<IndirectChunkData>,
    observers: Query<&mut ExtractedObserverChunks>,
) {
    for ob_chunks in &observers {
        if ob_chunks.buffers.is_some() {
            continue;
        }

        // TODO: we should probably do all this on the GPU
    }
}
