use indexmap::IndexMap;

use crate::data::{
    resourcepath::ResourcePath,
    tile::Transparency,
    variant_file_loader::VariantFileLoader,
    voxel::{descriptor::BlockVariantDescriptor, BlockModel},
};

use super::{texture::TextureRegistry, Registry};

#[derive(Debug, Clone)]
pub struct VariantRegistryEntry<'a> {
    pub transparency: Transparency,
    pub model: &'a Option<BlockModel>,
}

pub struct VariantRegistryLoader {
    descriptors: hb::HashMap<ResourcePath, BlockVariantDescriptor>,
}

impl VariantRegistryLoader {
    pub fn new() -> Self {
        Self {
            descriptors: hb::HashMap::new(),
        }
    }

    pub fn register_from_file_loader(&mut self, _loader: &VariantFileLoader) -> Result<(), ()> {
        Ok(())
    }

    pub fn register(&mut self, label: ResourcePath, descriptor: BlockVariantDescriptor) {
        self.descriptors.insert(label.into(), descriptor);
    }

    pub fn build_registry(
        self,
        texture_registry: &TextureRegistry,
    ) -> Result<BlockVariantRegistry, ()> {
        let mut map =
            IndexMap::<ResourcePath, BlockVariant, ahash::RandomState>::with_capacity_and_hasher(
                self.descriptors.len(),
                ahash::RandomState::new(),
            );

        for (label, descriptor) in self.descriptors.into_iter() {
            let model = if descriptor.options.transparency.is_opaque() {
                todo!() // Some(BlockModel::from_descriptor(&descriptor, texture_registry)?)
            } else {
                None
            };

            let variant = BlockVariant {
                transparency: descriptor.options.transparency,
                model,
            };

            map.insert(label, variant);
        }

        Ok(BlockVariantRegistry { map })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct BlockVariant {
    transparency: Transparency,
    model: Option<BlockModel>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, dm::Display)]
#[display(fmt="[block_variant:{:08}]", self.0)]
pub struct BlockVariantId(u32);

impl BlockVariantId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn as_u32(self) -> u32 {
        self.0 as u32
    }

    #[cfg(test)]
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

pub struct BlockVariantRegistry {
    map: IndexMap<ResourcePath, BlockVariant, ahash::RandomState>,
}

impl Registry for BlockVariantRegistry {
    type Item<'a> = VariantRegistryEntry<'a>;
    type Id = BlockVariantId;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>> {
        let variant = self.map.get(label)?;

        Some(VariantRegistryEntry {
            transparency: variant.transparency,
            model: &variant.model,
        })
    }

    fn get_by_id(&self, id: Self::Id) -> Self::Item<'_> {
        let idx = id.index();
        let (_, variant) = self.map.get_index(idx).unwrap();

        VariantRegistryEntry {
            transparency: variant.transparency,
            model: &variant.model,
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
