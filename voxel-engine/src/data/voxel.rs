use std::any::type_name;

use bevy::render::render_resource::TextureId;

use crate::util::FaceMap;

use super::tile::Transparency;

pub trait Voxel: Default {
    fn label() -> &'static str {
        type_name::<Self>()
    }

    fn properties() -> VoxelProperties;
}

#[derive(Clone)]
pub struct VoxelProperties {
    transparency: Transparency,
    model: Option<VoxelModel>,
}

#[derive(Copy, Clone)]
pub struct BlockModel {
    textures: FaceMap<TextureId>,
}

#[derive(Copy, Clone)]
pub enum VoxelModel {
    Block(BlockModel),
}
