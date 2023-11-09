use bevy::prelude::*;

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
        info!("Texture '{lbl}' has dimensions {rect:?}");
    }

    let atlas = voxel_texture_registry.atlas_texture();

    cmds.insert_resource(VoxelTextureAtlas(atlas));

    let mut voxel_reg_builder = VoxelRegistryBuilder::new(&voxel_texture_registry);
    voxel_reg_builder.register::<defaults::Void>();
    voxel_reg_builder.register::<defaults::DebugVoxel>();

    let voxel_registry = voxel_reg_builder.finish();
    let registries = Registries::new(voxel_texture_registry, voxel_registry);
    cmds.insert_resource(registries);
}
