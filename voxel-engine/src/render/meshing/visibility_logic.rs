//! A CPU-side early occlusion culling / mesh building rejection algorithm based on
//! Minecraft's ACC (advanced cave culling) algorithm.
//!
//! Implementation is largely taken from Tommo's excellent writeup of the algorithm:
//! - Part 1) https://tomcc.github.io/2014/08/31/visibility-1.html
//! - Part 2) https://tomcc.github.io/2014/08/31/visibility-2.html

use crate::data::registries::block::BlockVariantId;
use crate::data::tile::FaceSet;
use crate::topo::generic_chunk::GenericChunkReadAccess;
use crate::topo::{
    fb_localspace_to_min_mb_localspace, ivec_project_to_3d, transformations, CHUNK_MICROBLOCK_DIMS,
    FULL_BLOCK_MICROBLOCK_DIMS,
};
use crate::util::cubic::Cubic;
use crate::{cartesian_grid, data::tile::Face, util::FaceMap};
use bevy::math::{IVec2, IVec3};
use enum_map::{enum_map, EnumMap};
use itertools::Itertools;
use octo::voxelmap::VoxelMap;
use std::array;
use std::collections::VecDeque;

/// Describes the connections between the different faces of a chunk.
///
/// Say you are standing above a chunk looking down at its top face.
/// In such a scenario this type would tell you which faces you can exit the chunk through
/// if you were to enter the chunk through the top face. This is used to determine which chunks
/// are visible through another chunk.
///
/// This graph must be rebuilt every time an opaque block is changed in the chunk. Rebuilding
/// the graph is a somewhat expensive operation and should be done sparingly.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct ChunkConnectivityGraph {
    // matrix representation of the graph
    graph: EnumMap<Face, FaceSet>,
}

impl std::fmt::Debug for ChunkConnectivityGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("ChunkConnectivityGraph");

        for face in Face::FACES {
            let connections = self.get_connections(face);
            s.field(&format!("{face}"), &format!("{connections}"));
        }

        s.finish()
    }
}

impl ChunkConnectivityGraph {
    /// Create an empty graph with no relationship between any faces.
    /// This would represent a completely solid chunk.
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self {
            graph: EnumMap::from_fn(FaceSet::from),
        }
    }

    /// Create a graph where all faces are connected to eachother.
    /// This would represent a completely empty chunk.
    #[inline]
    #[must_use]
    pub fn filled() -> Self {
        Self {
            graph: enum_map::enum_map! { _ => FaceSet::all() },
        }
    }

    /// Add a connection between two faces.
    ///
    /// There's no direction to the connection so swapping the arguments has no effect.
    #[inline]
    pub fn add_connection(&mut self, first: Face, second: Face) {
        self.graph[first].insert(second);
        self.graph[second].insert(first);
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

        self.graph[first].remove(second);
        self.graph[second].remove(first);
    }

    /// Check if there's a connection between two faces.
    ///
    /// There's no direction to the connection so swapping the arguments has no effect.
    #[inline]
    #[must_use]
    pub fn has_connection(&self, first: Face, second: Face) -> bool {
        self.graph[first].contains(second)
    }

    /// Get all the faces connected to the given face (including itself!).
    #[inline]
    pub fn get_connections(&self, face: Face) -> FaceSet {
        self.graph[face]
    }

    /// Returns `true` if this graph is "filled" and every face is connected to every other face.
    #[inline]
    #[must_use]
    pub fn is_filled(&self) -> bool {
        self.graph
            .values()
            .all(|&faceset| faceset == FaceSet::all())
    }
}

type FillGrid = Cubic<{ CHUNK_MICROBLOCK_DIMS as usize }, u32>;

/// Get the 3D microblock position of the given `face_pos` on the interior `face` of a chunk.
///
/// Consider the `top` face of a chunk where `face_pos` would be a microblock position on that 2D face.
/// In such a case this function will get the 3D position of the microblock "touching" the `top` face
/// at that 2D position inside the chunk.
fn interior_chunk_face_mb_position(mb_face_pos: IVec2, face: Face) -> IVec3 {
    // Multiply by one less than the maximum since chunk bounds are exclusive of the upper bound.
    let mag = face.axis_direction().clamp(0, 1) * (CHUNK_MICROBLOCK_DIMS as i32 - 1);
    ivec_project_to_3d(mb_face_pos, face, mag)
}

/// Check if a 3D microblock position is touching the given interior face.
fn is_touching_interior_face(mb_pos: IVec3, face: Face) -> bool {
    // Multiply by one less than the maximum since chunk bounds are exclusive of the upper bound.
    let mag = face.axis_direction().clamp(0, 1) * (CHUNK_MICROBLOCK_DIMS as i32 - 1);
    let axis = face.axis().as_usize();
    mb_pos[axis] == mag
}

/// Get a bitset of all the interior chunk faces the given microblock position touches.
/// Will touch at most 3 different faces (if the microblock is in a corner).
fn faceset_of_touched_faces(mb_pos: IVec3) -> FaceSet {
    let mut faceset = FaceSet::empty();

    for face in Face::FACES {
        if is_touching_interior_face(mb_pos, face) {
            faceset.insert(face);
        }
    }

    faceset
}

