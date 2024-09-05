use std::{fs::File, io::Read, path::Path};

use indexmap::IndexMap;

use crate::data::{
    error::BlockVariantFileLoaderError,
    resourcepath::ResourcePath,
    tile::Transparency,
    voxel::{descriptor::BlockVariantDescriptor, BlockModel},
};

#[cfg(test)]
use crate::{
    data::{resourcepath::rpath, texture::FaceTexture, voxel::rotations::BlockModelFaceMap},
    util::FaceMap,
};

use super::{error::BlockVariantRegistryLoadError, texture::TextureRegistry, Registry};

pub const MAX_RECURSION_DEPTH: usize = 8;
pub static BLOCK_VARIANT_FILE_EXTENSION: &'static str = "block";

#[derive(Debug, Clone)]
pub struct BlockVariantRegistryEntry<'a> {
    pub options: BlockOptions,
    pub model: Option<&'a BlockModel>,
}

#[derive(Clone)]
pub struct BlockVariantFileLoader {
    raw_descriptors: hb::HashMap<ResourcePath, Vec<u8>>,
}

impl BlockVariantFileLoader {
    pub fn new() -> Self {
        Self {
            raw_descriptors: hb::HashMap::new(),
        }
    }

    pub fn labels(&self) -> impl Iterator<Item = &ResourcePath> {
        self.raw_descriptors.keys()
    }

    pub fn entries(&self) -> impl Iterator<Item = (&ResourcePath, &[u8])> {
        self.raw_descriptors.iter().map(|(r, v)| (r, v.as_slice()))
    }

    pub fn load_folder(
        &mut self,
        path: impl AsRef<Path>,
        recurse_depth: usize,
    ) -> Result<(), BlockVariantFileLoaderError> {
        let path = path.as_ref();

        for entry in walkdir::WalkDir::new(path).max_depth(recurse_depth) {
            let entry = entry?;

            if entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == BLOCK_VARIANT_FILE_EXTENSION)
            {
                let rpath =
                    ResourcePath::try_from(entry.path().strip_prefix(path).map_err(|_| {
                        BlockVariantFileLoaderError::InvalidFileName(entry.path().to_path_buf())
                    })?)
                    .map_err(|_| {
                        BlockVariantFileLoaderError::InvalidFileName(entry.path().to_path_buf())
                    })?;

                self.load_file(entry.path(), rpath)?;
            }
        }

        Ok(())
    }

    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        resource_path: ResourcePath,
    ) -> Result<(), BlockVariantFileLoaderError> {
        let path = path.as_ref();

        let mut file = File::open(&path)?;

        let mut buffer = Vec::<u8>::with_capacity(file.metadata()?.len() as _);
        file.read_to_end(&mut buffer)?;

        self.add_raw_buffer(resource_path, buffer);

        Ok(())
    }

    pub fn add_raw_buffer(&mut self, label: ResourcePath, buffer: Vec<u8>) {
        self.raw_descriptors.insert(label, buffer);
    }
}

pub struct BlockVariantRegistryLoader {
    file_loader: BlockVariantFileLoader,
    manual_descriptors: hb::HashMap<ResourcePath, BlockVariantDescriptor>,
}

impl BlockVariantRegistryLoader {
    pub fn new() -> Self {
        Self {
            file_loader: BlockVariantFileLoader::new(),
            manual_descriptors: hb::HashMap::new(),
        }
    }

    pub fn register_from_directory(
        &mut self,
        path: impl AsRef<Path>,
        recurse: bool,
    ) -> Result<(), BlockVariantRegistryLoadError> {
        let depth = if recurse { MAX_RECURSION_DEPTH } else { 0 };

        Ok(self.file_loader.load_folder(path, depth)?)
    }

    pub fn register(&mut self, label: ResourcePath, descriptor: BlockVariantDescriptor) {
        self.manual_descriptors.insert(label.into(), descriptor);
    }

