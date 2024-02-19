use bevy::{
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    prelude::*,
    render::{
        render_asset::{PrepareAssetError, RenderAsset},
        render_resource::{
            AddressMode, BindGroupEntries, CommandEncoderDescriptor, ComputePassDescriptor,
            Extent3d, FilterMode, ImageCopyTexture, ImageDataLayout, Origin3d, PipelineCache,
            SamplerDescriptor, Texture, TextureAspect, TextureDescriptor, TextureDimension,
            TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};

use crate::{
    mipmap::{MipGeneratorPipelineMeta, WORKGROUP_SIZE_PER_DIM},
    STORAGE_TEXTURE_FORMAT, TEXTURE_FORMAT,
};

#[derive(Asset, Clone, TypePath)]
pub struct MippedArrayTexture {
    pub label: Option<&'static str>,
    pub image: Image,
    pub dims: u32,
    pub array_layers: u32,
}

impl MippedArrayTexture {
    pub fn extent(&self) -> Extent3d {
        Extent3d {
            width: self.dims,
            height: self.dims,
            depth_or_array_layers: self.array_layers,
        }
    }

    pub fn mipmap_levels(&self) -> u32 {
        self.dims.ilog2()
    }
}

fn create_array_texture_with_filled_mip_level_0(
    asset: &MippedArrayTexture,
    gpu: &RenderDevice,
    queue: &RenderQueue,
) -> Texture {
    let desc = TextureDescriptor {
        label: asset.label,
        size: asset.extent(),
        mip_level_count: asset.mipmap_levels(),
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: STORAGE_TEXTURE_FORMAT,
        usage: TextureUsages::COPY_DST
            | TextureUsages::STORAGE_BINDING
            | TextureUsages::TEXTURE_BINDING,
        view_formats: &[TEXTURE_FORMAT],
    };

    let texture = gpu.create_texture(&desc);

    let block_size = TEXTURE_FORMAT.block_size(None).unwrap_or(4);
    let (block_width, block_height) = desc.format.block_dimensions();
    let layers = asset.array_layers;

    let mut binary_offset = 0;
    for layer in 0..layers {
        let mut mip_size = desc.mip_level_size(0).unwrap();
        // copying layers separately
        mip_size.depth_or_array_layers = 1;
        let mip_physical = mip_size.physical_size(TEXTURE_FORMAT);

        // All these calculations are performed on the physical size as that's the
        // data that exists in the buffer.
        let width_blocks = mip_physical.width / block_width;
        let height_blocks = mip_physical.height / block_height;

        let bytes_per_row = width_blocks * block_size;
        let data_size = bytes_per_row * height_blocks * mip_size.depth_or_array_layers;

        let end_offset = binary_offset + data_size as usize;

        queue.write_texture(
            ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: TextureAspect::All,
            },
            &asset.image.data[binary_offset..end_offset],
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height_blocks),
            },
            mip_physical,
        );

        binary_offset = end_offset;
    }

    texture
}

fn create_mip_view_sizes(mip_levels: u32, dims: u32) -> Vec<u32> {
    assert!(mip_levels > 1);

    let mut sizes = vec![dims];

    for mip in 1..mip_levels {
        sizes.push(sizes[(mip - 1) as usize] / 2);
    }

    sizes
}

fn create_mip_views(mip_levels: u32, texture: &Texture, array_layers: u32) -> Vec<TextureView> {
    let mut views = vec![];

    for mip in 0..mip_levels {
        views.push(texture.create_view(&TextureViewDescriptor {
            label: Some("mip"),
            format: Some(TEXTURE_FORMAT),
            dimension: Some(TextureViewDimension::D2Array),
            aspect: TextureAspect::All,
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(array_layers),
        }))
    }

    views
}

