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
use bevy::math::{ivec2, ivec3, IVec2, IVec3, Vec3Swizzles};
use enum_map::{enum_map, EnumMap};
use itertools::Itertools;
use octo::voxelmap::VoxelMap;
use std::array;
use std::cell::RefCell;
use std::collections::VecDeque;

/// Abbreviated as CCG in many other places.
///
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

fn scan<IsFillable>(
    lz: i32,
    rz: i32,
    aisle: IVec2,
    is_fillable: &IsFillable,
    stack: &mut VecDeque<IVec3>,
) where
    IsFillable: Fn(IVec3) -> bool,
{
    let mut span_added = false;

    for z in lz..rz {
        let pos = aisle.extend(z);
        if is_fillable(pos) && !span_added {
            stack.push_back(pos);
            span_added = true;
        }
    }
}

// TODO: benchmark and find the best order of the axes to iterate in
/// A cache-friendly flood fill algorithm.
#[inline]
fn span_flood_fill<IsFillable, SetFilled>(
    initial_pos: IVec3,
    is_fillable: IsFillable,
    mut set_filled: SetFilled,
    stack: &mut VecDeque<IVec3>,
) where
    IsFillable: Fn(IVec3) -> bool,
    SetFilled: FnMut(IVec3),
{
    assert!(
        stack.is_empty(),
        "Stack must be empty to be used by the flood filler"
    );
    if !is_fillable(initial_pos) {
        return;
    }

    stack.push_back(initial_pos);

    while let Some(pos) = stack.pop_back() {
        // These parameters define the aisle span we're going to fill
        let mut z = pos.z;
        let mut lz = pos.z;

        // Widen down
        while is_fillable(pos.with_z(lz - 1)) {
            set_filled(pos.with_z(lz - 1));
            lz -= 1;
        }

        // Widen up
        while is_fillable(pos.with_z(z)) {
            set_filled(pos.with_z(z));
            z += 1;
        }

        // Directions to the aisles on the sides of another aisle
        const AISLE_SPAN_SIDE_NORMALS: [IVec2; 4] =
            [ivec2(-1, -1), ivec2(-1, 1), ivec2(1, -1), ivec2(1, 1)];

        // Scan every aisle next to this one
        for aisle_side in AISLE_SPAN_SIDE_NORMALS {
            scan(lz, z - 1, pos.xy() + aisle_side, &is_fillable, stack);
        }
    }
}

