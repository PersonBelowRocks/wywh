use bevy::render::extract_resource::ExtractResource;

use crate::data::systems::{VoxelColorArrayTexture, VoxelNormalArrayTexture};

impl ExtractResource for VoxelColorArrayTexture {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl ExtractResource for VoxelNormalArrayTexture {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}
