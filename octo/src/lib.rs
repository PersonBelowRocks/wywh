//! An octree library for simple and fast cubic octrees
//!
//!

use derive_new::new;
use glam::UVec3;
use std::{marker::PhantomData, ops::Range};

/// The maximum depth allowed for an octree
pub const MAX_DEPTH: u8 = u8::MAX / 2;

pub trait MaxDepth: 'static {
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub(crate) struct Octet(NodeIdx);

impl Octet {
    pub fn from_usize(i: usize) -> Self {
        Self(NodeIdx(i as _))
    }

    pub fn indices(&self) -> Range<NodeIdx> {
        self.0..NodeIdx::new(8 + (self.0 .0))
    }

    pub fn octant(&self, octant: OctantPos) -> NodeIdx {
        let idx = octant.to_index() as u32;
        NodeIdx::new(self.indices().start.0 + idx)
    }
}

#[derive(new, Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub(crate) struct NodeIdx(u32);

impl NodeIdx {
    pub fn root() -> Self {
        Self(0)
    }

    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Copy, Clone)]
pub(crate) struct ControlByte(u8);

impl ControlByte {
    pub const fn depth(&self) -> u8 {
        self.0 & 0b01111111
    }

    pub const fn is_leaf(&self) -> bool {
        (self.0 & 0b10000000) != 0
    }

    pub const fn leaf_depth(depth: u8) -> Self {
        Self((depth as u8) | 0b10000000)
    }

    pub fn set_not_leaf(&mut self) {
        self.0 &= 0b01111111;
    }

    pub fn set_leaf(&mut self) {
        self.0 |= 0b10000000;
    }
}

pub(crate) struct Node<D: MaxDepth, T: Copy> {
    ctrl_byte: ControlByte,
    data: NodeData<T>,

    _d: PhantomData<D>,
}

impl<D: MaxDepth, T: Copy> Clone for Node<D, T> {
    fn clone(&self) -> Self {
        Self {
            ctrl_byte: self.ctrl_byte,
            data: self.data,

            _d: PhantomData,
        }
    }
}

impl<D: MaxDepth, T: Copy> Copy for Node<D, T> {}

impl<D: MaxDepth, T: Copy> Node<D, T> {
    pub const fn is_leaf(&self) -> bool {
        self.ctrl_byte.is_leaf()
    }

    pub fn value(&self) -> &T {
        if !self.is_leaf() {
            panic!("Cannot get value of non-leaf node");
        }

        unsafe { &self.data.value }
    }

    pub fn octet(&self) -> Octet {
        if self.is_leaf() {
            panic!("Cannot get octet of leaf node");
        }

        unsafe { self.data.octet }
    }

    pub fn set_leaf(&mut self, value: T) {
        self.ctrl_byte.set_leaf();
        self.data.value = value;
    }

    pub fn set_octet(&mut self, octet: Octet) {
        self.ctrl_byte.set_not_leaf();
        self.data.octet = octet;
    }

    pub const fn new_leaf(depth: u8, value: T) -> Self {
        Self {
            ctrl_byte: ControlByte::leaf_depth(depth),
            data: NodeData { value },

            _d: PhantomData,
        }
    }

