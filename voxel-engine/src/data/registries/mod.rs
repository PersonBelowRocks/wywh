use std::{any::type_name, fmt::Debug, hash, marker::PhantomData, sync::Arc};

use anymap::any::Any;
use bevy::ecs::system::Resource;
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use super::resourcepath::ResourcePath;

pub mod error;
pub mod model;
pub mod texture;
pub mod variant;

pub struct RegistryId<R: Registry + ?Sized> {
    id: u64,
    _reg: PhantomData<fn() -> R>,
}

impl<R: Registry + ?Sized> Clone for RegistryId<R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _reg: PhantomData,
        }
    }
}

impl<R: Registry + ?Sized> Copy for RegistryId<R> {}

impl<R: Registry + ?Sized> PartialEq for RegistryId<R> {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl<R: Registry + ?Sized> Eq for RegistryId<R> {}

impl<R: Registry + ?Sized> hash::Hash for RegistryId<R> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.id);
    }
}

impl<R: Registry> Debug for RegistryId<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", type_name::<R>(), self.id)
    }
}

impl<R: Registry> RegistryId<R> {
    /// Create a new registry ID from the provided `id`.
    /// RegistryIds should only be created when a registry is populated, so don't call this unless that's what you're doing.
    pub const fn new(id: u64) -> Self {
        Self {
            id,

            _reg: PhantomData,
        }
    }

    pub fn inner(self) -> u64 {
        self.id
    }
}

pub trait Registry: Send + Sync {
    type Item<'a>
    where
        Self: 'a;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>>;
    fn get_by_id(&self, id: RegistryId<Self>) -> Self::Item<'_>;
    fn get_id(&self, label: &ResourcePath) -> Option<RegistryId<Self>>;
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
