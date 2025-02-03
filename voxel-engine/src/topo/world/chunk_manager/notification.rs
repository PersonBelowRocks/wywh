use crate::topo::world::ChunkPos;
use bevy::prelude::Event;
use flume::{Receiver, Sender};

/// A chunk notification. Communicates the state change of a chunk, but this is a very loose
/// definition. Generally, chunk notifications are used to communicate that state "downstream" of a
/// chunk should be updated. This can for example be meshes, connectivity graphs, colliders, etc.
/// Data like this is not directly tied to the voxel data in a chunk, but rather must be "computed"
/// from the voxel data. When the voxel data changes, we send a notification, and the data can be
/// recomputed.
///
/// # Warning
/// A chunk may be updated several times in one tick, in which case multiple chunk notifications may be
/// sent for the same chunk. Make sure you de-duplicate notifications if your logic is sensitive to
/// duplicated chunk notifications!
#[derive(Event, Debug, Copy, Clone)]
pub struct ChunkNotification {
    pub chunk_pos: ChunkPos,
}

/// A central bus where all chunk notifications can be collected and chunk notification
/// senders can be created.
pub struct NotificationBus {
    tx: Sender<ChunkNotification>,
    rx: Receiver<ChunkNotification>,
}

impl Default for NotificationBus {
    fn default() -> Self {
        let (tx, rx) = flume::unbounded::<ChunkNotification>();
        Self { tx, rx }
    }
}

impl NotificationBus {
    /// Create a chunk notification sender for this bus.
    #[must_use]
    #[inline]
    pub fn sender(&self) -> Sender<ChunkNotification> {
        self.tx.clone()
    }

    /// Get a reference to the underlying notification sender for this bus.
    #[must_use]
    #[inline]
    pub fn sender_ref(&self) -> &Sender<ChunkNotification> {
        &self.tx
    }

    /// Get the chunk notification receiver for this bus.
    #[must_use]
    #[inline]
    pub fn receiver(&self) -> &Receiver<ChunkNotification> {
        &self.rx
    }
}