fn create_mip_storage_views(
    mip_levels: u32,
    texture: &Texture,
    array_layers: u32,
) -> Vec<TextureView> {
    let mut views = vec![];

    for mip in 0..mip_levels {
        views.push(texture.create_view(&TextureViewDescriptor {
            label: Some("mip_storage"),
            format: Some(STORAGE_TEXTURE_FORMAT),
            dimension: Some(TextureViewDimension::D2Array),
            aspect: TextureAspect::All,
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(array_layers),
        }))
    }

    views
}

impl RenderAsset for MippedArrayTexture {
    type ExtractedAsset = Self;
    type PreparedAsset = GpuImage;

    type Param = (
        SRes<RenderDevice>,
        SRes<RenderQueue>,
        SRes<MipGeneratorPipelineMeta>,
        SRes<PipelineCache>,
    );

    fn extract_asset(&self) -> Self::ExtractedAsset {
        self.clone()
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        param: &mut SystemParamItem<Self::Param>,
    ) -> Result<Self::PreparedAsset, PrepareAssetError<Self::ExtractedAsset>> {
        let (gpu, queue, pipeline_meta, pipeline_cache) = param;
        let mip_levels = asset.mipmap_levels();

        let texture = create_array_texture_with_filled_mip_level_0(&asset, gpu, queue);

        let views = create_mip_views(mip_levels, &texture, asset.array_layers);
        let storage_views = create_mip_storage_views(mip_levels, &texture, asset.array_layers);

        let view_sizes = create_mip_view_sizes(mip_levels, asset.dims);

        let mut bind_groups = vec![];
        for mip_level in 1..mip_levels {
            let previous_mip_view = &views[(mip_level - 1) as usize];
            let output_mip_view = &storage_views[mip_level as usize];

            bind_groups.push(gpu.create_bind_group(
                "mipmap_generator_bind_group",
                &pipeline_meta.bg_layout,
                &BindGroupEntries::sequential((previous_mip_view, output_mip_view)),
            ))
        }

        let gpu_pipeline = pipeline_cache
            .get_compute_pipeline(pipeline_meta.pipeline_id)
            .unwrap();

        let mut encoder = gpu.create_command_encoder(&CommandEncoderDescriptor {
            label: "mipmap_generation_cmd_encoder".into(),
        });

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: "mipmap_generation_pass".into(),
            timestamp_writes: None,
        });

        pass.set_pipeline(gpu_pipeline);
        for mip_level in 1..mip_levels {
            let bind_group = &bind_groups[mip_level as usize - 1];

            pass.set_bind_group(0, bind_group, &[]);

            // Get precomputed size
            let size = view_sizes[mip_level as usize];
            let workgroup_count: u32 = (size + WORKGROUP_SIZE_PER_DIM - 1) / WORKGROUP_SIZE_PER_DIM;

            pass.dispatch_workgroups(workgroup_count, workgroup_count, asset.array_layers);
        }

        // wgpu automatically ends the compute pass when dropping it.
        drop(pass);

        let commands = encoder.finish();
        queue.submit([commands]);

        let main_view = texture.create_view(&TextureViewDescriptor {
            label: Some("mipped_array_texture_main_view"),
            format: Some(TEXTURE_FORMAT),
            dimension: Some(TextureViewDimension::D2Array),
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None, // Some(mip_levels),
            base_array_layer: 0,
            array_layer_count: None, // Some(asset.array_layers),
        });

        Ok(GpuImage {
            texture,
            texture_view: main_view,
            texture_format: STORAGE_TEXTURE_FORMAT,
            sampler: gpu.create_sampler(&SamplerDescriptor {
                label: Some("mipped_array_texture_sampler"),
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                address_mode_w: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Nearest,
                min_filter: FilterMode::Nearest,
                mipmap_filter: FilterMode::Linear,
                lod_min_clamp: 0.0,
                lod_max_clamp: 1.0,
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
            }),
            size: UVec2::splat(asset.dims).as_vec2(),
            mip_level_count: mip_levels,
        })
    }
}
