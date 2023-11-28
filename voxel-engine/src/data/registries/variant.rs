use indexmap::IndexMap;

use crate::data::{
    tile::Transparency,
    voxel::{descriptor::VariantDescriptor, VoxelModel},
};

use super::{texture::TextureRegistry, Registry, RegistryId};

#[derive(Debug, Clone)]
pub struct VariantRegistryEntry<'a> {
    pub transparency: Transparency,
    pub model: &'a Option<VoxelModel>,
}

pub struct VariantRegistryLoader<'a> {
    descriptors: Vec<VariantDescriptor<'a>>,
}

impl<'a> VariantRegistryLoader<'a> {
    pub fn new() -> Self {
        Self {
            descriptors: Vec::new(),
        }
    }

    pub fn register(&mut self, descriptor: VariantDescriptor<'a>) {
        self.descriptors.push(descriptor);
    }

    pub fn build_registry(self, texture_registry: &TextureRegistry) -> VariantRegistry {
        let mut map = IndexMap::<String, Variant, ahash::RandomState>::with_capacity_and_hasher(
            self.descriptors.len(),
            ahash::RandomState::new(),
        );

        for descriptor in self.descriptors.into_iter() {
            let label = descriptor.label.to_string();

            let variant = Variant {
                transparency: descriptor.transparency,
                model: descriptor
                    .model
                    .map(|m| m.create_voxel_model(texture_registry)),
            };

            map.insert(label, variant);
        }

        VariantRegistry { map }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Variant {
    transparency: Transparency,
    model: Option<VoxelModel>,
}

pub struct VariantRegistry {
    map: IndexMap<String, Variant, ahash::RandomState>,
}

impl Registry for VariantRegistry {
    type Item<'a> = VariantRegistryEntry<'a>;

    fn get_by_label(&self, label: &str) -> Option<Self::Item<'_>> {
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

    fn get_id(&self, label: &str) -> Option<RegistryId<Self>> {
        self.map
            .get_index_of(label)
            .map(|i| RegistryId::new(i as _))
    }
}

#[cfg(test)]
mod tests {
    // TODO: tests!
}