/// A simple recursive flood fill algorithm.
/// Not actually recursive, but uses the provided [`VecDeque`] to queue positions.
/// Make sure the queue is cleared before providing it to this function.
#[inline]
fn classic_flood_fill<IsFillable, SetFilled>(
    initial_pos: IVec3,
    is_fillable: IsFillable,
    mut set_filled: SetFilled,
    queue: &mut VecDeque<IVec3>,
) where
    IsFillable: Fn(IVec3) -> bool,
    SetFilled: FnMut(IVec3),
{
    if !is_fillable(initial_pos) {
        return;
    }
    queue.push_back(initial_pos);

    while let Some(pos) = queue.pop_front() {
        if is_fillable(pos) {
            set_filled(pos);

            for face_normal in Face::FACES.map(Face::normal) {
                queue.push_back(pos + face_normal)
            }
        }
    }
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

    let fill_grid = RefCell::new(FillGrid::new(u32::MAX));
    let mut fill_map_regions = Vec::<FaceSet>::new();

    let mut queue = VecDeque::with_capacity(1024);

    'face_loop: for face in Face::FACES {
        let min_mb_face_pos = interior_chunk_face_mb_position(IVec2::ZERO, face);
        // Need to subtract one from the maximum here since we're iterating inclusive of the maximum position.
        let max_mb_face_pos =
            interior_chunk_face_mb_position(IVec2::splat(CHUNK_MICROBLOCK_DIMS as i32 - 1), face);

        for mb_pos in cartesian_grid!(min_mb_face_pos..=max_mb_face_pos) {
            // Will be 'None' if this position is not associated with a region,
            // and 'Some(region_index)' if it is associated with a region.
            let maybe_region_index: Option<u32> =
                match *fill_grid.borrow().get(mb_pos.as_uvec3()).unwrap() {
                    u32::MAX => None,
                    region_index => Some(region_index),
                };

            let target_faceset = match maybe_region_index {
                None => {
                    // If this position does not already belong to a region,
                    // then we start a flood fill at this position to create a new region/faceset.
                    let current_region_index = fill_map_regions.len() as u32;

                    // Predicate to test if a position is unfilled by this region.
                    let is_fillable = |mb_pos: IVec3| {
                        let Ok(is_unfilled) = fill_grid
                            .borrow()
                            .get(mb_pos.as_uvec3())
                            .map(|&region_index| region_index == u32::MAX)
                        else {
                            return false;
                        };

                        let Ok(is_transparent) =
                            chunk.get_mb(mb_pos).map(|block| !is_opaque(block))
                        else {
                            return false;
                        };

                        is_unfilled && is_transparent
                    };

                    // Closure to set a position as filled by this region.
                    let set_filled = |mb_pos: IVec3| {
                        let mut borrow = fill_grid.borrow_mut();
                        let mutable_region_index = borrow.get_mut(mb_pos.as_uvec3()).unwrap();
                        *mutable_region_index = current_region_index;

                        // Get or insert the faceset associated with this region
                        let faceset = match fill_map_regions.get_mut(current_region_index as usize)
                        {
                            Some(faceset) => faceset,
                            None => {
                                fill_map_regions.push(FaceSet::empty());
                                &mut fill_map_regions[current_region_index as usize]
                            }
                        };

                        // Add all the faces that we're touching to this faceset.
                        *faceset |= faceset_of_touched_faces(mb_pos);
                    };

                    span_flood_fill(mb_pos, is_fillable, set_filled, &mut queue);
                    // Clear the queue here just in case, even though it should always be empty after
                    // the flood fill has run.
                    queue.clear();

                    // If the flood fill started at an opaque position it won't have created a faceset
                    // for this region!
                    fill_map_regions.get(current_region_index as usize).copied()
                }
                // If this position already belongs to a region, then we just return that region's
                // faceset so that we can copy its connections.
                // Using the panicky index operator here is okay since we checked that this region-index
                // can't be a max value (aka. unfilled) earlier!
                Some(region_index) => Some(fill_map_regions[region_index as usize]),
            };

            // Update the graph if this flood fill led to the discovery of another face.
            if let Some(faceset) = target_faceset {
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

/// Abbreviated as CCSG in many other places.
///
/// A "web" of multiple CCGs (chunk connectivity graph) stitched together.
///
/// Provides methods that calculate the visibility of chunks.
/// The motivating use-case for this type is to perform the cave-culling algorithm.
///
/// In order to use the CCSG you must add all the CCGs that you want to consider in a visibility
/// check, and then provide an initial position and frustum for the check itself.
///
/// When used over the span of multiple frames, the state of the CCSG should be maintained to reflect
/// the state of chunks in the world. When the CCG of a chunk updates, ensure that the new CCG is added
/// to the CCSG.
// TODO: implement!
// TODO: test!
#[derive(Clone)]
pub struct ChunkConnectivitySupergraph {
    subgraphs: VoxelMap<ChunkConnectivityGraph>,
}

#[cfg(test)]
mod graph_construction {
    use super::*;
    use crate::topo::mock_chunk::MockChunk;
    use octo::Region;

    fn is_opaque(id: BlockVariantId) -> bool {
        !matches!(id, MockChunk::VOID)
    }

    /// Asserts that a CCG is like a vertical "tunnel", where only the top and bottom faces are
    /// connected to eachother, and all other paths are blocked off.
    #[rustfmt::skip] // looks better this way
    fn assert_graph_is_vertical_tunnel(graph: ChunkConnectivityGraph) {
        // These faces are connected because of the "tunnel" between them
        assert!(graph.has_connection(Face::Top, Face::Bottom));

        assert_eq!(FaceSet::from(Face::North), graph.get_connections(Face::North));
        assert_eq!(FaceSet::from(Face::East), graph.get_connections(Face::East));
        assert_eq!(FaceSet::from(Face::South), graph.get_connections(Face::South));
        assert_eq!(FaceSet::from(Face::West), graph.get_connections(Face::West));

        // Top and bottom faces are connected to eachother (and to themselves obviously)
        assert_eq!(FaceSet::from_iter([Face::Top, Face::Bottom]), graph.get_connections(Face::Top));
        assert_eq!(FaceSet::from_iter([Face::Top, Face::Bottom]), graph.get_connections(Face::Bottom));
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

    #[rustfmt::skip]
    #[test]
    fn construct_graph_for_chunk_with_tunnel() {
        let mut chunk = MockChunk::new();

        // There's a vertical path through the chunk between the top and bottom faces, but all other
        // sides of the chunk are walled off, creating a "tunnel" that can be traversed.
        chunk.fill_region(Region::new([0, 0, 0],   [16, 16, 1]), MockChunk::EXAMPLE1).unwrap();
        chunk.fill_region(Region::new([0, 0, 0],   [1, 16, 16]), MockChunk::EXAMPLE1).unwrap();
        chunk.fill_region(Region::new([15, 0, 16], [16, 16, 0]), MockChunk::EXAMPLE1).unwrap();
        chunk.fill_region(Region::new([16, 0, 15], [0, 16, 16]), MockChunk::EXAMPLE1).unwrap();

        let graph = connectivity_graph_construction_impl(&chunk, is_opaque);
    
        assert_graph_is_vertical_tunnel(graph);
    }

    #[test]
    fn construct_graph_with_chunk_with_1block_tunnel() {
        let mut chunk = MockChunk::new();

        chunk
            .fill_region(Region::new([0, 0, 0], [16, 16, 16]), MockChunk::EXAMPLE1)
            .unwrap();
        // Tiny tunnel running between the top and bottom faces
        chunk
            .fill_region(Region::new([8, 0, 8], [9, 16, 9]), MockChunk::VOID)
            .unwrap();

        let graph = connectivity_graph_construction_impl(&chunk, is_opaque);

        assert_graph_is_vertical_tunnel(graph);
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
