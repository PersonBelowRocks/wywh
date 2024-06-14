use bevy::{
    core::cast_slice,
    prelude::*,
    render::{
        render_resource::{
            BindGroupEntries, BindGroupLayout, Buffer, BufferDescriptor, BufferInitDescriptor,
            BufferUsages, CommandEncoderDescriptor, ComputePassDescriptor,
            ComputePipelineDescriptor, PipelineCache, ShaderSize, SpecializedComputePipeline,
            SpecializedComputePipelines,
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

    /// Get the indices for the metadata of this observer's in-range chunks on the GPU as described by the provided indirect chunk data.
    /// If the metadata didn't exist in the provided indirect chunk data, then its index is not part of the returned vector.
    /// The caller must handle this (or do what this function does manually) if it's an issue.
    pub fn get_metadata_indices(&self, indirect_data: &IndirectChunkData) -> Vec<u32> {
        let mut chunk_metadata_indices = Vec::<u32>::with_capacity(self.in_range.len());

        for &chunk_pos in self.in_range.iter() {
            let Some(metadata_index) = indirect_data.get_chunk_metadata_index(chunk_pos) else {
                continue;
            };

            chunk_metadata_indices.push(metadata_index);
        }

        chunk_metadata_indices
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

pub fn populate_observer_multi_draw_buffers(
    gpu: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    indirect_data: Res<IndirectChunkData>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedComputePipelines<PopulateObserverBuffersPipeline>>,
    pipeline: Res<PopulateObserverBuffersPipeline>,
    default_layouts: Res<DefaultBindGroupLayouts>,
    mut observers: Query<&mut ExtractedObserverChunks>,
) {
    let pipeline_id = pipelines.specialize(&pipeline_cache, &pipeline, ());

    let compute_pipeline = pipeline_cache
        .get_compute_pipeline(pipeline_id)
        .expect("We don't support async pipeline compilation yet");

    for mut ob_chunks in &mut observers {
        if ob_chunks.buffers.is_some() {
            continue;
        }

        let num_chunks = ob_chunks.in_range.len();
        let chunk_metadata_indices = ob_chunks.get_metadata_indices(&indirect_data);

        let metadata_index_buffer = gpu.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("observer_chunks_metadata_indices_buffer"),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            contents: cast_slice(&chunk_metadata_indices),
        });

        let metadata_buffer = &indirect_data.buffers().metadata;

        let buffers = ObserverIndirectBuffers {
            indirect_buffer: create_indirect_buffer(&gpu, num_chunks as _),
            instance_buffer: create_instance_buffer(&gpu, num_chunks as _),
        };

        let input_bg = gpu.create_bind_group(
            Some("observer_population_input_bind_group"),
            &default_layouts.observer_buffers_input_layout,
            &BindGroupEntries::sequential((
                metadata_buffer.as_entire_binding(),
                metadata_index_buffer.as_entire_binding(),
            )),
        );

        let output_bg = gpu.create_bind_group(
            Some("observer_population_output_bind_group"),
            &default_layouts.observer_buffers_output_layout,
            &BindGroupEntries::sequential((
                buffers.instance_buffer.as_entire_binding(),
                buffers.indirect_buffer.as_entire_binding(),
            )),
        );

        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("populate_observer_buffers_encoder"),
        });

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("populate_observer_buffers_compute_pass"),
            timestamp_writes: None,
        });

        pass.set_bind_group(0, &input_bg, &[]);
        pass.set_bind_group(1, &output_bg, &[]);

        let num_chunks = ob_chunks.in_range.len() as u32;
        pass.dispatch_workgroups(1, 1, num_chunks);

        // TODO: keep implementing
    }
}
