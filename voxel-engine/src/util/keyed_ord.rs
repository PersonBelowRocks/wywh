use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub trait Keyed<Id = ()> {
    type Key: Ord;

    fn key(&self) -> &Self::Key;
}

#[derive(Copy, Clone)]
pub struct KeyedOrd<T: Keyed<K>, K = ()> {
    data: T,

    _k: PhantomData<Arc<K>>,
}

impl<T: Keyed<K>, K> KeyedOrd<T, K> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            _k: PhantomData::default(),
        }
    }

    pub fn into_inner(self) -> T {
        self.data
    }
}

impl<T: Keyed<K>, K> Deref for KeyedOrd<T, K> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Keyed<K>, K> DerefMut for KeyedOrd<T, K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: Keyed<K>, K> PartialEq for KeyedOrd<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.data.key().eq(other.data.key())
    }
}

impl<T: Keyed<K>, K> Eq for KeyedOrd<T, K> {}

impl<T: Keyed<K>, K> PartialOrd for KeyedOrd<T, K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.data.key().partial_cmp(other.data.key())
    }
}

impl<T: Keyed<K>, K> Ord for KeyedOrd<T, K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.data.key().cmp(other.data.key())
    }
}
