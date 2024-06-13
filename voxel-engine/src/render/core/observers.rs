use bevy::{
    prelude::*,
    render::{
        render_resource::{
            BindGroupLayout, Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages,
            CommandEncoderDescriptor, ComputePipelineDescriptor, ShaderSize,
            SpecializedComputePipeline,
        },
        renderer::{RenderDevice, RenderQueue},
        Extract,
    },
};

use crate::topo::{controller::RenderableObserverChunks, world::ChunkPos};

use super::{
    indirect::{ChunkInstanceData, IndexedIndirectArgs, IndirectChunkData},
    shaders::POPULATE_OBSERVER_BUFFERS_HANDLE,
    utils::add_shader_constants,
    DefaultBindGroupLayouts,
};

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

#[derive(Resource)]
pub struct PopulateObserverBuffersPipeline {
    pub shader: Handle<Shader>,
    pub input_layout: BindGroupLayout,
    pub output_layout: BindGroupLayout,
}

impl FromWorld for PopulateObserverBuffersPipeline {
    fn from_world(world: &mut World) -> Self {
        let default_layouts = world.resource::<DefaultBindGroupLayouts>();

        Self {
            shader: POPULATE_OBSERVER_BUFFERS_HANDLE,
            input_layout: default_layouts.observer_buffers_input_layout.clone(),
            output_layout: default_layouts.observer_buffers_output_layout.clone(),
        }
    }
}

impl SpecializedComputePipeline for PopulateObserverBuffersPipeline {
    type Key = ();

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let mut shader_defs = vec![];
        add_shader_constants(&mut shader_defs);

        ComputePipelineDescriptor {
            label: Some("populate_observer_buffers_pipeline".into()),
            entry_point: "populate_buffers".into(),
            shader: self.shader.clone(),
            push_constant_ranges: vec![],
            shader_defs: shader_defs,
            layout: vec![self.input_layout.clone(), self.output_layout.clone()],
        }
    }
}

fn create_indirect_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_observer_indirect_buffer"),
        size: (chunks as u64) * u64::from(IndexedIndirectArgs::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::INDIRECT,
        mapped_at_creation: false,
    })
}

fn create_instance_buffer(gpu: &RenderDevice, chunks: u32) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_observer_instance_buffer"),
        size: (chunks as u64) * u64::from(ChunkInstanceData::SHADER_SIZE),
        usage: BufferUsages::STORAGE | BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

fn create_count_buffer(gpu: &RenderDevice) -> Buffer {
    gpu.create_buffer(&BufferDescriptor {
        label: Some("chunk_observer_num_chunks"),
        size: u32::SHADER_SIZE.into(),
        usage: BufferUsages::UNIFORM | BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
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

        let num_chunks = ob_chunks.in_range.len() as u32;
        let indirect_buffer = create_indirect_buffer(&gpu, num_chunks);
        let instance_buffer = create_instance_buffer(&gpu, num_chunks);

        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("populate_observer_indirect_instance_buffers_encoder"),
        });

        // TODO: keep implementing
    }
}
