use std::marker::PhantomData;

use bevy::render::{
    render_resource::{
        encase::{internal::WriteInto, StorageBuffer as FormattedBuffer},
        Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, ShaderSize, ShaderType,
    },
    renderer::{RenderDevice, RenderQueue},
};
use itertools::Itertools;
use rangemap::RangeSet;

use crate::util::ChunkIndexMap;

use super::{ChunkInstanceData, GpuChunkMetadata};

pub fn to_formatted_bytes<T: ShaderType + ShaderSize + WriteInto>(slice: &[T]) -> Vec<u8> {
    let capacity = slice.len() * u64::from(T::SHADER_SIZE) as usize;
    let mut scratch = FormattedBuffer::<Vec<u8>>::new(Vec::with_capacity(capacity));
    scratch.write(&slice).unwrap();
    scratch.into_inner()
}

pub fn instance_bytes_from_metadata(metadata: &ChunkIndexMap<GpuChunkMetadata>) -> Vec<u8> {
    let capacity = metadata.len();
    let mut scratch = Vec::<ChunkInstanceData>::with_capacity(capacity);

    for (chunk, data) in metadata {
        scratch.push(ChunkInstanceData {
            pos: chunk.worldspace_min().as_vec3(),
            first_quad: data.start_quad,
        });
    }

    to_formatted_bytes::<ChunkInstanceData>(&scratch)
}

/// Resizable array living entirely on the GPU. Unlike bevy's buffer helper types this type has no data in main memory.
/// This is done to conserve memory. Read and write operations are queued immediately, aka. this type doesn't try to batch
/// automatically. If you're going to use this then try to batch together writes and removals as much as possible to send fewer commands
/// to the GPU.
#[derive(Clone)]
pub struct VramArray<T: ShaderType + ShaderSize + WriteInto> {
    buffer: Buffer,
    buffer_len: u32,
    label: &'static str,
    usages: BufferUsages,

    _ty: PhantomData<fn() -> T>,
}

#[allow(dead_code)]
impl<T: ShaderType + ShaderSize + WriteInto> VramArray<T> {
    pub fn item_size() -> u64 {
        u64::from(T::SHADER_SIZE)
    }

    /// Constructor function. The provided label will be used as the wgpu label for the buffer.
    /// The buffer will always have the [`BufferUsages`] `COPY_DST` and `COPY_SRC` in addition to whatever you
    /// provide here. The buffer needs these usages in order to be copied and written to by this type.
    pub fn new(label: &'static str, usages: BufferUsages, gpu: &RenderDevice) -> Self {
        Self {
            buffer: gpu.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: 0,
                usage: BufferUsages::COPY_DST | BufferUsages::COPY_SRC | usages,
                mapped_at_creation: false,
            }),
            buffer_len: 0,
            label,
            usages,

            _ty: PhantomData,
        }
    }

    fn create_buffer(&self, gpu: &RenderDevice, size: u64) -> Buffer {
        gpu.create_buffer(&BufferDescriptor {
            label: Some(self.label),
            size,
            usage: BufferUsages::COPY_DST | BufferUsages::COPY_SRC | self.usages,
            mapped_at_creation: false,
        })
    }

    /// How many items of type `T` are in the buffer on the GPU.
    pub fn len(&self) -> u32 {
        self.buffer_len
    }

    /// How many bytes this buffer takes up on the GPU.
    /// Equivalent to `VramArray::len() * T::SHADER_SIZE`
    pub fn vram_bytes(&self) -> u64 {
        (self.len() as u64) * Self::item_size()
    }

    /// The GPU buffer containing all our data.
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Append a slice of data to the buffer on the gpu.
    pub fn append(&mut self, queue: &RenderQueue, gpu: &RenderDevice, data: &[T]) {
        // The encase crate (utilities for working on gpu data) seems to hate empty slices
        // and keeps insisting they're bigger than they are, so we return early if we have
        // an empty slice (there's nothing to do anyways, appending an empty slice is essentially no-op)
        if data.is_empty() {
            return;
        }

        // calculate our new size after we append all the data
        let size = self.vram_bytes() + (data.len() as u64 * Self::item_size());

        // this buffer will replace our old one
        let buffer = self.create_buffer(gpu, size);

        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: Some(&format!("{}-append_cmd_encoder", self.label)),
        });

        // first we copy our existing data to the new buffer
        encoder.copy_buffer_to_buffer(&self.buffer, 0, &buffer, 0, self.vram_bytes());
        queue.submit([encoder.finish()]);

        // then we format and append the provided data
        let formatted = to_formatted_bytes(&data);
        queue.write_buffer(&buffer, self.vram_bytes(), &formatted);

        // set the buffer and bump our length
        self.buffer = buffer;
        self.buffer_len += data.len() as u32;
    }

    /// Removes all the data between the different provided ranges. Think of this as copying all the data
    /// contained in the buffer that does NOT fall between any of the provided ranges.
    /// The range bounds are indices of `T` in the GPU buffer. Not indices of bytes!
    pub fn remove(&mut self, gpu: &RenderDevice, queue: &RenderQueue, ranges: &RangeSet<u32>) {
        // we collect the ranges into a vector here so we don't have to do any duplicate calculation of gaps
        let remaining = ranges.gaps(&(0..self.len())).collect_vec();
        // the end of the range should always be greater than the start
        let new_length: u32 = remaining.iter().map(|r| r.end - r.start).sum();

        // allocate a new buffer on the GPU
        let new_buffer = self.create_buffer(gpu, (new_length as u64) * Self::item_size());

        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: Some(&format!("{}-remove_cmd_encoder", self.label)),
        });

        // copy everything we want to keep into our new buffer
        let mut current_index: u64 = 0;
        for range in remaining {
            let min = (range.start as u64) * Self::item_size();
            let max = (range.end as u64) * Self::item_size();
            let copy_size = max - min;

            encoder.copy_buffer_to_buffer(&self.buffer, min, &new_buffer, current_index, copy_size);
            current_index += copy_size;
        }

        queue.submit([encoder.finish()]);

        self.buffer = new_buffer;
        self.buffer_len = new_length;
    }
}
