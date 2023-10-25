use bevy::{asset::AssetPath, prelude::*};

use super::{
    error::TextureLoadingError,
    tile::{Transparency, VoxelId},
    voxel::{Voxel, VoxelModel, VoxelProperties},
};

pub struct RegistryManager {
    pub textures: VoxelTextureRegistry,
    pub voxels: VoxelRegistry,
}

pub struct RegistryBuilder<'a, 'b> {
    textures: VoxelTextureRegistryBuilder<'a, 'b>,
}

pub struct VoxelRegistry {
    labels: hb::HashMap<&'static str, VoxelId>,
    properties: Vec<VoxelProperties>,
}

impl VoxelRegistry {
    pub fn get_id(&self, label: &str) -> Option<VoxelId> {
        self.labels.get(label).copied()
    }

    pub fn get_properties_from_label(&self, label: &str) -> Option<VoxelProperties> {
        let id = self.labels.get(label)?;
        Some(self.properties[id.to_usize()])
    }

    pub fn get_properties(&self, id: VoxelId) -> VoxelProperties {
        self.properties[id.to_usize()]
    }
}

pub struct VoxelRegistryBuilder {
    labels: hb::HashMap<&'static str, VoxelId>,
    properties: Vec<VoxelProperties>,
}

impl VoxelRegistryBuilder {
    pub fn new() -> Self {
        Self {
            labels: hb::HashMap::new(),
            properties: Vec::new(),
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
}

impl<'a, 'b> VoxelTextureRegistryBuilder<'a, 'b> {
    pub fn new(server: &'a AssetServer, textures: &'b mut Assets<Image>) -> Self {
        Self {
            server,
            textures,
            builder: TextureAtlasBuilder::default(),
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
            return Err(TextureLoadingError::InvalidTextureDimensions(path.into()));
        }

        let id: AssetId<Image> = handle.into();

        self.builder.add_texture(id, texture);

        Ok(TextureId(id))
    }

    pub fn finish(mut self) -> VoxelTextureRegistry {
        let atlas = self.builder.finish(&mut self.textures).unwrap();

        VoxelTextureRegistry { atlas }
    }
}

pub struct VoxelTextureRegistry {
    // texture_atlas_uvs: SyncHashMap<TextureId, Rect>,
    atlas: TextureAtlas,
}

impl VoxelTextureRegistry {
    pub fn get(&self, id: TextureId) -> Option<Rect> {
        let idx = self.atlas.get_texture_index(id.inner())?;
        self.atlas.textures.get(idx).copied()
    }

    pub fn atlas_texture(&self) -> Handle<Image> {
        self.atlas.texture.clone()
    }
}
