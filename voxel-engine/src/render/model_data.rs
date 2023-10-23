use bevy::prelude::{Vec3, Vec2};

use crate::{util::{FaceMap, Axis2D, AxisMagnitude}, data::tile::Face};

#[derive(Clone)]
pub struct VoxelModel {
    global: ModelData,
    cull_groups: FaceMap<ModelData>
}

#[derive(Clone)]
pub struct ModelData {
    quads: Vec<GreedyQuad>,
    submesh: ModelDataSubmesh
}

#[derive(Clone)]
pub struct ModelDataSubmesh {
    pub indices: Vec<u32>,
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub texture_ids: Vec<u32>,
}

#[derive(Copy, Clone)]
pub struct GreedyQuad {
    rotatation: AxisMagnitude,
    dims: GreedyDims,
    pos: Vec3
}

impl GreedyQuad {
    pub fn rotate_to_face(self, face: Face) -> Self {
        todo!()
    }
}

#[derive(Copy, Clone)]
pub struct GreedyDims {
    stretch: Axis2D,
    other: f32
}

impl ModelData {
    pub fn rotate_to_face(&self, face: Face) -> Self {
        let rot = Face::North.rotation_between(face);

        todo!()
    }
}