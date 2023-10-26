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

pub struct VoxelTextureRegistryBuilder<'a> {
    server: &'a AssetServer,
    handles: hb::HashMap<String, TextureId>,
}

impl<'a> VoxelTextureRegistryBuilder<'a> {
    pub fn new(server: &'a AssetServer) -> Self {
        Self {
            server,
            handles: hb::HashMap::new(),
        }
    }

    pub fn add_texture<'p>(
        &mut self,
        path: impl Into<AssetPath<'p>>,
    ) -> Result<TextureId, TextureLoadingError> {
        let path: AssetPath = path.into();

        let handle = self.server.load::<Image>(path.clone());
        let id: AssetId<Image> = handle.into();

        self.handles.insert(path.to_string(), TextureId(id));
        // self.builder.add_texture(id, texture);

        Ok(TextureId(id))
    }

    fn block_until_loaded<'t>(
        &self,
        textures: &'t Assets<Image>,
    ) -> Result<hb::HashMap<TextureId, &'t Image>, TextureLoadingError> {
        let mut finished =
            hb::HashMap::<TextureId, &Image>::with_capacity(self.handles.values().len());
        let mut resume_polling = true;

        while resume_polling {
            resume_polling = false;

            for id in self.handles.values() {
                if finished.contains_key(id) {
                    continue;
                }

                // TODO: doesnt work because we cant access the handle in the same system, we need to try it later
                if let Some(image) = textures.get(id.inner()) {
                    println!("loaded an image with id {id:?}");
                    let size = image.texture_descriptor.size;
                    if size.height != size.width {
                        return Err(TextureLoadingError::InvalidTextureDimensions(*id));
                    }

                    finished.insert(*id, image);
                } else {
                    resume_polling = true;
                }
            }
        }

        Ok(finished)
    }

    pub fn finish(
        mut self,
        textures: &mut Assets<Image>,
    ) -> Result<VoxelTextureRegistry, TextureLoadingError> {
        /*
        let texture = self
            .textures
            .get(handle.clone())
            .ok_or_else(|| TextureLoadingError::FileNotFound(path.clone().into()))?;
        if texture.texture_descriptor.size.width != texture.texture_descriptor.size.height {
            return Err(TextureLoadingError::InvalidTextureDimensions(
                path.clone().into(),
            ));
        }
        */
        let mut builder = TextureAtlasBuilder::default();

        println!("loading textures");
        for (id, texture) in self.block_until_loaded(textures)? {
            builder.add_texture(id.inner(), texture);
        }
        println!("textures loaded");

        let atlas = builder.finish(textures).unwrap();

        Ok(VoxelTextureRegistry {
            labels: self.handles,
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
