use bevy::{asset::LoadedFolder, prelude::*};

use crate::{data::registries::texture::TextureRegistryLoader, AppState};

use super::{
    registries::{
        error::TextureRegistryError, texture::TextureRegistry, variant::VariantRegistryLoader,
        Registries,
    },
    tile::Transparency,
    voxel::descriptor::{self, BlockModelDescriptor, VariantDescriptor, VoxelModelDescriptor},
};

#[derive(Resource, Default)]
pub struct VoxelTextureFolder(pub Handle<LoadedFolder>);

#[derive(Resource, Default)]
pub struct VoxelTextureAtlas(pub Handle<Image>);

pub(crate) fn load_textures(mut cmds: Commands, server: Res<AssetServer>) {
    cmds.insert_resource(VoxelTextureFolder(server.load_folder("textures")));
}

pub(crate) fn check_textures(
    mut next_state: ResMut<NextState<AppState>>,
    folder: Res<VoxelTextureFolder>,
    mut events: EventReader<AssetEvent<LoadedFolder>>,
) {
    for event in events.read() {
        if event.is_loaded_with_dependencies(&folder.0) {
            next_state.set(AppState::Finished);
        }
    }
}

fn create_texture_registry(
    folders: Res<Assets<LoadedFolder>>,
    mut images: ResMut<Assets<Image>>,
    texture_folder: Res<VoxelTextureFolder>,
) -> Result<TextureRegistry, TextureRegistryError> {
    // rust-analyzer can't infer this type for some reason so we have to explicitly state it
    let folder: &LoadedFolder = folders
        .get(&texture_folder.0)
        .ok_or(TextureRegistryError::VoxelTextureFolderNotLoaded)?;

    let mut registry_loader = TextureRegistryLoader::new();
    for handle in folder.handles.iter() {
        let Some(path) = handle.path() else {
            return Err(TextureRegistryError::CannotMakePath(handle.clone()));
        };

        let id = handle.id().try_typed::<Image>()?;
        let texture = images
            .get(id)
            .ok_or(TextureRegistryError::TextureDoesntExist(path.clone()))?;

        if texture.height() != texture.width() {
            return Err(TextureRegistryError::InvalidImageDimensions(path.clone()));
        }

        info!("Loaded texture at path: '{}'", path);
        registry_loader.register(path.to_string(), id);
    }

    Ok(registry_loader.build_registry(images.as_mut())?)
}

pub fn build_registries(world: &mut World) {
    let sysid = world.register_system(create_texture_registry);
    let result: Result<TextureRegistry, TextureRegistryError> = world.run_system(sysid).unwrap();

    let registries = Registries::new();

    let textures = match result {
        Ok(textures) => textures,
        Err(error) => {
            error!("Error when building texture registry: '{:?}'", error);
            panic!("Cannot build registries");
        }
    };

    world.insert_resource(VoxelTextureAtlas(textures.atlas_texture().clone()));

    let builtin_variants = [
        (
            "void",
            VariantDescriptor {
                transparency: Transparency::Transparent,
                model: None,
            },
        ),
        (
            "debug",
            VariantDescriptor {
                transparency: Transparency::Opaque,
                model: Some(VoxelModelDescriptor::Block(BlockModelDescriptor::filled(
                    "textures\\debug_texture.png".into(),
                ))),
            },
        ),
    ];

    let mut loader = VariantRegistryLoader::new();

    for (label, descriptor) in builtin_variants {
        loader.register(label, descriptor);
    }

    let variants = loader.build_registry(&textures);
    if let Err(error) = variants {
        error!("Error building variant registry: '{error}'");
        panic!();
    }

    registries.add_registry(textures);
    registries.add_registry(variants.unwrap());

    world.insert_resource(registries);
}
