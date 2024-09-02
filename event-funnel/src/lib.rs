//! Useful utilities to extend the functionality of bevy's default events.
//! Simplifies the process of gathering results of tasks to use them in the

use std::{
    any::{type_name, TypeId},
    marker::PhantomData,
};

use bevy::prelude::*;
use crossbeam::channel::{Receiver, Sender};

/// Plugin that adds the necessary infrastructure for funneling an event type.
pub struct EventFunnelPlugin<E: Event + 'static> {
    manual: bool,
    _event_type: PhantomData<&'static E>,
}

impl<E: Event + 'static> EventFunnelPlugin<E> {
    pub fn auto() -> Self {
        Self {
            manual: false,
            _event_type: PhantomData::default(),
        }
    }

    pub fn manual() -> Self {
        Self {
            manual: true,
            _event_type: PhantomData::default(),
        }
    }
}

/// Error indicating that the funnel's receiver in the main world was closed.
/// Stores the event that failed to send so it can be recovered.
#[derive(thiserror::Error, derive_more::Debug, Clone)]
#[error("Event funnel for '{}' was closed and event could not be sent.", type_name::<E>())]
pub struct ChannelClosed<E>(pub E);

/// An event sender that can be cloned and sent to other threads/tasks.
#[derive(Resource, Clone)]
pub struct EventFunnel<E: Event> {
    tx: Sender<E>,
}

impl<E: Event> EventFunnel<E> {
    /// Funnel an event to the main world. Returns an error if the
    /// receiver in the main world is closed and therefore the event has nowhere to go.
    pub fn send(&self, event: E) -> Result<(), ChannelClosed<E>> {
        self.tx.send(event).map_err(|err| ChannelClosed(err.0))
    }

    /// The number of events currently in the funnel
    pub fn events(&self) -> usize {
        self.tx.len()
    }
}

impl<E: Event> EventFunnelPlugin<E> {
    // TODO: order this after bevy's event update system
    pub const COLLECTION_SYSTEM: FunnelCollectSystem<E> = FunnelCollectSystem::<E>(PhantomData);
}

impl<E: Event> Plugin for EventFunnelPlugin<E> {
    fn build(&self, app: &mut App) {
        if !self.manual {
            app.add_event::<E>();
        }

        let (tx, rx) = crossbeam::channel::unbounded::<E>();

        let event_funnel_collect = move |mut writer: EventWriter<E>| {
            writer.send_batch(rx.try_iter());
        };

        app.insert_resource(EventFunnel { tx })
            .add_systems(First, event_funnel_collect.in_set(Self::COLLECTION_SYSTEM));
    }
}

pub use funnel_system_label::FunnelCollectSystem;
// rust's derive macro is kind of stupid and doesn't use correct bounds for generic types, so
// we need to manually define everything which is quite noisy so we put it in its own module to avoid
// dirtying all the other code
mod funnel_system_label {
    use std::hash::Hash;

    use super::*;

    /// The system set of the collection system for this event's funnel.
    #[derive(derive_more::Debug, Copy, SystemSet)]
    pub struct FunnelCollectSystem<E: 'static>(#[debug(skip)] pub(crate) PhantomData<&'static E>);

    impl<E: 'static> Default for FunnelCollectSystem<E> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<E> Clone for FunnelCollectSystem<E> {
        fn clone(&self) -> Self {
            Self::default()
        }
    }

    impl<E> Eq for FunnelCollectSystem<E> {}

    impl<E> PartialEq for FunnelCollectSystem<E> {
        fn eq(&self, _: &Self) -> bool {
            true
        }
    }

    impl<E> Hash for FunnelCollectSystem<E> {
        fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
    }
}
