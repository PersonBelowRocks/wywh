use std::sync::Arc;

use bevy::{asset::AssetPath, prelude::*};

use super::{
    error::TextureLoadingError,
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

pub struct VoxelTextureRegistryBuilder<'a, 'b> {
    server: &'a AssetServer,
    textures: &'b mut Assets<Image>,
    builder: TextureAtlasBuilder,
    labels: hb::HashMap<String, TextureId>,
}

impl<'a, 'b> VoxelTextureRegistryBuilder<'a, 'b> {
    pub fn new(server: &'a AssetServer, textures: &'b mut Assets<Image>) -> Self {
        Self {
            server,
            textures,
            builder: TextureAtlasBuilder::default(),
            labels: hb::HashMap::new(),
        }
    }

    pub fn add_texture<'p>(
        &mut self,
        path: impl Into<AssetPath<'p>>,
    ) -> Result<TextureId, TextureLoadingError> {
        let path: AssetPath = path.into();

        let handle = self.server.load::<Image>(path.clone());
        let texture = self
            .textures
            .get(handle.clone())
            .ok_or_else(|| TextureLoadingError::FileNotFound(path.clone().into()))?;
        if texture.texture_descriptor.size.width != texture.texture_descriptor.size.height {
            return Err(TextureLoadingError::InvalidTextureDimensions(
                path.clone().into(),
            ));
        }

        let id: AssetId<Image> = handle.into();

        self.labels.insert(path.to_string(), TextureId(id));
        self.builder.add_texture(id, texture);

        Ok(TextureId(id))
    }

    pub fn finish(mut self) -> VoxelTextureRegistry {
        let atlas = self.builder.finish(&mut self.textures).unwrap();

        VoxelTextureRegistry {
            labels: self.labels,
            atlas,
        }
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
