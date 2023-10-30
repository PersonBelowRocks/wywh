use std::sync::Arc;

use bevy::{asset::AssetPath, prelude::*, sprite::TextureAtlasBuilderError};

use super::{
    error::TextureLoadingError,
    textures::VoxelTextureRegistryError,
    tile::VoxelId,
    voxel::{Voxel, VoxelModel, VoxelProperties},
};

#[derive(Clone, Resource)]
pub struct Registries {
    pub textures: Arc<VoxelTextureRegistry>,
    pub voxels: Arc<VoxelRegistry>,
}

impl Registries {
    pub fn new(textures: VoxelTextureRegistry, voxels: VoxelRegistry) -> Self {
        Self {
            textures: Arc::new(textures),
            voxels: Arc::new(voxels),
        }
    }
}

pub struct VoxelRegistry {
    labels: hb::HashMap<&'static str, VoxelId>,
    properties: Vec<VoxelProperties>,
    models: Vec<Option<VoxelModel>>,
}

impl VoxelRegistry {
    pub fn get_id(&self, label: &str) -> Option<VoxelId> {
        self.labels.get(label).copied()
    }

    pub fn get_properties_from_label(&self, label: &str) -> Option<VoxelProperties> {
        let id = self.labels.get(label)?;
        Some(self.properties[id.to_usize()].clone())
    }

    pub fn get_properties(&self, id: VoxelId) -> VoxelProperties {
        self.properties[id.to_usize()].clone()
    }

    pub fn get_model(&self, id: VoxelId) -> Option<VoxelModel> {
        self.models[id.to_usize()]
    }
}

pub struct VoxelRegistryBuilder<'a> {
    textures: &'a VoxelTextureRegistry,

    labels: hb::HashMap<&'static str, VoxelId>,
    properties: Vec<VoxelProperties>,
    models: Vec<Option<VoxelModel>>,
}

impl<'a> VoxelRegistryBuilder<'a> {
    pub fn new(textures: &'a VoxelTextureRegistry) -> Self {
        Self {
            textures,
            labels: hb::HashMap::new(),
            properties: Vec::new(),
            models: Vec::new(),
        }
    }

    pub fn register<V: Voxel>(&mut self) {
        let properties = V::properties();
        let label = V::label();

        let voxel_id = VoxelId::new(self.properties.len() as _);

        self.labels.insert(label, voxel_id);
        self.properties.push(properties);
    }

    pub fn finish(self) -> VoxelRegistry {
        VoxelRegistry {
            labels: self.labels,
            properties: self.properties,
            models: self.models,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, dm::From, dm::Into)]
pub struct TextureId(AssetId<Image>);

impl TextureId {
    pub fn inner(self) -> AssetId<Image> {
        self.0
    }
}

pub struct VoxelTextureRegistryBuilder {
    builder: TextureAtlasBuilder,
    labels: hb::HashMap<String, TextureId>,
}

impl VoxelTextureRegistryBuilder {
    pub fn new() -> Self {
        Self {
            builder: Default::default(),
            labels: Default::default(),
        }
    }

    pub fn add_texture(&mut self, handle: impl Into<AssetId<Image>>, image: &Image, label: String) {
        let id: AssetId<Image> = handle.into();

        self.builder.add_texture(id, image);
        self.labels.insert(label, TextureId(id));
    }

    pub fn finish(
        self,
        images: &mut Assets<Image>,
    ) -> Result<VoxelTextureRegistry, TextureAtlasBuilderError> {
        let atlas = self.builder.finish(images)?;

        Ok(VoxelTextureRegistry {
            labels: self.labels,
            atlas,
        })
    }
}

pub struct VoxelTextureRegistry {
    labels: hb::HashMap<String, TextureId>,
    atlas: TextureAtlas,
}

impl VoxelTextureRegistry {
    pub fn get_id(&self, label: &str) -> Option<TextureId> {
        self.labels.get(label).copied()
    }

    pub fn get(&self, id: TextureId) -> Option<Rect> {
        let idx = self.atlas.get_texture_index(id.inner())?;
        self.atlas.textures.get(idx).copied()
    }

    pub fn atlas_texture(&self) -> Handle<Image> {
        self.atlas.texture.clone()
    }
}
