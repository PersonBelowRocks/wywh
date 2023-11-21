use bevy::{prelude::*, render::texture::ImageSampler};

use crate::defaults;

use super::{
    registry::{Registries, VoxelRegistryBuilder, VoxelTextureRegistry},
    textures::{VoxelTextureAtlas, VoxelTextureRegistryError},
};

pub(crate) fn create_registries(
    In(result): In<Result<VoxelTextureRegistry, VoxelTextureRegistryError>>,

    mut cmds: Commands,
    mut textures: ResMut<Assets<Image>>,
) {
    if let Err(error) = result {
        error!("Encountered error when building voxel texture registry: {error}");
        panic!("Fatal error!")
    }

    let voxel_texture_registry = result.unwrap();

    for (lbl, rect) in voxel_texture_registry.iter_rects() {
        let min = rect.min;
        let max = rect.max;

        info!("Texture '{lbl}' has dimensions {min}..{max}");
    }

    let atlas = voxel_texture_registry.atlas_texture();
    let voxel_tex_atlas_img = textures.get_mut(atlas.clone()).unwrap();
    voxel_tex_atlas_img.sampler = ImageSampler::nearest();

    cmds.insert_resource(VoxelTextureAtlas(atlas));

    let mut voxel_reg_builder = VoxelRegistryBuilder::new(&voxel_texture_registry);
    voxel_reg_builder.register::<defaults::Void>();
    voxel_reg_builder.register::<defaults::DebugVoxel>();

    let voxel_registry = voxel_reg_builder.finish();
    let registries = Registries::new(voxel_texture_registry, voxel_registry);
    cmds.insert_resource(registries);
}
