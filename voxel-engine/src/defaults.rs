use crate::data::{
    registry::VoxelTextureRegistry,
    tile::Transparency,
    voxel::{BlockModel, Voxel, VoxelModel, VoxelProperties},
};

#[derive(Default, Copy, Clone, Debug)]
pub struct Void;

impl Voxel for Void {
    fn model(textures: &VoxelTextureRegistry) -> Option<VoxelModel> {
        None
    }

    fn properties() -> VoxelProperties {
        VoxelProperties {
            transparency: Transparency::Transparent,
        }
    }
}

#[derive(Default, Copy, Clone, Debug)]
pub struct DebugVoxel;

impl Voxel for DebugVoxel {
    fn model(textures: &VoxelTextureRegistry) -> Option<VoxelModel> {
        let debug_texture = textures.get_id("textures/debug_texture.png").unwrap();
        let model = BlockModel::filled(debug_texture);
        Some(VoxelModel::Block(model))
    }

    fn properties() -> VoxelProperties {
        VoxelProperties {
            transparency: Transparency::Opaque,
        }
    }
}
