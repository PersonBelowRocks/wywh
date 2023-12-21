use indexmap::IndexMap;

use crate::data::{
    resourcepath::ResourcePath,
    tile::Transparency,
    variant_file_loader::VariantFileLoader,
    voxel::{descriptor::VariantDescriptor, VoxelModel},
};

use super::{error::VariantRegistryError, texture::TextureRegistry, Registry, RegistryId};

#[derive(Debug, Clone)]
pub struct VariantRegistryEntry<'a> {
    pub transparency: Transparency,
    pub model: &'a Option<VoxelModel>,
}

pub struct VariantRegistryLoader {
    descriptors: hb::HashMap<ResourcePath, VariantDescriptor>,
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

    pub fn register(&mut self, label: ResourcePath, descriptor: VariantDescriptor) {
        self.descriptors.insert(label.into(), descriptor);
    }

    pub fn build_registry(
        self,
        texture_registry: &TextureRegistry,
    ) -> Result<VariantRegistry, VariantRegistryError> {
        let mut map =
            IndexMap::<ResourcePath, Variant, ahash::RandomState>::with_capacity_and_hasher(
                self.descriptors.len(),
                ahash::RandomState::new(),
            );

        for (label, descriptor) in self.descriptors.into_iter() {
            let model = descriptor
                .model
                .map(|m| m.create_voxel_model(texture_registry))
                .map(|r| r.map(Some))
                .unwrap_or(Ok(None))?;

            let variant = Variant {
                transparency: descriptor.transparency,
                model,
            };

            map.insert(label, variant);
        }

        Ok(VariantRegistry { map })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Variant {
    transparency: Transparency,
    model: Option<VoxelModel>,
}

pub struct VariantRegistry {
    map: IndexMap<ResourcePath, Variant, ahash::RandomState>,
}

impl Registry for VariantRegistry {
    type Item<'a> = VariantRegistryEntry<'a>;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>> {
        let variant = self.map.get(label)?;

        Some(VariantRegistryEntry {
            transparency: variant.transparency,
            model: &variant.model,
        })
    }

    fn get_by_id(&self, id: RegistryId<Self>) -> Self::Item<'_> {
        let idx = id.inner() as usize;
        let (_, variant) = self.map.get_index(idx).unwrap();

        VariantRegistryEntry {
            transparency: variant.transparency,
            model: &variant.model,
        }
    }

    fn get_id(&self, label: &ResourcePath) -> Option<RegistryId<Self>> {
        self.map
            .get_index_of(label)
            .map(|i| RegistryId::new(i as _))
    }
}

#[cfg(test)]
mod tests {
    // TODO: tests!
}
