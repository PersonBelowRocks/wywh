use std::hash::BuildHasher;

use bevy::math::{ivec2, ivec3, IVec2, IVec3, Vec3};
use hb::hash_map::Entry;
use priority_queue::PriorityQueue;
use world::ChunkPos;

use crate::{data::tile::Face, util::Axis3D};
pub mod block;
pub mod bounding_box;
pub mod controller;
pub mod error;
pub mod neighbors;
pub mod transformations;
pub mod world;

pub use transformations::*;

pub use controller::ObserverSettings;

#[inline]
pub fn ivec_project_to_3d(pos: IVec2, face: Face, mag: i32) -> IVec3 {
    match face.axis() {
        Axis3D::X => ivec3(mag, pos.y, pos.x),
        Axis3D::Y => ivec3(pos.x, mag, pos.y),
        Axis3D::Z => ivec3(pos.x, pos.y, mag),
    }
}

#[inline]
pub fn ivec_project_to_2d(pos: IVec3, face: Face) -> IVec2 {
    match face.axis() {
        Axis3D::X => ivec2(pos.z, pos.y),
        Axis3D::Y => ivec2(pos.x, pos.z),
        Axis3D::Z => ivec2(pos.x, pos.y),
    }
}

/// A priority queue keyed with chunk positions.
#[derive(Clone)]
pub struct ChunkJobQueue<T> {
    priorities: PriorityQueue<ChunkPos, u32, rustc_hash::FxBuildHasher>,
    items: hb::HashMap<ChunkPos, T, rustc_hash::FxBuildHasher>,
}

impl<T> ChunkJobQueue<T> {
    pub fn new() -> Self {
        Self {
            priorities: PriorityQueue::default(),
            items: hb::HashMap::default(),
        }
    }

    /// Add an item at the given chunk position and update the priority if the new one is higher.
    /// Returns the old item at this chunk position if it existed.
    #[inline]
    pub fn push(&mut self, chunk_pos: ChunkPos, item: T, priority: u32) -> Option<T> {
        self.priorities.push_increase(chunk_pos, priority);
        let previous_item = self.items.insert(chunk_pos, item);

        previous_item
    }

    /// Add an item into the queue by calling a factory with the existing item (if it exists).
    ///
    /// The `factory` closure takes an optional pair of an item's priority and the item itself.
    /// It returns an optional priority and item to be inserted (and in the process overwrite the existing item).
    ///
    /// For the parameter of the closure:
    /// - `None` means that there was no item associated with the given chunk position.
    /// - `Some(priority, item)` means that there was an item associated with the chunk position,
    /// which the factory can read.
    ///
    /// For the return value of the closure:
    /// - `None` means that nothing will be inserted or mutated in the queue.
    /// - `Some(priority, item)` means that the item will be inserted at the chunk position with the priority.
    ///
    /// # Priority Behaviour
    /// Unlike the regular `ChunkJobQueue::push()` method, this method will overwrite the existing item
    /// regardless of its priority. Meaning you can use this method to decrease the priority of an existing item, whereas
    /// regular `push()` only mutates the queue if the new priority is higher than the existing one.
    #[inline]
    pub fn push_with<F>(&mut self, chunk_pos: ChunkPos, factory: F)
    where
        F: FnOnce(Option<(u32, &T)>) -> Option<(u32, T)>,
    {
        // FIXME: this syntax really sucks
        let pair = (self.get_priority(chunk_pos), self.get(chunk_pos));
        let Some((priority, item)) = (match pair {
            (Some(priority), Some(item)) => factory(Some((priority, item))),
            (None, None) => factory(None),
            _ => unreachable!("methods on this type guarantee that we never have a priority without an item, and vice-versa")
        }) else {
            // Factory produced nothing so we don't insert
            return;
        };

        self.priorities.push(chunk_pos, priority);
        self.items.insert(chunk_pos, item);
    }

    /// Remove an item from the queue, returning it if it existed.
    #[inline]
    pub fn remove(&mut self, chunk_pos: ChunkPos) -> Option<T> {
        self.priorities.remove(&chunk_pos);
        self.items.remove(&chunk_pos)
    }

    /// Get the item at the given chunk position if it exists.
    #[inline]
    pub fn get(&self, chunk_pos: ChunkPos) -> Option<&T> {
        self.items.get(&chunk_pos)
    }

    /// Get the priority for the given chunk position if it exists.
    #[inline]
    pub fn get_priority(&self, chunk_pos: ChunkPos) -> Option<u32> {
        self.priorities.get_priority(&chunk_pos).copied()
    }

    /// Remove the highest priority chunk position from this queue and return it along with its
    /// associated item.
    #[inline]
    pub fn pop(&mut self) -> Option<(ChunkPos, T)> {
        let (chunk_pos, _) = self.priorities.pop()?;
        Some((chunk_pos, self.items.remove(&chunk_pos).unwrap()))
    }

    /// Recalculate the priority for every item in the queue.
    #[inline]
    pub fn recalculate_priorities<F: Fn(ChunkPos, &T) -> u32>(&mut self, callback: F) {
        for (&mut chunk_pos, priority) in self.priorities.iter_mut() {
            let item = self.items.get(&chunk_pos).expect(
                "item should exist in the item hashmap since it was in the internal priority queue",
            );
            *priority = callback(chunk_pos, item);
        }
    }
}