    pub const fn get_value(&self) -> Option<&T> {
        if self.ctrl_byte.is_leaf() {
            Some(unsafe { &self.data.value })
        } else {
            None
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) union NodeData<T: Copy> {
    value: T,
    octet: Octet,
}

/// A cubic octree
pub struct Octree<D: MaxDepth, T: Copy + Eq> {
    nodes: Vec<Node<D, T>>,
}

impl<D: MaxDepth, T: Copy + Eq> Octree<D, T> {
    pub const fn dimensions() -> u32 {
        D::SIZE
    }

    pub fn new(value: T) -> Self {
        // SAFETY: depth 0 is a valid depth
        let root = Node::new_leaf(0, value);

        Self { nodes: vec![root] }
    }

    /// Clear the entire octree to the provided value.
    #[inline]
    pub fn clear(&mut self, value: T) {
        // Sets the root node
        self.nodes[0] = Node::new_leaf(0, value);

        self.nodes.drain(1..);
    }

    /// Get a node by its index.
    /// Panics if the index is out of bounds.
    #[inline]
    fn node(&self, idx: NodeIdx) -> &Node<D, T> {
        let index = idx.to_usize();
        &self.nodes[index]
    }

    /// Get a node by its index
    /// Panics if the index is out of bounds.
    #[inline]
    fn node_mut(&mut self, idx: NodeIdx) -> &mut Node<D, T> {
        let index = idx.to_usize();
        &mut self.nodes[index]
    }

    fn root(&self) -> &Node<D, T> {
        &self.nodes[0]
    }

    fn root_mut(&mut self) -> &mut Node<D, T> {
        &mut self.nodes[0]
    }

    // TODO: docs
    #[inline]
    pub fn root_value(&self) -> Option<&T> {
        self.root().get_value()
    }

    #[inline]
    fn insert_at_node(&mut self, node_idx: NodeIdx, node_pos: NodePos, target: NodePos, value: T) {
        let mut node_idx = node_idx;
        let mut node_pos = node_pos;

        {
            let node = self.node(node_idx);
            debug_assert!(node.is_leaf());
        }

        while target.depth != self.node(node_idx).ctrl_byte.depth() {
            let subnode = {
                let node = self.node(node_idx);

                let subnode_depth = node.ctrl_byte.depth() + 1;

                // TODO: depth checking or something
                Node::new_leaf(subnode_depth, *node.value())
            };

            let subnodes_start = self.nodes.len();
            let octet = Octet::from_usize(subnodes_start);
            self.nodes.extend([subnode; 8]);

            let node_mut = self.node_mut(node_idx);
            node_mut.set_octet(octet);

            // TODO: safety
            let octant_index = node_pos.octant_pos(target).to_index();
            node_pos = node_pos.next_level_position(target);

            node_idx = NodeIdx::new((subnodes_start + octant_index) as u32);
        }

        let node = self.node_mut(node_idx);
        node.set_leaf(value);

        dbg!(node_pos);
        dbg!(node_idx);
    }

    fn deepest_existing_node(&self, target: NodePos) -> (NodePos, NodeIdx) {
        let mut cur_node = self.root();
        let mut cur_node_idx = NodeIdx::root();
        let mut cur_node_pos = NodePos::root();

        while !cur_node.is_leaf() && target.depth > cur_node_pos.depth {
            let octet = cur_node.octet();
            let octant_pos = cur_node_pos.octant_pos(target);

            let node_index = octet.octant(octant_pos);

            cur_node = self.node(node_index);
            cur_node_idx = node_index;
            // TODO: safety guarantees
            cur_node_pos = cur_node_pos.next_level_position(target);
        }

        (cur_node_pos, cur_node_idx)
    }

    // TODO: docs
    #[inline]
    pub fn insert(&mut self, target: NodePos, value: T) {
        let (deepest_node_pos, deepest_node_idx) = self.deepest_existing_node(target);
        self.insert_at_node(deepest_node_idx, deepest_node_pos, target, value);
    }

    // TODO: docs
    #[inline]
    pub fn get(&self, target: NodePos) -> &T {
        let (_, deepest_node_idx) = self.deepest_existing_node(target);
        let node = self.node(deepest_node_idx);

        // Self::deepest_existing_node() should only fetch leaves
        debug_assert!(node.is_leaf());

        node.value()
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct OctantPos(u8);

impl OctantPos {
    pub fn from_raw(raw: u8) -> Self {
        debug_assert!(raw < 8);

        Self(raw)
    }

    pub fn from_pos_unchecked(pos: UVec3) -> Self {
        let mut flags = 0u8;

        flags |= ((pos.x != 0) as u8) << 2;
        flags |= ((pos.y != 0) as u8) << 1;
        flags |= ((pos.z != 0) as u8) << 0;

        Self(flags)
    }

    pub fn to_index(self) -> usize {
        self.0 as usize
    }
}

/// The position of a node within an octree.
#[derive(Copy, Clone, Debug)]
pub struct NodePos {
    depth: u8,
    pos: UVec3,
}

impl NodePos {
    /// Create a new node position. The provided position is the position of the node
    /// in the grid at the provided depth.
    #[inline]
    pub const fn new(depth: u8, pos: UVec3) -> Self {
        Self { depth, pos }
    }

    /// The position of the root node in an octree. This will be the same for all octrees since
    /// they all have a root node.
    #[inline]
    pub const fn root() -> Self {
        Self::new(0, UVec3::ZERO)
    }

    /// Calculates which octant at this node position the target node position is in.
    ///
    /// ## SAFETY
    /// - The target depth must be greater than or equal to the current depth.
    /// - The target position must be inside of this node's octet (this function just finds the *octant*)
    #[inline]
    pub fn octant_pos(&self, target: Self) -> OctantPos {
        debug_assert!(target.depth > self.depth);

        let depth_diff = target.depth - self.depth;
        let depth_diff_between_target_and_next_layer = depth_diff.saturating_sub(1);

        let nxl_dd = depth_diff_between_target_and_next_layer;

        let nxl_pos = target.pos >> nxl_dd;
        let nxl_min = self.pos << 1;

        let out: UVec3 = nxl_pos - nxl_min;

        // None of the components in the out position can be greater than one.
        debug_assert!(!out.cmpgt(UVec3::ONE).any());

        OctantPos::from_pos_unchecked(out)
    }

    // TODO: docs and safety
    #[inline]
    pub fn next_level_position(&self, target: Self) -> NodePos {
        debug_assert!(target.depth > self.depth);

        let depth_diff = target.depth - self.depth;
        let depth_diff_between_target_and_next_layer = depth_diff.saturating_sub(1);

        let nxl_dd = depth_diff_between_target_and_next_layer;

        let nxl_pos = target.pos >> nxl_dd;

        NodePos::new(self.depth + 1, nxl_pos)
    }
}

#[cfg(test)]
mod tests {
    use glam::uvec3;

    use super::*;

    #[test]
    fn test_node_insert_and_get() {
        let mut octree = Octree::<X6, _>::new(42);
        octree.insert(NodePos::new(1, uvec3(1, 1, 1)), 43);

        let value_at_inserted_pos = *octree.get(NodePos::new(1, uvec3(1, 1, 1)));
        assert_eq!(43, value_at_inserted_pos);

        let untouched_positions = [
            uvec3(0, 0, 0),
            uvec3(1, 0, 0),
            uvec3(1, 0, 1),
            uvec3(0, 0, 1),
            uvec3(0, 1, 0),
            uvec3(1, 1, 0),
            // uvec3(1, 1, 1) is the position we wrote to
            uvec3(0, 1, 1),
        ];

        for untouched_pos in untouched_positions {
            let untouched_value = *octree.get(NodePos::new(1, untouched_pos));
            assert_eq!(42, untouched_value);
        }
        // Set this position back to 42
        octree.insert(NodePos::new(1, uvec3(1, 1, 1)), 42);

        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let pos = uvec3(x, y, z);
                    assert_eq!(42, *octree.get(NodePos::new(1, pos)));
                }
            }
        }

        octree.insert(NodePos::new(6, uvec3(3, 3, 3)), 44);

        for x in 0..64 {
            for y in 0..64 {
                for z in 0..64 {
                    let pos = uvec3(x, y, z);
                    // Skip the position we wrote to
                    if pos == uvec3(3, 3, 3) {
                        continue;
                    }

                    assert_eq!(42, *octree.get(NodePos::new(6, pos)));
                }
            }
        }

        assert_eq!(44, *octree.get(NodePos::new(6, uvec3(3, 3, 3))));
    }

    #[test]
    fn test_node_pos_octant_selection() {
        // target_depth_dims=64
        // target_depth=6
        // depth=4
        // self.pos=vec(0, 0, 0)
        // target=vec3(3, 3, 3)
        // -> out=vec3(1, 1, 1)
        // ----------
        // the absolute position in the next grid is vec3(1, 1, 1)

        // target_depth_dims=64
        // target_depth=6
        // depth=4
        // depth_dims=16
        // self.pos=vec(7, 7, 7)
        // target=vec3(28, 31, 28)
        // -> out=vec3(0, 1, 0)
        // ----------
        // the absolute position in the next grid is vec3(14, 15, 14)

        assert_eq!(
            OctantPos::from_pos_unchecked(uvec3(1, 1, 1)),
            NodePos::new(4, uvec3(0, 0, 0)).octant_pos(NodePos::new(6, uvec3(3, 3, 3)))
        );

        assert_eq!(
            OctantPos::from_pos_unchecked(uvec3(0, 1, 0)),
            NodePos::new(4, uvec3(7, 7, 7)).octant_pos(NodePos::new(6, uvec3(28, 31, 28)))
        );

        // TODO: more thorough testing here
    }

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

    #[test]
    fn test_octant_pos_conversions() {
        let test = |raw: u8, pos: UVec3| -> bool {
            OctantPos::from_raw(raw) == OctantPos::from_pos_unchecked(pos)
        };

        assert!(test(0b00000111, uvec3(1, 1, 1)));
        assert!(test(0b00000101, uvec3(1, 0, 1)));
        assert!(test(0b00000001, uvec3(0, 0, 1)));
        assert!(test(0b00000110, uvec3(1, 1, 0)));
    }
}
