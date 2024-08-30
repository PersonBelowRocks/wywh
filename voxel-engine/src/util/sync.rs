use std::time::Duration;

/// Describes the strategy that should be used when getting a lock over chunk data.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LockStrategy {
    /// Block for the given duration while waiting for a lock, and error if we exceed the timeout.
    Timeout(Duration),
    /// Block indefinitely while waiting for a lock. Does not error but might deadlock. Unwrapping
    /// on the returned [`StrategySyncError`] should be fine when you know that you're using a blocking
    /// strategy.
    Blocking,
    /// Immediately get a lock to the data if possible, otherwise error.
    Immediate,
}

/// Error(s) related to strategic synchronization and locking.
/// The error reflects the kind of [`LockStrategy`] used in the call.
/// See the documentation on [`LockStrategy`] for more information.
///
/// [read handles]: crate::topo::world::chunk::ChunkReadHandle
/// [write handles]: crate::topo::world::chunk::ChunkWriteHandle
#[derive(te::Error, Debug, Clone)]
pub enum StrategySyncError {
    /// Could not get a lock in time.
    #[error("Timed out after waiting {0:?} for lock")]
    Timeout(Duration),
    /// Could not get a lock immediately.
    #[error("Could not get a lock immediately")]
    ImmediateFailure,
}

/// Implemented for types (locks) that can return a read-only guard with respect to a given
/// runtime-defined strategy. Currently only implemented for [parking_lot]'s [read-write locks].
///
/// [read-write locks]: parking_lot::RwLock
/// [parking_lot]: https://docs.rs/parking_lot/0.12.3/parking_lot/
pub trait StrategicReadLock {
    /// The read-only guard type for this implementor.
    type RGuard<'a>
    where
        Self: 'a;

    /// Get a read-only guard using the given `strategy`.
    /// The returned error depends on the `strategy`.
    /// See documentation on [`LockStrategy`] and [`StrategySyncError`] for more information.
    fn strategic_read(&self, strategy: LockStrategy)
        -> Result<Self::RGuard<'_>, StrategySyncError>;
}

/// Implemented for types (locks) that can return a read/write guard with respect to a given
/// runtime-defined strategy. Currently only implemented for [parking_lot]'s [read-write locks].
///
/// [read-write locks]: parking_lot::RwLock
/// [parking_lot]: https://docs.rs/parking_lot/0.12.3/parking_lot/
pub trait StrategicWriteLock {
    /// The read/write guard type for this implementor.
    type WGuard<'a>
    where
        Self: 'a;

    /// Get a read/write guard using the given `strategy`.
    /// The returned error depends on the `strategy`.
    /// See documentation on [`LockStrategy`] and [`StrategySyncError`] for more information.
    fn strategic_write(
        &self,
        strategy: LockStrategy,
    ) -> Result<Self::WGuard<'_>, StrategySyncError>;
}

impl<T> StrategicReadLock for parking_lot::RwLock<T> {
    type RGuard<'a> = parking_lot::RwLockReadGuard<'a, T> where T: 'a;

    #[inline]
    fn strategic_read(
        &self,
        strategy: LockStrategy,
    ) -> Result<Self::RGuard<'_>, StrategySyncError> {
        match strategy {
            LockStrategy::Timeout(dur) => self
                .try_read_for(dur)
                .ok_or(StrategySyncError::Timeout(dur)),
            LockStrategy::Immediate => self.try_read().ok_or(StrategySyncError::ImmediateFailure),
            LockStrategy::Blocking => Ok(self.read()),
        }
    }
}

impl<T> StrategicWriteLock for parking_lot::RwLock<T> {
    type WGuard<'a> = parking_lot::RwLockWriteGuard<'a, T> where T: 'a;

    #[inline]
    fn strategic_write(
        &self,
        strategy: LockStrategy,
    ) -> Result<Self::WGuard<'_>, StrategySyncError> {
        match strategy {
            LockStrategy::Timeout(dur) => self
                .try_write_for(dur)
                .ok_or(StrategySyncError::Timeout(dur)),
            LockStrategy::Immediate => self.try_write().ok_or(StrategySyncError::ImmediateFailure),
            LockStrategy::Blocking => Ok(self.write()),
        }
    }
}

impl<T> StrategicWriteLock for parking_lot::Mutex<T> {
    type WGuard<'a> = parking_lot::MutexGuard<'a, T> where T: 'a;

    #[inline]
    fn strategic_write(
        &self,
        strategy: LockStrategy,
    ) -> Result<Self::WGuard<'_>, StrategySyncError> {
        match strategy {
            LockStrategy::Timeout(dur) => self
                .try_lock_for(dur)
                .ok_or(StrategySyncError::Timeout(dur)),
            LockStrategy::Immediate => self.try_lock().ok_or(StrategySyncError::ImmediateFailure),
            LockStrategy::Blocking => Ok(self.lock()),
        }
    }
}
