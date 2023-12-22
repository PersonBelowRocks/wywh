use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{asset::LoadedFolder, prelude::*};

use crate::{
    data::{registries::texture::TextureRegistryLoader, resourcepath::ResourcePath},
    AppState,
};

use super::{
    registries::{
        error::TextureRegistryError, texture::TextureRegistry, variant::VariantRegistryLoader,
        Registries,
    },
    tile::Transparency,
    variant_file_loader::VariantFileLoader,
    voxel::descriptor::VariantDescriptor,
};

pub static TEXTURE_FOLDER_NAME: &'static str = "textures";
pub static NORMALMAPS_FOLDER_NAME: &'static str = "normalmaps";

#[derive(Resource, Default)]
pub struct VoxelTextureFolder {
    pub handle: Handle<LoadedFolder>,
    pub loaded: bool,
}

#[derive(Resource, Default)]
pub struct VoxelNormalMapFolder {
    pub handle: Handle<LoadedFolder>,
    pub loaded: bool,
}

#[derive(Resource, Default)]
pub struct VoxelColorTextureAtlas(pub Handle<Image>);

#[derive(Resource, Default)]
pub struct VoxelNormalTextureAtlas(pub Handle<Image>);

#[derive(Resource, Deref, dm::Constructor)]
pub struct VariantFolders(Arc<Vec<PathBuf>>);

pub(crate) fn load_textures(mut cmds: Commands, server: Res<AssetServer>) {
    cmds.insert_resource(VoxelTextureFolder {
        handle: server.load_folder(TEXTURE_FOLDER_NAME),
        loaded: false,
    });
    cmds.insert_resource(VoxelNormalMapFolder {
        handle: server.load_folder(NORMALMAPS_FOLDER_NAME),
        loaded: false,
    });
}

pub(crate) fn check_textures(
    mut next_state: ResMut<NextState<AppState>>,
    mut texture_folder: ResMut<VoxelTextureFolder>,
    mut normalmap_folder: ResMut<VoxelNormalMapFolder>,
    mut events: EventReader<AssetEvent<LoadedFolder>>,
) {
    for event in events.read() {
        if event.is_loaded_with_dependencies(&texture_folder.handle) {
            texture_folder.loaded = true;
        }

        if event.is_loaded_with_dependencies(&normalmap_folder.handle) {
            normalmap_folder.loaded = true;
        }
    }

    if texture_folder.loaded && normalmap_folder.loaded {
        next_state.set(AppState::Finished);
    }
}

fn create_texture_registry(
    folders: Res<Assets<LoadedFolder>>,
    mut images: ResMut<Assets<Image>>,
    texture_folder: Res<VoxelTextureFolder>,
    normalmap_folder: Res<VoxelNormalMapFolder>,
) -> Result<TextureRegistry, TextureRegistryError> {
    // rust-analyzer can't infer this type for some reason so we have to explicitly state it
    let texture_folder: &LoadedFolder = folders
        .get(&texture_folder.handle)
        .ok_or(TextureRegistryError::VoxelTextureFolderNotLoaded)?;

    let normalmap_folder: &LoadedFolder = folders
        .get(&normalmap_folder.handle)
        .ok_or(TextureRegistryError::VoxelNormalMapFolderNotLoaded)?;

    let textures = {
        let mut map = hb::HashMap::<ResourcePath, AssetId<Image>>::new();

        for handle in texture_folder.handles.iter() {
            let Some(asset_path) = handle.path() else {
                return Err(TextureRegistryError::CannotMakePath(handle.clone()));
            };

            let id = handle.id().try_typed::<Image>()?;

            let path = asset_path.path().strip_prefix(TEXTURE_FOLDER_NAME).unwrap();
            map.insert(ResourcePath::try_from(path)?, id);
        }

        map
    };

    let normalmaps = {
        let mut map = hb::HashMap::<ResourcePath, AssetId<Image>>::new();

        for handle in normalmap_folder.handles.iter() {
            let Some(asset_path) = handle.path() else {
                return Err(TextureRegistryError::CannotMakePath(handle.clone()));
            };

            let id = handle.id().try_typed::<Image>()?;

            let path = asset_path
                .path()
                .strip_prefix(NORMALMAPS_FOLDER_NAME)
                .unwrap();
            map.insert(ResourcePath::try_from(path)?, id);
        }

        map
    };

    let mut registry_loader = TextureRegistryLoader::new();

    for (rpath, &texture) in textures.iter() {
        let normalmap = normalmaps.get(rpath).copied();

        registry_loader.register(rpath.clone(), texture, normalmap)
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
    world.insert_resource(VoxelNormalTextureAtlas(textures.normal_texture().clone()));

    let variant_folders = world.get_resource::<VariantFolders>().unwrap();

    let mut file_loader = VariantFileLoader::new();
    let mut registry_loader = VariantRegistryLoader::new();

    registry_loader.register(
        ResourcePath::try_from("void").unwrap(),
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
                registry_loader.register(label.clone(), descriptor);
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
