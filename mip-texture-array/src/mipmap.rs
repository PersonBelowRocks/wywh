use bevy::{
    prelude::*,
    render::{
        render_resource::{
            binding_types::{texture_2d_array, texture_storage_2d_array},
            BindGroupLayout, BindGroupLayoutEntries, CachedComputePipelineId, ComputePipeline,
            ComputePipelineDescriptor, ComputePipelineId, PipelineCache, ShaderDefVal,
            ShaderStages, SpecializedComputePipeline, SpecializedComputePipelines,
            StorageTextureAccess, TextureSampleType,
        },
        renderer::RenderDevice,
    },
};

use crate::STORAGE_TEXTURE_FORMAT;

pub const MIPMAP_COMPUTE_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(147253048844306429480044375066837132481);

pub const WORKGROUP_SIZE_PER_DIM: u32 = 8;
#[derive(Clone, Resource)]
pub struct MipGeneratorPipelineMeta {
    pub(crate) bg_layout: BindGroupLayout,
    pub(crate) pipeline_id: CachedComputePipelineId,
}

impl FromWorld for MipGeneratorPipelineMeta {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope::<SpecializedComputePipelines<MipGeneratorPipeline>, Self>(
            |world, mut specialized_pipelines| {
                let gpu = world.resource::<RenderDevice>();
                let cache = world.resource::<PipelineCache>();

                let bg_layout = gpu.create_bind_group_layout(
                    "compute_mipmap_bg_layout",
                    &BindGroupLayoutEntries::sequential(
                        ShaderStages::COMPUTE,
                        (
                            texture_2d_array(TextureSampleType::Float { filterable: true }),
                            texture_storage_2d_array(
                                STORAGE_TEXTURE_FORMAT,
                                StorageTextureAccess::WriteOnly,
                            ),
                        ),
                    ),
                );

                Self {
                    bg_layout: bg_layout.clone(),
                    pipeline_id: specialized_pipelines.specialize(
                        cache,
                        &MipGeneratorPipeline { layout: bg_layout },
                        MipGeneratorPipelineKey,
                    ),
                }
            },
        )
    }
}

#[derive(Clone)]
pub(crate) struct MipGeneratorPipeline {
    layout: BindGroupLayout,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub(crate) struct MipGeneratorPipelineKey;

impl SpecializedComputePipeline for MipGeneratorPipeline {
    type Key = MipGeneratorPipelineKey;

    fn specialize(&self, _key: Self::Key) -> ComputePipelineDescriptor {
        let defs = vec![
            ShaderDefVal::UInt("WG_SIZE_X".into(), 8),
            ShaderDefVal::UInt("WG_SIZE_Y".into(), 8),
        ];

        ComputePipelineDescriptor {
            label: Some("mipmap_generation_pipeline".into()),
            push_constant_ranges: vec![],
            shader: MIPMAP_COMPUTE_SHADER_HANDLE,
            entry_point: "compute_mipmap".into(),
            layout: vec![self.layout.clone()],
            shader_defs: defs,
        }
    }
}
