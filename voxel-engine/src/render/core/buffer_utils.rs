use std::{marker::PhantomData, ops::Range};

use bevy::{
    core::{cast_slice, Pod},
    render::{
        render_resource::{
            encase::{self, internal::WriteInto, StorageBuffer as FormattedBuffer},
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, ShaderSize,
            ShaderType, StorageBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
    },
};

fn to_formatted_bytes<T: ShaderType + ShaderSize + WriteInto>(slice: &[T]) -> Vec<u8> {
    let mut scratch = FormattedBuffer::<Vec<u8>>::new(vec![]);
    scratch.write(&slice);
    scratch.into_inner()
}

#[derive(Clone)]
pub struct VramArray<T: ShaderType + ShaderSize + WriteInto> {
    buffer: Buffer,
    buffer_len: u64,
    label: &'static str,
    usages: BufferUsages,

    _ty: PhantomData<fn() -> T>,
}

impl<T: ShaderType + ShaderSize + WriteInto> VramArray<T> {
    pub fn item_size() -> u64 {
        u64::from(T::min_size())
    }

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

    pub fn len(&self) -> u64 {
        self.buffer_len
    }

    pub fn vram_bytes(&self) -> u64 {
        self.len() * Self::item_size()
    }

    /// Append a slice of data to the buffer on the gpu.
    pub fn append(&mut self, queue: &RenderQueue, gpu: &RenderDevice, data: &[T]) {
        // calculate our new size after we append all the data
        let size = self.vram_bytes() + (data.len() as u64 * Self::item_size());

        // this buffer will replace our old one
        let buffer = gpu.create_buffer(&BufferDescriptor {
            label: Some(self.label),
            size,
            usage: BufferUsages::COPY_DST | BufferUsages::COPY_SRC | self.usages,
            mapped_at_creation: false,
        });

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
        self.buffer_len += data.len() as u64;
    }

    /// Removes all the data between the different provided ranges. Think of this as copying all the data
    /// contained in the buffer that does NOT fall between any of the provided ranges.
    /// The range bounds are indices of `T` in the GPU buffer. Not indices of bytes!
    pub fn remove(&mut self, queue: &RenderQueue, gpu: &RenderDevice, ranges: &[Range<u64>]) {
        todo!()
    }
}