fn flood_fill<F: Fn(IVec3) -> bool>(
    mb_pos: IVec3,
    queue: &mut VecDeque<IVec3>,
    fill_map: &mut FillGrid,
    fill_map_regions: &mut Vec<FaceSet>,
    is_opaque: F,
) -> Option<FaceSet> {
    // Skip this microblock if it's already been filled or if it's opaque.
    let existing = *fill_map.get(mb_pos.as_uvec3()).unwrap();
    if existing != u32::MAX {
        // If this microblock has already been visited, return the faceset from that visit.
        return Some(fill_map_regions[existing as usize]);
    } else if is_opaque(mb_pos) {
        return None;
    }

    let region_index = fill_map_regions.len();
    fill_map_regions.push(FaceSet::empty());

    queue.push_front(mb_pos);

    while let Some(next_mb_pos) = queue.pop_back() {
        if is_opaque(next_mb_pos) || *fill_map.get(next_mb_pos.as_uvec3()).unwrap() != u32::MAX {
            continue;
        }

        *fill_map.get_mut(next_mb_pos.as_uvec3()).unwrap() = region_index as u32;

        let faceset = faceset_of_touched_faces(next_mb_pos);
        fill_map_regions[region_index] |= faceset;

        // Don't visit microblocks outside of this chunk.
        for face in (!faceset).iter() {
            let face_normal = face.normal();
            queue.push_front(next_mb_pos + face_normal);
        }
    }

    Some(fill_map_regions[region_index])
}

/// Flood-fill based algorithm for constructing a chunk connectivity graph.
pub fn connectivity_graph_construction_impl<C, IsOpaque>(
    chunk: &C,
    is_opaque: IsOpaque,
) -> ChunkConnectivityGraph
where
    C: GenericChunkReadAccess,
    IsOpaque: Fn(BlockVariantId) -> bool,
{
    let mut graph = ChunkConnectivityGraph::empty();

    // Boxing this so that it doesn't blow up the stack.
    let mut fill_map = Box::new(FillGrid::new(u32::MAX));
    let mut fill_map_regions = Vec::<FaceSet>::new();

    let mut queue = VecDeque::with_capacity(1024);

    'face_loop: for face in Face::FACES {
        let min_mb_face_pos = interior_chunk_face_mb_position(IVec2::ZERO, face);
        // Need to subtract one from the maximum here since we're iterating inclusive of the maximum position.
        let max_mb_face_pos =
            interior_chunk_face_mb_position(IVec2::splat(CHUNK_MICROBLOCK_DIMS as i32 - 1), face);

        for mb_pos in cartesian_grid!(min_mb_face_pos..=max_mb_face_pos) {
            let flood_fill_result = flood_fill(
                mb_pos,
                &mut queue,
                &mut fill_map,
                &mut fill_map_regions,
                |mb_pos| {
                    let microblock = chunk.get_mb(mb_pos).unwrap();
                    is_opaque(microblock)
                },
            );

            queue.clear();

            if let Some(faceset) = flood_fill_result {
                for faceset_face in faceset.iter() {
                    graph.add_connection(face, faceset_face);
                }

                // If this face is connected to every other face, adding a new face will be a no-op.
                // We can only add faces through of flood-fill searches, never remove them.
                // For this reason, it's safe to skip to the next face if this face has the max number of connections.
                if graph.get_connections(face) == FaceSet::all() {
                    continue 'face_loop;
                }
            }
        }
    }

    graph
}

#[cfg(test)]
mod graph_construction {
    use super::*;
    use crate::topo::mock_chunk::MockChunk;
    use octo::Region;

    fn is_opaque(id: BlockVariantId) -> bool {
        !matches!(id, MockChunk::VOID)
    }

    #[test]
    fn construct_graph_for_transparent_chunk() {
        // Will be filled with void blocks by default.
        let chunk = MockChunk::new();

        let graph = connectivity_graph_construction_impl(&chunk, is_opaque);
        dbg!(graph);
        assert!(graph.is_filled());
    }

    #[test]
    fn construct_graph_for_chunk_with_walls() {
        let mut chunk = MockChunk::new();

        // 1 wall
        chunk
            .fill_region(Region::new([8, 0, 0], [9, 16, 16]), MockChunk::EXAMPLE1)
            .unwrap();
        let graph = connectivity_graph_construction_impl(&chunk, is_opaque);
        // Graph is not filled since the wall is blocking the chunk
        assert!(!graph.is_filled());

        assert!(graph.has_connection(Face::East, Face::West));
        assert!(graph.has_connection(Face::Top, Face::Bottom));
        // This direction is walled off
        assert!(!graph.has_connection(Face::North, Face::South));
        // But these aren't
        assert!(graph.has_connection(Face::North, Face::Bottom));
        assert!(graph.has_connection(Face::North, Face::Top));
        assert!(graph.has_connection(Face::North, Face::West));
        assert!(graph.has_connection(Face::North, Face::East));
    }
}

#[cfg(test)]
mod graph_logic {
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

        let connections = graph.get_connections(Face::North);
        assert!(connections.contains(Face::North));
        assert_eq!(1, connections.len());

        let mut graph = ChunkConnectivityGraph::empty();
        graph.add_connection(Face::North, Face::South);
        graph.add_connection(Face::North, Face::East);
        graph.add_connection(Face::North, Face::Top);
        graph.add_connection(Face::North, Face::Bottom);

        // The order depends on the order of Face::FACES
        let connections = graph.get_connections(Face::North);
        assert!(connections.contains(Face::Top));
        assert!(connections.contains(Face::Bottom));
        assert!(connections.contains(Face::North));
        assert!(connections.contains(Face::East));
        assert!(connections.contains(Face::South));

        let connections = graph.get_connections(Face::East);
        assert!(connections.contains(Face::North));
        assert!(connections.contains(Face::East));
    }
}
