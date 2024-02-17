use std::cmp::min;

use bevy::{
    asset::load_internal_asset,
    ecs::system::SystemParam,
    math::{uvec2, uvec3},
    prelude::*,
    render::{
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{texture_2d_array, texture_storage_2d_array},
            BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, CommandEncoderDescriptor,
            ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, PipelineCache,
            ShaderDefVal, ShaderStages, SpecializedComputePipeline, SpecializedComputePipelines,
            StorageTextureAccess, TextureAspect, TextureFormat, TextureSampleType, TextureView,
            TextureViewDescriptor, TextureViewDimension,
        },
        renderer::{RenderDevice, RenderQueue},
        RenderApp,
    },
};

use crate::{TextureArray, TEXTURE_FORMAT};

pub const MIPMAP_COMPUTE_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(147253048844306429480044375066837132481);

pub const WORKGROUP_SIZE_PER_DIM: u32 = 8;

#[derive(SystemParam)]
pub struct MipmapGeneratorSystemParam<'w> {
    pub textures: ResMut<'w, RenderAssets<Image>>,
    pub pipeline: Res<'w, MipGeneratorPipeline>,
    pub pipeline_cache: Res<'w, PipelineCache>,
    pub compute_pipelines: ResMut<'w, SpecializedComputePipelines<MipGeneratorPipeline>>,
    pub queue: Res<'w, RenderQueue>,
    pub gpu: Res<'w, RenderDevice>,
}

pub struct MipmapGeneratorParams<'a> {
    pub textures: &'a mut RenderAssets<Image>,
    pub pipeline: &'a MipGeneratorPipeline,
    pub pipeline_cache: &'a PipelineCache,
    pub compute_pipelines: &'a mut SpecializedComputePipelines<MipGeneratorPipeline>,
    pub queue: &'a RenderQueue,
    pub gpu: &'a RenderDevice,
}

impl<'a> From<MipmapGeneratorSystemParam<'a>> for MipmapGeneratorParams<'a> {
    fn from(value: MipmapGeneratorSystemParam<'a>) -> Self {
        Self {
            textures: value.textures.into_inner(),
            pipeline: value.pipeline.into_inner(),
            pipeline_cache: value.pipeline_cache.into_inner(),
            compute_pipelines: value.compute_pipelines.into_inner(),
            queue: value.queue.into_inner(),
            gpu: value.gpu.into_inner(),
        }
    }
}

#[derive(Resource, Default)]
pub struct TexArrayMipGenerator;

impl TexArrayMipGenerator {
    pub fn generate_mips<'a>(
        &self,
        texarr: &mut TextureArray,
        params: impl Into<MipmapGeneratorParams<'a>>,
    ) {
        let params: MipmapGeneratorParams = params.into();

        let cmd_encoder_desc = CommandEncoderDescriptor {
            label: "mipmap_generation_cmd_encoder".into(),
        };
        let mut encoder = params.gpu.create_command_encoder(&cmd_encoder_desc);

        // TODO: error handling
        let texarr_img = params.textures.get(texarr.handle()).unwrap();

        let pipeline_id = params.compute_pipelines.specialize(
            params.pipeline_cache,
            params.pipeline,
            MipGeneratorPipelineKey,
        );

        let mip_levels = texarr.mip_levels();

        let mut views = vec![];
        let mut view_sizes = vec![texarr.dims()];

        for mip_level in 0..mip_levels {
            let view = texarr_img.texture.create_view(&TextureViewDescriptor {
                label: Some("mip"),
                format: Some(texarr_img.texture_format),
                dimension: Some(TextureViewDimension::D2Array),
                base_mip_level: mip_level,
                mip_level_count: Some(1),
                aspect: TextureAspect::All,
                base_array_layer: 0,
                array_layer_count: Some(texarr.tex_array_len()),
            });

            views.push(view);
            if mip_level > 0 {
                view_sizes.push(view_sizes[(mip_level - 1) as usize] / 2);
            }
        }

        let mut bind_groups = vec![];
        for mip_level in 1..mip_levels {
            let previous_mip_view = &views[(mip_level - 1) as usize];
            let output_mip_view = &views[mip_level as usize];

            bind_groups.push(params.gpu.create_bind_group(
                "mipmap_generator_bind_group",
                &params.pipeline.bg_layout,
                &BindGroupEntries::sequential((previous_mip_view, output_mip_view)),
            ))
        }

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: "mipmap_generation_pass".into(),
            timestamp_writes: None,
        });

        let gpu_pipeline = params
            .pipeline_cache
            .get_compute_pipeline(pipeline_id)
            .unwrap();

        pass.set_pipeline(gpu_pipeline);

        for mip_level in 1..texarr.mip_levels() {
            let bind_group = &bind_groups[mip_level as usize];

            pass.set_bind_group(0, bind_group, &[]);

            // Get precomputed size
            let size = view_sizes[mip_level as usize];
            let workgroup_count: u32 = (size + WORKGROUP_SIZE_PER_DIM - 1) / WORKGROUP_SIZE_PER_DIM;

            pass.dispatch_workgroups(workgroup_count, workgroup_count, texarr.tex_array_len());
        }

        // wgpu automatically ends the compute pass when dropping it.
        drop(pass);

        let commands = encoder.finish();

        params.queue.submit([commands]);
    }
}

#[derive(Clone, Resource)]
pub struct MipGeneratorPipeline {
    bg_layout: BindGroupLayout,
}

impl FromWorld for MipGeneratorPipeline {
    fn from_world(world: &mut World) -> Self {
        let gpu = world.resource::<RenderDevice>();

        Self {
            bg_layout: gpu.create_bind_group_layout(
                "compute_mipmap_bg_layout",
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::COMPUTE,
                    (
                        texture_2d_array(TextureSampleType::Float { filterable: true }),
                        texture_storage_2d_array(TEXTURE_FORMAT, StorageTextureAccess::WriteOnly),
                    ),
                ),
            ),
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub struct MipGeneratorPipelineKey;

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
            layout: vec![self.bg_layout.clone()],
            shader_defs: defs,
        }
    }
}

pub struct MipGeneratorPlugin;

impl Plugin for MipGeneratorPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            MIPMAP_COMPUTE_SHADER_HANDLE,
            "mipmap.wgsl",
            Shader::from_wgsl
        );

        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<SpecializedComputePipelines<MipGeneratorPipeline>>();
        render_app.init_resource::<TexArrayMipGenerator>();
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<MipGeneratorPipeline>();
    }
}
