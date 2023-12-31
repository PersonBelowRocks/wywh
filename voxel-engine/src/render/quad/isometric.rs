use bevy::math::{vec2, vec3, IVec3, Vec2, Vec3};
use ordered_float::{FloatIsNan, NotNan};

use crate::{
    data::tile::Face,
    util::{notnan::NotNanVec2, Axis3D},
};

use super::{anon::Quad, data::QData};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct QuadIsometry {
    pub face: Face,
    magnitude: NotNan<f32>,
    pos: NotNanVec2,
}

impl QuadIsometry {
    #[inline]
    pub fn new(pos: Vec2, magnitude: f32, face: Face) -> Result<Self, FloatIsNan> {
        Ok(Self {
            face,
            magnitude: NotNan::new(magnitude)?,
            pos: pos.try_into()?,
        })
    }

    #[inline]
    pub fn pos_3d(self) -> Vec3 {
        todo!()
    }
}

/*
    0---1
    |   |
    2---3
*/

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum QuadVertex {
    BottomLeft = 2,
    BottomRight = 3,
    TopLeft = 0,
    TopRight = 1,
}

impl QuadVertex {
    pub const VERTICES: [Self; 4] = [
        Self::BottomLeft,
        Self::BottomRight,
        Self::TopLeft,
        Self::TopRight,
    ];

    #[inline]
    pub fn as_usize(self) -> usize {
        match self {
            Self::BottomLeft => 2,
            Self::BottomRight => 3,
            Self::TopLeft => 0,
            Self::TopRight => 1,
        }
    }

    #[inline]
    pub fn from_usize(v: usize) -> Option<Self> {
        match v {
            2 => Some(Self::BottomLeft),
            3 => Some(Self::BottomRight),
            0 => Some(Self::TopLeft),
            1 => Some(Self::TopRight),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PositionedQuad<Data: Copy> {
    pos: NotNanVec2,
    pub quad: Quad,
    pub data: QData<Data>,
}

impl<Data: Copy> PositionedQuad<Data> {
    #[inline]
    pub fn new(pos: Vec2, quad: Quad, data: QData<Data>) -> Result<Self, FloatIsNan> {
        Ok(Self {
            pos: pos.try_into()?,
            quad,
            data,
        })
    }

    #[inline]
    pub fn widen(&mut self, by: f32) {
        self.quad = self.quad.widened(by);
    }

    #[inline]
    pub fn heighten(&mut self, by: f32) {
        self.quad = self.quad.heightened(by);
    }

    #[inline]
    pub fn pos(&self) -> Vec2 {
        self.pos.vec()
    }

    pub fn width(&self) -> f32 {
        self.quad.x()
    }

    pub fn height(&self) -> f32 {
        self.quad.y()
    }

    #[inline]
    pub fn vertex_pos(&self, vertex: QuadVertex) -> Vec2 {
        let pos = self.pos();

        match vertex {
            QuadVertex::BottomLeft => pos,
            QuadVertex::BottomRight => vec2(pos.x + self.width(), pos.y),
            QuadVertex::TopLeft => vec2(pos.x, pos.y + self.height()),
            QuadVertex::TopRight => vec2(pos.x + self.width(), pos.y + self.height()),
        }
    }

    #[inline]
    pub fn get_vertex(&self, vertex: QuadVertex) -> (Vec2, &Data) {
        let pos = self.vertex_pos(vertex);
        let data = self.data.get(vertex);

        (pos, data)
    }

    #[inline]
    pub fn get_vertex_mut(&mut self, vertex: QuadVertex) -> (Vec2, &mut Data) {
        let pos = self.vertex_pos(vertex);
        let data = self.data.get_mut(vertex);

        (pos, data)
    }
}

#[inline]
pub fn project_to_3d(pos: Vec2, face: Face, mag: f32) -> Vec3 {
    match face.axis() {
        Axis3D::X => vec3(mag, pos.y, pos.x),
        Axis3D::Y => vec3(pos.x, mag, pos.y),
        Axis3D::Z => vec3(pos.x, pos.y, mag),
    }
}

#[inline]
pub fn project_to_2d(pos: Vec3, face: Face) -> Vec2 {
    match face.axis() {
        Axis3D::X => vec2(pos.z, pos.y),
        Axis3D::Y => vec2(pos.x, pos.z),
        Axis3D::Z => vec2(pos.x, pos.y),
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct IsometrizedQuad<Data: Copy> {
    pub isometry: QuadIsometry,
    pub quad: PositionedQuad<Data>,
}

impl<Data: Copy> IsometrizedQuad<Data> {
    pub fn pos_3d(&self, vertex: QuadVertex) -> Vec3 {
        let pos_2d = self.quad.vertex_pos(vertex);
        project_to_3d(pos_2d, self.isometry.face, self.isometry.magnitude.into())
    }

    pub fn data(&self) -> &[Data; 4] {
        self.quad.data.inner()
    }

    pub fn topology(&self) -> [u32; 6] {
        todo!()
    }

    pub fn get_vertex(&self, vertex: QuadVertex) -> (Vec3, &Data) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projection() {
        let pos = vec2(1.0, 2.0);

        assert_eq!(vec3(10.0, 2.0, 1.0), project_to_3d(pos, Face::North, 10.0));
        assert_eq!(
            vec2(1.0, 2.0),
            project_to_2d(vec3(10.0, 2.0, 1.0), Face::North)
        );
    }
}
