extern crate thiserror as te;

use asset::MippedArrayTexture;
use bevy::{
    asset::load_internal_asset,
    prelude::*,
    render::{
        render_asset::{RenderAssetPlugin, RenderAssets},
        render_resource::{Extent3d, SpecializedComputePipelines, TextureDimension, TextureFormat},
        texture::TextureFormatPixelInfo,
        Render, RenderApp, RenderSet,
    },
    utils::Uuid,
};

mod error;
pub use error::*;

use crate::mipmap::{MipGeneratorPipeline, MipGeneratorPipelineMeta, MIPMAP_COMPUTE_SHADER_HANDLE};

pub mod asset;
pub use asset::*;
pub mod mipmap;

pub const TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;
pub const STORAGE_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

#[derive(Default)]
pub struct MippedArrayTexturePlugin {
    /// Should the prepared `GpuImage`s that we create be injected into Bevy's default `RenderAssets<Image>`?
    /// This option should NEVER be used outside of testing stuff
    pub inject_into_render_images: bool,
}

impl Plugin for MippedArrayTexturePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            MIPMAP_COMPUTE_SHADER_HANDLE,
            "mipmap.wgsl",
            Shader::from_wgsl
        );

        app.init_asset::<MippedArrayTexture>();
        app.add_plugins(RenderAssetPlugin::<MippedArrayTexture>::default());

        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<SpecializedComputePipelines<MipGeneratorPipeline>>();

        if self.inject_into_render_images {
            render_app.add_systems(
                Render,
                inject_array_textures_into_render_images.in_set(RenderSet::PrepareAssets),
            );
        }
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<MipGeneratorPipelineMeta>();
    }
}

fn inject_array_textures_into_render_images(
    array_textures: Res<RenderAssets<MippedArrayTexture>>,
    mut images: ResMut<RenderAssets<Image>>,
) {
    for (id, image) in array_textures.iter() {
        // this is so sketchy but bevy's AsBindGroup trait only gives you access to RenderAssets<Image>
        // so to make it easier for myself to test this crap we do this
        images.insert(id.untyped().typed_unchecked::<Image>(), image.clone());
    }
}

/// Builder for a texture array
#[derive(Clone, Debug)]
pub struct MippedArrayTextureBuilder {
    handles: Vec<AssetId<Image>>,
    empty_pixel_data: Vec<u8>,
    format: TextureFormat,
    dims: u32,
}

impl MippedArrayTextureBuilder {
    pub fn new(dims: u32) -> Self {
        Self::new_with_format(dims, vec![0; 4], TextureFormat::Rgba8UnormSrgb)
    }

    fn new_with_format(dims: u32, empty_pixel_data: Vec<u8>, format: TextureFormat) -> Self {
        Self {
            handles: Vec::new(),
            empty_pixel_data,
            format,
            dims,
        }
    }

    /// Add an image to the builder. `handle` is the image handle, `images` is the `Assets` instance where the image the handle points to is stored.
    /// Returns an error (and doesn't add the image to the builder) if the image doesn't exist in the provided `Assets` or if the image dimensions aren't the
    // same as what the builder expects.
    pub fn add_image(
        &mut self,
        asset_id: AssetId<Image>,
        images: &Assets<Image>,
    ) -> Result<usize, TextureArrayBuilderError> {
        let image = images
            .get(asset_id)
            .ok_or_else(|| TextureArrayBuilderError::ImageNotFound(asset_id.clone()))?;
        let extent = image.texture_descriptor.size;

        if extent.width != self.dims || extent.height != self.dims {
            return Err(TextureArrayBuilderError::IncorrectImageDimensions {
                ed: self.dims,
                x: extent.width,
                y: extent.height,
            });
        }

        let idx = self.handles.len();
        self.handles.push(asset_id);
        Ok(idx)
    }

    pub fn finish(
        &self,
        images: &mut Assets<Image>,
        array_textures: &mut Assets<MippedArrayTexture>,
    ) -> Result<Handle<MippedArrayTexture>, TextureArrayBuilderError> {
        let total_imgs = self.handles.len();

        let arr_tex_dims = Extent3d {
            width: self.dims,
            height: self.dims * (total_imgs as u32),
            depth_or_array_layers: 1,
        };

        let mut arr_texture = Image::new_fill(
            arr_tex_dims,
            TextureDimension::D2,
            &self.empty_pixel_data,
            self.format,
        );

        // arr_texture.texture_descriptor.mip_level_count = self.dims.ilog2();

        for (idx, asset_id) in self.handles.iter().enumerate() {
            // The image might have been removed from the assets by the time that finish() is run, so we handle the error again here so we avoid panicking in a library.
            let source = images
                .get(*asset_id)
                .ok_or_else(|| TextureArrayBuilderError::ImageNotFound(asset_id.clone()))?;

            // Finally we perform the actual copy.
            self.copy_to_arr_tex(&mut arr_texture, source, idx as _)?;
        }

        arr_texture.reinterpret_stacked_2d_as_array(total_imgs as _);
        let asset = MippedArrayTexture {
            label: None,
            image: arr_texture,
            array_layers: total_imgs as _,
            dims: self.dims,
        };

        let manual_id = AssetId::Uuid {
            uuid: Uuid::new_v4(),
        };
        array_textures.insert(manual_id, asset);

        Ok(Handle::Weak(manual_id))
    }

    fn copy_to_arr_tex(
        &self,
        arr_texture: &mut Image,
        source: &Image,
        idx: u32,
    ) -> Result<(), TextureArrayBuilderError> {
        let extent = source.texture_descriptor.size;
        if extent.width != self.dims || extent.height != self.dims {
            return Err(TextureArrayBuilderError::IncorrectImageDimensions {
                ed: self.dims,
                x: extent.width,
                y: extent.height,
            });
        }

        // This code is largely taken from the crate "bevy_tile_atlas" by https://github.com/MrGVSV
        // link to function in repo: https://github.com/MrGVSV/bevy_tile_atlas/blob/main/src/tile_atlas.rs#L289

        let tile_size = UVec2::splat(self.dims);
        let rect_width = tile_size.x as usize;
        let rect_height = tile_size.y as usize;
        let rect_x = 0usize;
        let rect_y = (idx as usize) * tile_size.y as usize;
        let atlas_width = arr_texture.texture_descriptor.size.width as usize;
        let format_size = arr_texture.texture_descriptor.format.pixel_size();

        for (texture_y, bound_y) in (rect_y..rect_y + rect_height).enumerate() {
            let begin = (bound_y * atlas_width + rect_x) * format_size;
            let end = begin + rect_width * format_size;
            let texture_begin = texture_y * rect_width * format_size;
            let texture_end = texture_begin + rect_width * format_size;

            arr_texture.data[begin..end].copy_from_slice(&source.data[texture_begin..texture_end]);
        }

        Ok(())
    }
}
