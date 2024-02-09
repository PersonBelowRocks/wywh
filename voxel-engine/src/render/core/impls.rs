use bevy::render::extract_resource::ExtractResource;

use crate::data::systems::{VoxelColorTextureAtlas, VoxelNormalTextureAtlas};

impl ExtractResource for VoxelColorTextureAtlas {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl ExtractResource for VoxelNormalTextureAtlas {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}
