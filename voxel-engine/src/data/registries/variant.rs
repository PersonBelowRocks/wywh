use indexmap::IndexMap;

use crate::data::{tile::Transparency, voxel::VoxelModel};

use super::{Registry, RegistryId};

#[derive(Debug, Clone)]
pub struct VariantRegistryEntry<'a> {
    pub transparency: Transparency,
    pub model: &'a Option<VoxelModel>,
}

pub struct VariantRegistryLoader {
    map: IndexMap<String, Variant, ahash::RandomState>,
}

impl VariantRegistryLoader {
    pub fn new() -> Self {
        Self {
            map: IndexMap::with_hasher(ahash::RandomState::new()),
        }
    }

    pub fn register(&mut self, label: impl Into<String>, variant: Variant) {
        self.map.insert(label.into(), variant);
    }

    pub fn build_registry(self) -> VariantRegistry {
        VariantRegistry { map: self.map }
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
    use bevy::math::vec2;

    use crate::data::voxel::BlockModel;

    use super::*;

    #[test]
    fn variant_registry_basics() {
        let mut loader = VariantRegistryLoader::new();

        loader.register(
            "air",
            Variant {
                transparency: Transparency::Transparent,
                model: None,
            },
        );

        loader.register(
            "test",
            Variant {
                transparency: Transparency::Opaque,
                model: Some(VoxelModel::Block(BlockModel::filled(vec2(0.0, 0.0)))),
            },
        );

        let registry = loader.build_registry();

        assert_eq!(Some(RegistryId::new(0)), registry.get_id("air"));
        assert_eq!(Some(RegistryId::new(1)), registry.get_id("test"));

        assert_eq!(
            Transparency::Transparent,
            registry.get_by_label("air").unwrap().transparency
        );
        assert_eq!(&None, registry.get_by_label("air").unwrap().model);

        assert_eq!(
            Transparency::Opaque,
            registry.get_by_label("test").unwrap().transparency
        );
        assert_eq!(
            &Some(VoxelModel::Block(BlockModel::filled(vec2(0.0, 0.0)))),
            registry.get_by_label("test").unwrap().model
        );
    }
}