    pub fn build_registry(
        self,
        texture_registry: &TextureRegistry,
    ) -> Result<BlockVariantRegistry, BlockVariantRegistryLoadError> {
        let mut map =
            IndexMap::<ResourcePath, BlockVariant, ahash::RandomState>::with_capacity_and_hasher(
                self.manual_descriptors.len(),
                ahash::RandomState::new(),
            );

        for (rpath, descriptor) in self.manual_descriptors.into_iter() {
            let model = if let Some(model_desc) = descriptor.model {
                Some(model_desc.create_block_model(texture_registry)?)
            } else {
                None
            };

            let variant = BlockVariant {
                options: descriptor.options,
                model,
            };

            map.insert(rpath, variant);
        }

        for (rpath, buffer) in self.file_loader.entries() {
            let descriptor =
                toml::from_str::<BlockVariantDescriptor>(String::from_utf8_lossy(buffer).as_ref())?;

            let model = if let Some(model_desc) = descriptor.model {
                Some(model_desc.create_block_model(texture_registry)?)
            } else {
                None
            };

            let variant = BlockVariant {
                options: descriptor.options,
                model,
            };

            map.insert(rpath.clone(), variant);
        }

        Ok(BlockVariantRegistry { map })
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct BlockVariant {
    options: BlockOptions,
    model: Option<BlockModel>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct BlockOptions {
    pub transparency: Transparency,
    #[serde(default)]
    pub subdividable: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, dm::Display)]
#[display("[block_variant:{:08}]", self.0)]
pub struct BlockVariantId(u32);

impl BlockVariantId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub fn as_u32(self) -> u32 {
        self.0 as u32
    }

    #[cfg(test)]
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    // TODO: safety docs
    #[inline]
    pub unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

pub struct BlockVariantRegistry {
    map: IndexMap<ResourcePath, BlockVariant, ahash::RandomState>,
}

impl BlockVariantRegistry {
    pub const RPATH_VOID: &'static str = "void";
}

#[cfg(test)]
impl BlockVariantRegistry {
    pub const VOID: BlockVariantId = BlockVariantId::new(0);

    pub const RPATH_FULL: &'static str = "full";
    pub const FULL: BlockVariantId = BlockVariantId::new(1);
    pub const RPATH_SUBDIV: &'static str = "subdiv";
    pub const SUBDIV: BlockVariantId = BlockVariantId::new(2);

    pub fn new_mock(_registry: &TextureRegistry) -> Self {
        let mut map = IndexMap::with_hasher(ahash::RandomState::default());

        map.insert(
            rpath(Self::RPATH_VOID),
            BlockVariant {
                options: BlockOptions {
                    transparency: Transparency::Transparent,
                    subdividable: true,
                },
                model: None,
            },
        );

        map.insert(
            rpath(Self::RPATH_FULL),
            BlockVariant {
                options: BlockOptions {
                    transparency: Transparency::Opaque,
                    subdividable: false,
                },
                model: Some(BlockModel {
                    directions: FaceMap::new(),
                    model: BlockModelFaceMap::filled(FaceTexture::new(TextureRegistry::TEX1)),
                }),
            },
        );

        map.insert(
            rpath(Self::RPATH_SUBDIV),
            BlockVariant {
                options: BlockOptions {
                    transparency: Transparency::Opaque,
                    subdividable: true,
                },
                model: Some(BlockModel {
                    directions: FaceMap::new(),
                    model: BlockModelFaceMap::filled(FaceTexture::new(TextureRegistry::TEX2)),
                }),
            },
        );

        Self { map }
    }
}

impl Registry for BlockVariantRegistry {
    type Item<'a> = BlockVariantRegistryEntry<'a>;
    type Id = BlockVariantId;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>> {
        let variant = self.map.get(label)?;

        Some(BlockVariantRegistryEntry {
            options: variant.options,
            model: variant.model.as_ref(),
        })
    }

    fn get_by_id(&self, id: Self::Id) -> Self::Item<'_> {
        let idx = id.index();
        let (_, variant) = self.map.get_index(idx).unwrap();

        BlockVariantRegistryEntry {
            options: variant.options,
            model: variant.model.as_ref(),
        }
    }

    fn get_id(&self, label: &ResourcePath) -> Option<Self::Id> {
        self.map.get_index_of(label).map(|i| BlockVariantId(i as _))
    }
}

#[cfg(test)]
mod tests {
    // TODO: tests!
}
