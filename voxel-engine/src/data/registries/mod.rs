use std::{any::type_name, fmt::Debug, marker::PhantomData, sync::Arc};

use anymap::any::Any;
use bevy::ecs::system::Resource;
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use self::error::RegistryError;

pub mod error;
pub mod texture;

#[derive(Copy, Clone)]
pub struct RegistryId<R: Registry + ?Sized> {
    id: u64,
    _reg: PhantomData<fn() -> R>,
}

impl<R: Registry> Debug for RegistryId<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", type_name::<R>(), self.id)
    }
}

impl<R: Registry> RegistryId<R> {
    /// Create a new registry ID from the provided `id`.
    /// RegistryIds should only be created when a registry is populated, so don't call this unless that's what you're doing.
    pub fn new(id: u64) -> Self {
        Self {
            id,

            _reg: PhantomData,
        }
    }

    pub fn id(self) -> u64 {
        self.id
    }
}

pub trait Registry: Send + Sync {
    type Item;

    fn get_by_label(&self, label: &str) -> Option<&Self::Item>;
    fn get_by_id(&self, id: RegistryId<Self>) -> &Self::Item;
    fn get_id(&self, label: &str) -> RegistryId<Self>;
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

    pub fn add_registry<R: Registry + 'static>(&self, registry: R) -> Result<(), RegistryError> {
        self.registries.write().insert(registry);
        Ok(())
    }

    pub fn get_registry<R: Registry + 'static>(&self) -> Option<RegistryRef<'_, R>> {
        let guard = self.registries.read();

        if !guard.contains::<R>() {
            return None;
        } else {
            Some(RwLockReadGuard::map(guard, |g| g.get::<R>().unwrap()))
        }
    }
}
