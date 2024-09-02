use std::marker::PhantomData;

use bevy::ecs::event::EventUpdates;
use bevy::prelude::*;
use flume::Sender;

use crate::{generic_system_set, ChannelClosed};

generic_system_set!(FunnelCollectionSystem);

/// Plugin that adds infrastructure for sending an event from multiple threads to the main schedule.
///
/// This plugin will insert the resource [`EventFunnel`] to the world,
/// which can be cloned and sent to tasks or threads and be used to send events back to the main schedule.
pub struct EventFunnelPlugin<E: Event + 'static> {
    existing: bool,
    _event_type: PhantomData<&'static E>,
}

impl<E: Event + 'static> EventFunnelPlugin<E> {
    /// Register a new event and add funnel handling for it.
    pub fn for_new() -> Self {
        Self {
            existing: false,
            _event_type: PhantomData::default(),
        }
    }

    /// Register funnel handling for an existing event.
    pub fn for_existing() -> Self {
        Self {
            existing: true,
            _event_type: PhantomData::default(),
        }
    }
}

/// An event sender that can be cloned and sent to other threads/tasks. Sends all events to the main world.
#[derive(Resource, Clone)]
pub struct EventFunnel<E: Event> {
    tx: Sender<E>,
}

impl<E: Event> EventFunnel<E> {
    /// Send an event to the main world. Returns an error if the
    /// receiver in the main world is closed and therefore the event has nowhere to go.
    pub fn send(&self, event: E) -> Result<(), ChannelClosed<E>> {
        self.tx.send(event).map_err(|err| ChannelClosed(err.0))
    }

    /// The number of events currently in the underlying channel.
    pub fn events(&self) -> usize {
        self.tx.len()
    }
}

impl<E: Event> EventFunnelPlugin<E> {
    /// The system set for the collection system for this event's funnel.
    pub const COLLECTION_SYSTEM: FunnelCollectionSystem<E> = FunnelCollectionSystem::<E>::new();
}

impl<E: Event> Plugin for EventFunnelPlugin<E> {
    fn build(&self, app: &mut App) {
        assert!(
            !app.world().contains_resource::<EventFunnel<E>>(),
            "event already has a funnel"
        );

        if !self.existing {
            app.add_event::<E>();
        } else {
            assert!(
                app.world().contains_resource::<Events<E>>(),
                "tried to add a funnel for an existing event, but there was no 'Events<E>' resource for the type"
            );
        }

        let (tx, rx) = flume::unbounded::<E>();

        let funnel_collect = move |mut writer: EventWriter<E>| {
            writer.send_batch(rx.try_iter());
        };

        app.insert_resource(EventFunnel { tx })
            .add_systems(First, funnel_collect.in_set(Self::COLLECTION_SYSTEM))
            // need to collect events after we clear the old ones
            .configure_sets(First, Self::COLLECTION_SYSTEM.after(EventUpdates));
    }
}
