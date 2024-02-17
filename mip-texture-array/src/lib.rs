extern crate thiserror as te;

use bevy::{
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::TextureFormatPixelInfo,
    },
};

mod error;
pub use error::*;

/// Builder for a texture array
#[derive(Clone, Debug)]
pub struct TextureArrayBuilder {
    handles: Vec<Handle<Image>>,
    empty_pixel_data: Vec<u8>,
    format: TextureFormat,
    dims: u32,
}

impl TextureArrayBuilder {
    pub fn new(dims: u32) -> Self {
        Self::new_with_format(dims, vec![0; 4], TextureFormat::Rgba8UnormSrgb)
    }

    pub fn new_with_format(dims: u32, empty_pixel_data: Vec<u8>, format: TextureFormat) -> Self {
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
        handle: Handle<Image>,
        images: &Assets<Image>,
    ) -> Result<usize, TextureArrayBuilderError> {
        let image = images
            .get(&handle)
            .ok_or_else(|| TextureArrayBuilderError::ImageNotFound(handle.clone()))?;
        let extent = image.texture_descriptor.size;

        if extent.width != self.dims || extent.height != self.dims {
            return Err(TextureArrayBuilderError::IncorrectImageDimensions {
                ed: self.dims,
                x: extent.width,
                y: extent.height,
            });
        }

        let idx = self.handles.len();
        self.handles.push(handle);
        Ok(idx)
    }

    pub fn finish(
        &self,
        images: &mut Assets<Image>,
    ) -> Result<TextureArray, TextureArrayBuilderError> {
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

        for (idx, handle) in self.handles.iter().enumerate() {
            // The image might have been removed from the assets by the time that finish() is run, so we handle the error again here so we avoid panicking in a library.
            let source = images
                .get(handle)
                .ok_or_else(|| TextureArrayBuilderError::ImageNotFound(handle.clone()))?;

            // Finally we perform the actual copy.
            self.copy_to_arr_tex(&mut arr_texture, source, idx as _)?;
        }

        arr_texture.reinterpret_stacked_2d_as_array(total_imgs as _);

        let texture_array_handle = images.add(arr_texture);

        Ok(TextureArray {
            handle: texture_array_handle,
            images: total_imgs as _,
            dims: self.dims,
        })
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

#[derive(Clone, Debug)]
pub struct TextureArray {
    handle: Handle<Image>,
    images: u32,
    dims: u32,
}

impl TextureArray {
    pub fn handle(&self) -> &Handle<Image> {
        &self.handle
    }

    pub fn images(&self) -> u32 {
        self.images
    }

    pub fn dims(&self) -> u32 {
        self.dims
    }
}
