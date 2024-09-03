use std::{any::type_name, marker::PhantomData, time::Duration};

use bevy::ecs::event::EventUpdates;
use bevy::prelude::*;
use flume::{Receiver, RecvTimeoutError, Sender, TryRecvError};

use crate::{funnel::EventFunnel, generic_system_set, ChannelClosed};

generic_system_set!(AsyncEventBroadcastSystem);

/// Error encountered when reading an event sent from the main world.
#[derive(thiserror::Error, Debug, Clone)]
pub enum AsyncRecvError {
    #[error("Channel was closed and no event could be read")]
    ChannelClosed,
    #[error("Timed out waiting to receive an event")]
    Timeout,
}

/// An event writer to send events to the corresponding [`AsyncEventReader`]s.
/// Using this from bevy's main schedule is kind of pointless since you should just use the regular event writer there,
/// but this writer is useful if you need to send events from a bunch of tasks, to a bunch of other tasks.
#[derive(Clone, Resource)]
pub struct AsyncEventWriter<E: Event + 'static> {
    tx: Sender<E>,
}

impl<E: Event + 'static> AsyncEventWriter<E> {
    /// Send an event through this event channel.
    pub fn send(&self, event: E) -> Result<(), ChannelClosed<E>> {
        self.tx.send(event).map_err(|err| ChannelClosed(err.0))
    }

    /// The number of events currently in the underlying channel.
    pub fn events(&self) -> usize {
        self.tx.len()
    }

    /// The number of receivers for this event.
    pub fn receivers(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// Reads events sent through a channel from the main world or from another task.
#[derive(Clone, Resource)]
pub struct AsyncEventReader<E: Event + 'static> {
    rx: Receiver<E>,
}

impl<E: Event> AsyncEventReader<E> {
    pub fn recv(&self) -> Result<E, AsyncRecvError> {
        self.rx.recv().map_err(|_| AsyncRecvError::ChannelClosed)
    }

    pub fn try_recv(&self) -> Result<E, AsyncRecvError> {
        self.rx.try_recv().map_err(|err| match err {
            TryRecvError::Empty => AsyncRecvError::Timeout,
            TryRecvError::Disconnected => AsyncRecvError::ChannelClosed,
        })
    }

    pub fn try_recv_timeout(&self, timeout: Duration) -> Result<E, AsyncRecvError> {
        self.rx.recv_timeout(timeout).map_err(|err| match err {
            RecvTimeoutError::Timeout => AsyncRecvError::Timeout,
            RecvTimeoutError::Disconnected => AsyncRecvError::ChannelClosed,
        })
    }

    pub async fn recv_async(&self) -> Result<E, AsyncRecvError> {
        self.rx
            .recv_async()
            .await
            .map_err(|_| AsyncRecvError::ChannelClosed)
    }

    /// The number of events currently in the underlying channel.
    pub fn events(&self) -> usize {
        self.rx.len()
    }
}

/// Add an event to the app and insert the async [reader][] and [writer][] for it.
/// The added event may be used as normal by bevy systems, but instead of dropping the events every frame
/// they are broadcast to all the async readers.
///
/// # Memory Leak Warning
/// If events are sent with either the [`AsyncEventWriter`] or bevy's [`EventWriter`][] and NOT received by a
/// [`AsyncEventReader`], they will accumulate in the underlying channel.
///
/// [reader]: AsyncEventReader
/// [writer]: AsyncEventWriter
/// [EventWriter]: bevy::prelude::EventWriter
pub struct AsyncEventPlugin<E: 'static + Event> {
    _event_type: PhantomData<&'static E>,
}

impl<E: 'static + Event> Default for AsyncEventPlugin<E> {
    fn default() -> Self {
        Self {
            _event_type: PhantomData,
        }
    }
}

impl<E: Event> AsyncEventPlugin<E> {
    pub const BROADCAST_SYSTEM: AsyncEventBroadcastSystem<E> =
        AsyncEventBroadcastSystem::<E>::new();
}

/// Assert that an app can have an async event (and its readers/writers) added.
fn assert_app_supports_async_event<E: Event + 'static>(app: &App) {
    assert!(
        !app.world().contains_resource::<Events<E>>(),
        "can't add async event handling for existing event type"
    );
    assert!(
        !app.world().contains_resource::<EventFunnel<E>>(),
        "can't add async event handling for funneled event"
    );
}

impl<E: Event + 'static> Plugin for AsyncEventPlugin<E> {
    fn build(&self, app: &mut App) {
        assert_app_supports_async_event::<E>(app);

        let (tx, rx) = flume::unbounded::<E>();

        app.init_resource::<Events<E>>()
            .insert_resource(AsyncEventReader { rx })
            .insert_resource(AsyncEventWriter { tx })
            .configure_sets(First, Self::BROADCAST_SYSTEM.in_set(EventUpdates))
            .add_systems(
                First,
                broadcast_async_events::<E>.in_set(Self::BROADCAST_SYSTEM),
            );
    }
}

pub fn broadcast_async_events<E: Event>(
    tx: Res<AsyncEventWriter<E>>,
    mut events: ResMut<Events<E>>,
) {
    if tx.receivers() == 1 {
        warn!("Broadcasting event {} with only one receiver, which is likely the one in the main world.", type_name::<E>());
    }

    if tx.receivers() == 0 {
        warn!(
            "Broadcasting event {} with no receiver, meaning the channel is likely closed.",
            type_name::<E>()
        );
    }

    for event in events.update_drain() {
        tx.send(event).unwrap()
    }
}
