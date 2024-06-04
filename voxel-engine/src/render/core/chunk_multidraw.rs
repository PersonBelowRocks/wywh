use std::{cell::OnceCell, ops::Range};

use bevy::{
    core::cast_slice,
    prelude::*,
    render::{
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, ShaderSize,
        },
        renderer::{RenderDevice, RenderQueue},
    },
};
use once_cell::unsync::Lazy;

use crate::{
    render::{meshing::controller::ChunkMeshData, quad::GpuQuad},
    util::{ChunkMap, MultiChunkMap},
};

use super::{buffer_utils::VramArray, gpu_chunk::ChunkRenderData};

#[derive(Clone)]
pub struct ChunkBufferBounds {
    pub instance: u32,
    pub indices: Range<u32>,
    pub quads: Range<u32>,
}

fn copyable_buffer_desc(label: &'static str, usages: BufferUsages) -> BufferDescriptor<'static> {
    BufferDescriptor {
        label: Some(label),
        size: 0,
        usage: BufferUsages::COPY_DST | BufferUsages::COPY_SRC | usages,
        mapped_at_creation: false,
    }
}

pub const INDEX_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| copyable_buffer_desc("chunk_multidraw_index_buffer", BufferUsages::INDEX));

pub const QUAD_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| copyable_buffer_desc("chunk_multidraw_quad_buffer", BufferUsages::STORAGE));

pub const INSTANCE_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| copyable_buffer_desc("chunk_multidraw_instance_buffer", BufferUsages::VERTEX));

pub const INDIRECT_BUFFER_DESC: Lazy<BufferDescriptor<'static>> =
    Lazy::new(|| copyable_buffer_desc("chunk_multidraw_indirect_buffer", BufferUsages::INDIRECT));

#[derive(Clone)]
pub struct MultidrawBuffers {
    pub index: VramArray<u32>,
    pub quad: VramArray<GpuQuad>,
    // TODO: instance data
    pub instance: Buffer,
    pub instance_buf_len: u64,
    // TODO: make this a vram buffer
    pub indirect: Buffer,
    pub indirect_buf_len: u64,
}

impl MultidrawBuffers {
    pub fn new(gpu: &RenderDevice) -> Self {
        Self {
            index: todo!(),
            quad: todo!(),
            instance: gpu.create_buffer(&INSTANCE_BUFFER_DESC),
            instance_buf_len: 0,
            indirect: gpu.create_buffer(&INDIRECT_BUFFER_DESC),
            indirect_buf_len: 0,
        }
    }
}

#[derive(Resource, Clone)]
pub struct ChunkMultidrawData {
    buffers: MultidrawBuffers,
    bounds: MultiChunkMap<ChunkBufferBounds>,
}

impl ChunkMultidrawData {
    #[allow(dead_code)]
    pub fn new(gpu: &RenderDevice) -> Self {
        Self {
            buffers: MultidrawBuffers::new(gpu),
            bounds: MultiChunkMap::new(),
        }
    }

    pub fn upload_chunks(
        &mut self,
        gpu: &RenderDevice,
        queue: &RenderQueue,
        chunks: ChunkMap<ChunkMeshData>,
    ) {
        let mut new_buffers = MultidrawBuffers::new(gpu);
        let mut bounds = MultiChunkMap::<ChunkBufferBounds>::new();

        for (chunk, mesh) in chunks.iter() {
            todo!();
        }
    }
}
