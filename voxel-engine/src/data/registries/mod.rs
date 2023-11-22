use std::{any::type_name, fmt::Debug, marker::PhantomData};

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

    fn register(&mut self, label: &str, entry: Self::Item) -> RegistryId<Self>;
    /// Freeze the registry and prevent further additions being made.
    fn freeze(&mut self);
    fn get_by_label(&self, label: &str) -> Option<&Self::Item>;
    fn get_by_id(&self, id: RegistryId<Self>) -> &Self::Item;
}
