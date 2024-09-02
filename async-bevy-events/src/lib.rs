//! Useful utilities to extend the functionality of bevy's default events.
//! Simplifies the process of gathering results of tasks to use them in the main world.

use std::{any::type_name, fmt::Debug};

pub mod async_events;
pub mod funnel;

pub use async_events::{
    AsyncEventBroadcastSystem, AsyncEventPlugin, AsyncEventReader, AsyncEventWriter, AsyncRecvError,
};

pub use funnel::{EventFunnel, EventFunnelPlugin, FunnelCollectionSystem};

/// Error indicating that a channel was closed and an event could not be sent
#[derive(thiserror::Error, Clone)]
#[error("Channel for '{}' was closed and event could not be sent.", type_name::<E>())]
pub struct ChannelClosed<E>(pub E);

impl<E> Debug for ChannelClosed<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChannelClosed<{}>", type_name::<E>())
    }
}

/// Create a [`SystemSet`][] type that is generic over some type parameter.
///
/// [`SystemSet`]: bevy::ecs::prelude::SystemSet
macro_rules! generic_system_set {
    ($name:ident) => {
        #[derive(derive_more::Debug, Copy, bevy::prelude::SystemSet)]
        pub struct $name<E: 'static>(#[debug(skip)] std::marker::PhantomData<&'static E>);

        impl<E: 'static> $name<E> {
            pub const fn new() -> Self {
                Self(PhantomData)
            }
        }

        impl<E: 'static> Default for $name<E> {
            fn default() -> Self {
                Self(PhantomData)
            }
        }

        impl<E> Clone for $name<E> {
            fn clone(&self) -> Self {
                Self::default()
            }
        }

        impl<E> Eq for $name<E> {}

        impl<E> PartialEq for $name<E> {
            fn eq(&self, _: &Self) -> bool {
                true
            }
        }

        impl<E> std::hash::Hash for $name<E> {
            fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
        }
    };
}

pub(crate) use generic_system_set;
