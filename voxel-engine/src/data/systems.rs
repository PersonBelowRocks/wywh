use std::{ffi::OsStr, path::PathBuf, sync::Arc};

use bevy::{asset::LoadedFolder, prelude::*};

use crate::{data::registries::texture::TextureRegistryLoader, AppState};

use super::{
    registries::{
        error::TextureRegistryError, texture::TextureRegistry, variant::VariantRegistryLoader,
        Registries,
    },
    tile::Transparency,
    variant_file_loader::VariantFileLoader,
    voxel::descriptor::VariantDescriptor,
};

#[derive(Resource, Default)]
pub struct VoxelTextureFolder(pub Handle<LoadedFolder>);

#[derive(Resource, Default)]
pub struct VoxelColorTextureAtlas(pub Handle<Image>);

#[derive(Resource, Deref, dm::Constructor)]
pub struct VariantFolders(Arc<Vec<PathBuf>>);

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
        let label = path
            .path()
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or(TextureRegistryError::BadFileName(path.clone()))?;
        registry_loader.register(label.to_string(), id, None);
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

    world.insert_resource(VoxelColorTextureAtlas(textures.color_texture().clone()));
    let variant_folders = world.get_resource::<VariantFolders>().unwrap();

    let mut file_loader = VariantFileLoader::new();
    let mut registry_loader = VariantRegistryLoader::new();

    registry_loader.register(
        "void",
        VariantDescriptor {
            transparency: Transparency::Transparent,
            model: None,
        },
    );

    for folder in variant_folders.iter() {
        if let Err(err) = file_loader.load_folder(folder) {
            let path = folder.as_path().to_string_lossy();
            error!("Error while loading variant folder at path '{path}': '{err}'");
        }
    }

    for label in file_loader.labels() {
        match file_loader.parse(label) {
            Ok(descriptor) => {
                info!("Registering variant with label '{label}'");
                registry_loader.register(label, descriptor);
            }
            Err(error) => error!("Couldn't parse variant descriptor with label '{label}': {error}"),
        }
    }

    let variants = registry_loader.build_registry(&textures);

    if let Err(error) = variants {
        error!("Error building variant registry: '{error}'");
        panic!();
    }

    registries.add_registry(textures);
    registries.add_registry(variants.unwrap());

    world.insert_resource(registries);
}
