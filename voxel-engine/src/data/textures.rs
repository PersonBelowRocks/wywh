use bevy::{
    asset::{AssetPath, LoadedFolder},
    prelude::*,
    sprite::TextureAtlasBuilderError,
};

use crate::AppState;

use super::registry::{VoxelTextureRegistry, VoxelTextureRegistryBuilder};

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

#[derive(te::Error, Debug)]
pub enum VoxelTextureRegistryError {
    #[error("Could not get path for handle {0:?}")]
    CannotMakePath(UntypedHandle),
    #[error("{0:?} did not resolve to an `Image` asset")]
    BadAssetType(AssetPath<'static>),
    #[error("{0:?} was not a square image")]
    InvalidImageDimensions(AssetPath<'static>),
    #[error("{0}")]
    AtlasBuilderError(#[from] TextureAtlasBuilderError),
}

pub(crate) fn create_voxel_texture_registry(
    voxel_textures: Res<VoxelTextureFolder>,
    mut textures: ResMut<Assets<Image>>,
    folders: Res<Assets<LoadedFolder>>,
) -> Result<VoxelTextureRegistry, VoxelTextureRegistryError> {
    let folder = folders.get(&voxel_textures.0).unwrap();
    let mut builder = VoxelTextureRegistryBuilder::new();

    for handle in folder.handles.iter() {
        let Some(path) = handle.path() else {
            return Err(VoxelTextureRegistryError::CannotMakePath(handle.clone()));
        };

        let id = handle.id().typed_unchecked::<Image>();
        if let Some(tex) = textures.get(id) {
            if tex.height() != tex.width() {
                return Err(VoxelTextureRegistryError::InvalidImageDimensions(
                    path.clone(),
                ));
            }
            info!("Loaded texture at path: '{}'", path);
            builder.add_texture(id, tex, path.to_string());
        } else {
            return Err(VoxelTextureRegistryError::BadAssetType(path.clone()));
        }
    }

    Ok(builder.finish(textures.as_mut())?)
}
