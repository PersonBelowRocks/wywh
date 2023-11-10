use crate::data::{
    registry::VoxelTextureRegistry,
    tile::Transparency,
    voxel::{BlockModel, SimpleStorage, Voxel, VoxelModel, VoxelProperties},
};

#[derive(Default, Copy, Clone, Debug)]
pub struct Void;

impl Voxel for Void {
    type Stored = SimpleStorage;

    fn model(&self, _textures: &VoxelTextureRegistry) -> Option<VoxelModel> {
        None
    }

    fn properties() -> VoxelProperties {
        VoxelProperties {
            transparency: Transparency::Transparent,
        }
    }

    fn from_stored(_storage: Self::Stored) -> Self {
        Self
    }

    fn store(&self) -> Self::Stored {
        SimpleStorage
    }
}

#[derive(Default, Copy, Clone, Debug)]
pub struct DebugVoxel;

impl Voxel for DebugVoxel {
    type Stored = SimpleStorage;

    fn model(&self, textures: &VoxelTextureRegistry) -> Option<VoxelModel> {
        let debug_texture = textures.get_id("textures/debug_texture.png").unwrap();
        let model = BlockModel::filled(debug_texture);
        Some(VoxelModel::Block(model))
    }

    fn from_stored(_storage: Self::Stored) -> Self {
        Self
    }

    fn store(&self) -> Self::Stored {
        SimpleStorage
    }

    fn properties() -> VoxelProperties {
        VoxelProperties {
            transparency: Transparency::Opaque,
        }
    }
}
