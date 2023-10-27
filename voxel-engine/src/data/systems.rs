use bevy::prelude::*;

use crate::defaults;

use super::registry::{
    Registries, TextureId, VoxelRegistryBuilder, VoxelTextureAtlas,
    TempVoxelTextureRegistry, VoxelTextureRegistryBuilder, VoxelTextureAtlasManager,
};

pub(crate) fn load_textures(mut cmds: Commands, server: Res<AssetServer>) {
    let mut loader = VoxelTextureRegistryBuilder::new(server.as_ref());

    loader.add_texture("textures/debug_texture.png");

    cmds.insert_resource(loader.to_registry());
}

pub(crate) fn create_registries(
    In(result): In<Result<VoxelTextureRegistry, VoxelTextureRegistryError>>,

    mut cmds: Commands,
    mut temp_v_tex_reg: ResMut<TempVoxelTextureRegistry>,
    mut textures: ResMut<Assets<Image>>,
) {
    if let Err(error) = result {
        error!("Encountered error when building voxel texture registry: {error}");
        panic!("Fatal error!")
    }

    let voxel_texture_registry = result.unwrap();

    let atlas = texture_registry.atlas_texture();

    cmds.insert_resource(atlas);

    let mut voxel_reg_builder = VoxelRegistryBuilder::new(&texture_registry);
    voxel_reg_builder.register::<defaults::Void>();
    voxel_reg_builder.register::<defaults::DebugVoxel>();

    let voxel_registry = voxel_reg_builder.finish();
    let registries = Registries::new(texture_registry, voxel_registry);
    cmds.insert_resource(registries);
    cmds.remove_resource::<TempVoxelTextureRegistry>();
}
