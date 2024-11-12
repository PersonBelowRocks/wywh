//! A CPU-side early occlusion culling / mesh building rejection algorithm based on
//! Minecraft's ACC (advanced cave culling) algorithm.
//!
//! Implementation is largely taken from Tommo's excellent writeup of the algorithm:
//! - Part 1) https://tomcc.github.io/2014/08/31/visibility-1.html
//! - Part 2) https://tomcc.github.io/2014/08/31/visibility-2.html

use bevy::math::IVec3;
use octo::voxelmap::VoxelMap;
use std::array;

use crate::data::registries::block::BlockVariantId;
use crate::topo::generic_chunk::GenericChunkReadAccess;
use crate::topo::{
    fb_localspace_to_min_mb_localspace, transformations, CHUNK_MICROBLOCK_DIMS,
    FULL_BLOCK_MICROBLOCK_DIMS,
};
use crate::util::cubic::Cubic;
use crate::{cartesian_grid, data::tile::Face, util::FaceMap};

/// Describes the connections between the different faces of a chunk.
///
/// Say you are standing above a chunk looking down at its top face.
/// In such a scenario this type would tell you which faces you can exit the chunk through
/// if you were to enter the chunk through the top face. This is used to determine which chunks
/// are visible through another chunk.
///
/// This graph must be rebuilt every time an opaque block is changed in the chunk. Rebuilding
/// the graph is a somewhat expensive operation and should be done sparingly.
#[derive(Copy, Clone)]
pub struct ChunkConnectivityGraph {
    // matrix representation of the graph. top 2 bits in the rows are ignored.
    graph: [u8; 6],
}

impl ChunkConnectivityGraph {
    const FACE_CONNECTIONS_MASK: u8 = 0xFF >> 2;

    /// Create an empty graph with no relationship between any faces.
    /// This would represent a completely solid chunk.
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self {
            graph: array::from_fn(|i| 0b1 << (i as u8)),
        }
    }

    /// Create a graph where all faces are connected to eachother.
    /// This would represent a completely empty chunk.
    #[inline]
    #[must_use]
    pub fn filled() -> Self {
        Self {
            // the top 2 bits are unused and should always be 0
            graph: array::from_fn(|_| Self::FACE_CONNECTIONS_MASK),
        }
    }

    /// Add a connection between two faces.
    ///
    /// There's no direction to the connection so swapping the arguments has no effect.
    #[inline]
    pub fn add_connection(&mut self, first: Face, second: Face) {
        self.graph[first.as_usize()] |= 0b1 << second.as_u8();
        self.graph[second.as_usize()] |= 0b1 << first.as_u8();
    }

    /// Remove a connection between two faces.
    ///
    /// There's no direction to the connection so swapping the arguments has no effect.
    ///
    /// If both faces are the same this operation will be a no-op, since a face is always
    /// accessible through itself.
    #[inline]
    pub fn remove_connection(&mut self, first: Face, second: Face) {
        if first == second {
            return;
        }

        self.graph[first.as_usize()] &= !(0b1 << second.as_u8());
        self.graph[second.as_usize()] &= !(0b1 << first.as_u8());
    }

    /// Check if there's a connection between two faces.
    ///
    /// There's no direction to the connection so swapping the arguments has no effect.
    #[inline]
    #[must_use]
    pub fn has_connection(&self, first: Face, second: Face) -> bool {
        self.graph[first.as_usize()] & (0b1 << second.as_u8()) != 0
    }

    /// Get all the faces connected to the given face (including itself!).
    #[inline]
    pub fn get_connections(&self, face: Face) -> impl Iterator<Item = Face> {
        let connections = self.graph[face.as_usize()];
        Face::FACES
            .into_iter()
            .filter(move |face| connections & (0b1 << face.as_u8()) != 0)
    }

    /// Returns `true` if this graph is "filled" and every face is connected to every other face.
    #[inline]
    #[must_use]
    pub fn is_filled(&self) -> bool {
        self.graph.iter().all(|&f| f == Self::FACE_CONNECTIONS_MASK)
    }
}

pub fn connectivity_graph_construction_impl<C, IsOpaque>(
    chunk: &C,
    is_opaque: IsOpaque,
) -> Result<ChunkConnectivityGraph, C::Error>
where
    C: GenericChunkReadAccess,
    IsOpaque: Fn(BlockVariantId) -> bool,
{
    let filling = Cubic::<{ CHUNK_MICROBLOCK_DIMS as usize }, u32>::new(0);
    let regions = Vec::<u8>::new();

    let mut graph = ChunkConnectivityGraph::empty();

    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filled_graph() {
        let graph = ChunkConnectivityGraph::filled();

        for face1 in Face::FACES {
            for face2 in Face::FACES {
                assert!(graph.has_connection(face1, face2));
            }
        }
    }

    #[test]
    fn test_connection_removal() {
        let mut graph = ChunkConnectivityGraph::empty();

        graph.add_connection(Face::North, Face::South);
        graph.add_connection(Face::North, Face::West);
        graph.add_connection(Face::North, Face::Top);
        graph.add_connection(Face::North, Face::Bottom);

        assert!(graph.has_connection(Face::North, Face::North));
        assert!(graph.has_connection(Face::North, Face::South));
        assert!(graph.has_connection(Face::North, Face::West));
        assert!(graph.has_connection(Face::North, Face::Top));
        assert!(graph.has_connection(Face::North, Face::Bottom));

        graph.remove_connection(Face::North, Face::Top);
        graph.remove_connection(Face::North, Face::South);

        assert!(graph.has_connection(Face::North, Face::North));
        assert!(!graph.has_connection(Face::North, Face::South));
        assert!(graph.has_connection(Face::North, Face::West));
        assert!(!graph.has_connection(Face::North, Face::Top));
        assert!(graph.has_connection(Face::North, Face::Bottom));
    }

    #[test]
    fn test_connection_iter() {
        let graph = ChunkConnectivityGraph::empty();

        let mut connections = graph.get_connections(Face::North);
        assert_eq!(Some(Face::North), connections.next());
        assert_eq!(None, connections.next());

        let mut graph = ChunkConnectivityGraph::empty();
        graph.add_connection(Face::North, Face::South);
        graph.add_connection(Face::North, Face::East);
        graph.add_connection(Face::North, Face::Top);
        graph.add_connection(Face::North, Face::Bottom);

        // The order depends on the order of Face::FACES
        let mut connections = graph.get_connections(Face::North);
        assert_eq!(Some(Face::Top), connections.next());
        assert_eq!(Some(Face::Bottom), connections.next());
        assert_eq!(Some(Face::North), connections.next());
        assert_eq!(Some(Face::East), connections.next());
        assert_eq!(Some(Face::South), connections.next());

        let mut connections = graph.get_connections(Face::East);
        assert_eq!(Some(Face::North), connections.next());
        assert_eq!(Some(Face::East), connections.next());
    }
}
