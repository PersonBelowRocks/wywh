use anymap::any::Any;
use bevy::ecs::system::Resource;
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use std::cell::{LazyCell, OnceCell};
use std::sync::{LazyLock, OnceLock};
use std::{
    fmt::{Debug, Display},
    hash::Hash,
    sync::Arc,
};

use super::resourcepath::ResourcePath;

pub mod block;
pub mod error;
pub mod model;
pub mod texture;

pub trait Registry: Send + Sync {
    type Id: Sized + Eq + Hash + Clone + Display;

    type Item<'a>
    where
        Self: 'a;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>>;
    fn get_by_id(&self, id: Self::Id) -> Self::Item<'_>;
    fn get_id(&self, label: &ResourcePath) -> Option<Self::Id>;
}

#[derive(Clone, Debug)]
pub enum RegistryStage<L, F> {
    Loading(L),
    Frozen(F),
}

type RegistriesAnymap = anymap::Map<dyn Any + Send + Sync>;

/// A collection of registries used by the game, engine, or addons.
pub struct RegistryManager {
    registries: RwLock<RegistriesAnymap>,
}

/// The global registry manager. All registries should exist in this registry manager if it's possible.
///
/// This registry manager will be initialized by the engine during startup. Modifying registries after
/// the startup phase may lead to weird unintended behaviour, and should never be done.
pub static REGISTRY_MANAGER: LazyLock<RegistryManager> = LazyLock::new(RegistryManager::new);

pub type RegistryRef<'a, R> = MappedRwLockReadGuard<'a, R>;

impl RegistryManager {
    fn new() -> Self {
        Self {
            registries: RwLock::new(anymap::Map::new()),
        }
    }

    pub fn add_registry<R: Registry + 'static>(&self, registry: R) {
        self.registries.write().insert(registry);
    }

    pub fn get_registry<R: Registry + 'static>(&self) -> Option<RegistryRef<'_, R>> {
        let guard = self.registries.read();

        // The call to anymap::Map::get here returns an option but due to the closure signature in RwLockReadGuard we have to return a reference
        // to a type. Therefore we unwrap on the get call and test if the type exists in the map before we get there.
        if !guard.contains::<R>() {
            return None;
        } else {
            Some(RwLockReadGuard::map(guard, |g| g.get::<R>().unwrap()))
        }
    }
}
