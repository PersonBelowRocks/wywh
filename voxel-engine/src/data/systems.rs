use std::{path::PathBuf, sync::Arc};

use bevy::{
    asset::LoadedFolder,
    ecs::system::SystemParam,
    prelude::*,
    render::{render_asset::RenderAssets, texture::GpuImage},
};
use mip_texture_array::asset::MippedArrayTexture;

use crate::{
    data::{registries::texture::TextureRegistryLoader, resourcepath::ResourcePath},
    AppState,
};

use super::{
    error::TextureAtlasesGetAssetError,
    registries::{
        error::TextureRegistryError,
        texture::{TexregFaces, TextureRegistry},
        variant::VariantRegistryLoader,
        Registries,
    },
    resourcepath::rpath,
    tile::Transparency,
    variant_file_loader::VariantFileLoader,
    voxel::descriptor::{BlockDescriptor, VariantDescriptor},
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

#[derive(Resource, Default, Clone)]
pub struct VoxelColorArrayTexture(pub Handle<MippedArrayTexture>);

#[derive(Resource, Default, Clone)]
pub struct VoxelNormalArrayTexture(pub Handle<MippedArrayTexture>);

#[derive(SystemParam)]
pub struct ArrayTextureHandles<'w> {
    pub color: Option<Res<'w, VoxelColorArrayTexture>>,
    pub normal: Option<Res<'w, VoxelNormalArrayTexture>>,
}

pub struct ArrayTextures<'a> {
    pub color: &'a MippedArrayTexture,
    pub normal: &'a MippedArrayTexture,
}

pub struct GpuArrayTextures<'a> {
    pub color: &'a GpuImage,
    pub normal: &'a GpuImage,
}

impl<'w> ArrayTextureHandles<'w> {
    pub fn get_assets<'a>(
        &self,
        assets: &'a Assets<MippedArrayTexture>,
    ) -> Result<ArrayTextures<'a>, TextureAtlasesGetAssetError> {
        let Some(handle) = self.color.as_deref() else {
            return Err(TextureAtlasesGetAssetError::MissingColorHandle);
        };

        let color = assets
            .get(&handle.0)
            .ok_or(TextureAtlasesGetAssetError::MissingColor)?;

        let Some(handle) = self.normal.as_deref() else {
            return Err(TextureAtlasesGetAssetError::MissingNormalHandle);
        };

        let normal = assets
            .get(&handle.0)
            .ok_or(TextureAtlasesGetAssetError::MissingNormal)?;

        Ok(ArrayTextures { color, normal })
    }

    pub fn get_render_assets<'a>(
        &self,
        assets: &'a RenderAssets<MippedArrayTexture>,
    ) -> Result<GpuArrayTextures<'a>, TextureAtlasesGetAssetError> {
        let Some(handle) = self.color.as_deref() else {
            return Err(TextureAtlasesGetAssetError::MissingColorHandle);
        };

        let color = assets
            .get(&handle.0)
            .ok_or(TextureAtlasesGetAssetError::MissingColor)?;

        let Some(handle) = self.normal.as_deref() else {
            return Err(TextureAtlasesGetAssetError::MissingNormalHandle);
        };

        let normal = assets
            .get(&handle.0)
            .ok_or(TextureAtlasesGetAssetError::MissingNormal)?;

        Ok(GpuArrayTextures { color, normal })
    }
}

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
    mut array_textures: ResMut<Assets<MippedArrayTexture>>,
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

    Ok(registry_loader.build_registry(images.as_ref(), &mut array_textures)?)
}

pub fn build_registries(world: &mut World) {
    let create_texreg_sysid = world.register_system(create_texture_registry);

    let result: Result<TextureRegistry, TextureRegistryError> =
        world.run_system(create_texreg_sysid).unwrap();

    let registries = Registries::new();

    let textures = match result {
        Ok(textures) => textures,
        Err(error) => {
            error!("Error when building texture registry: '{:?}'", error);
            panic!("Cannot build registries");
        }
    };

    world.insert_resource(VoxelColorArrayTexture(textures.color_texture().clone()));
    world.insert_resource(VoxelNormalArrayTexture(textures.normal_texture().clone()));
    world.insert_resource(TexregFaces(textures.face_texture_buffer()));

    let variant_folders = world.get_resource::<VariantFolders>().unwrap();

    let mut file_loader = VariantFileLoader::new();
    let mut registry_loader = VariantRegistryLoader::new();

    registry_loader.register(
        rpath("void"),
        BlockDescriptor {
            transparency: Transparency::Transparent,
            directions: Default::default(),
            default: Default::default(),
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
                registry_loader.register(label.clone(), todo!());
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
