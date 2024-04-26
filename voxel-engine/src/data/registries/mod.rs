use std::{
    fmt::{Debug, Display},
    hash::{Hash},
    sync::Arc,
};

use anymap::any::Any;
use bevy::ecs::system::Resource;
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

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

#[derive(Clone, Resource)]
pub struct Registries {
    registries: Arc<RwLock<RegistriesAnymap>>,
}

pub type RegistryRef<'a, R> = MappedRwLockReadGuard<'a, R>;

impl Registries {
    pub fn new() -> Self {
        Self {
            registries: Arc::new(RwLock::new(anymap::Map::new())),
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
