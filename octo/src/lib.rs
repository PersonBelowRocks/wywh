//! An octree library for simple and fast cubic octrees
//!
//!

use bytemuck::Pod;
use derive_new::new;
use glam::UVec3;
use simba::simd::AutoU32x4;
use std::{cmp::max, marker::PhantomData, num::NonZeroU32};

/// The maximum depth allowed for an octree
pub const MAX_DEPTH: u8 = u8::MAX / 2;

pub trait MaxDepth {
    const DEPTH: u8;
    const SIZE: u32 = 1u32 << Self::DEPTH as u32;
}

macro_rules! depth {
    ($t:ident, $v:literal) => {
        #[derive(Copy, Clone)]
        pub struct $t;

        impl MaxDepth for $t {
            const DEPTH: u8 = $v;
        }
    };
}

depth!(X1, 1);
depth!(X2, 2);
depth!(X3, 3);
depth!(X4, 4);
depth!(X5, 5);
depth!(X6, 6);
depth!(X7, 7);
depth!(X8, 8);

#[derive(new, Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub(crate) struct OctetIdx(NonZeroU32);

impl OctetIdx {
    pub fn to_usize(self) -> usize {
        u32::from(self.0) as usize
    }
}

#[derive(new, Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub(crate) struct ValueIdx(NonZeroU32);

impl ValueIdx {
    pub fn to_usize(self) -> usize {
        u32::from(self.0) as usize
    }
}

#[derive(Copy, Clone)]
pub(crate) struct ControlByte(u8);

impl ControlByte {
    pub const fn is_leaf(&self) -> bool {
        (self.0 & 0b1 << 7) == 0
    }

    pub const unsafe fn leaf_depth(depth: u8) -> Self {
        Self((depth as u8) & 0b01111111)
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct NodePos<D: MaxDepth> {
    depth: u8,
    pos: UVec3,

    _d: PhantomData<D>,
}

impl<D: MaxDepth> NodePos<D> {
    pub unsafe fn octant(&self, target: UVec3) -> usize {
        todo!()
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Octet([ValueIdx; 8]);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct OctantPos(u8);

impl OctantPos {}

pub(crate) struct Node<D: MaxDepth, T: Copy> {
    ctrl_byte: ControlByte,
    data: NodeData<T>,

    _d: PhantomData<D>,
}

impl<D: MaxDepth, T: Copy> Node<D, T> {
    pub const fn is_leaf(&self) -> bool {
        self.ctrl_byte.is_leaf()
    }

    pub const unsafe fn new_leaf(depth: u8, value: T) -> Self {
        Self {
            ctrl_byte: ControlByte::leaf_depth(depth),
            data: NodeData { value },

            _d: PhantomData,
        }
    }

    pub const fn value(&self) -> Option<&T> {
        if self.ctrl_byte.is_leaf() {
            Some(unsafe { &self.data.value })
        } else {
            None
        }
    }
}

pub(crate) union NodeData<T: Copy> {
    value: T,
    octets: OctetIdx,
}

/// A cubic octree
pub struct Octree<D: MaxDepth, T: Copy> {
    values: Vec<Node<D, T>>,
    octants: Vec<Octet>,
}

impl<D: MaxDepth, T: Copy> Octree<D, T> {
    pub const fn dimensions() -> u32 {
        D::SIZE
    }

    pub fn new(value: T) -> Self {
        // SAFETY: depth 0 is a valid depth
        let root = unsafe { Node::new_leaf(0, value) };

        Self {
            values: vec![root],
            octants: vec![],
        }
    }

    /// ## SAFETY
    /// The provided octet index must be valid for this octree
    unsafe fn octet(&self, octants_index: OctetIdx) -> &Octet {
        let index = octants_index.to_usize();

        unsafe { self.octants.get_unchecked(index) }
    }

    fn root(&self) -> &Node<D, T> {
        &self.values[0]
    }

    pub fn root_value(&self) -> Option<&T> {
        self.values[0].value()
    }

    pub fn insert(&mut self, pos: UVec3, value: T) {
        let mut cur_node = self.root();

        while !cur_node.is_leaf() {
            // SAFETY: We know this node is not a leaf
            let octet_index = unsafe { cur_node.data.octets };

            // TODO: safety note
            let octant = unsafe { self.octet(octet_index) };

            todo!();
        }
    }
}

/// The number of octants in the grid at this depth level
pub(crate) const fn octree_level_size(depth: u8) -> u32 {
    // 0 => 1
    // 1 => 8
    // 2 => 64

    8u32.pow(depth as u32)
}

/// The dimensions of the grid at this depth level
pub(crate) const fn octree_level_dimensions(depth: u8) -> u32 {
    // 0 => 1
    // 1 => 2
    // 2 => 4
    // 3 => 8

    0b1u32 << (depth as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_octree_level_size() {
        assert_eq!(1, octree_level_size(0));
        assert_eq!(8, octree_level_size(1));
        assert_eq!(64, octree_level_size(2));
        assert_eq!(512, octree_level_size(3));
    }

    #[test]
    fn test_octree_level_dimensions() {
        assert_eq!(1, octree_level_dimensions(0));
        assert_eq!(2, octree_level_dimensions(1));
        assert_eq!(4, octree_level_dimensions(2));
        assert_eq!(8, octree_level_dimensions(3));
    }
}
