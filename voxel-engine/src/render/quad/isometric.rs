use bevy::math::{vec2, vec3, Vec2, Vec3};
use ordered_float::{FloatIsNan, NotNan};

use crate::{
    data::tile::Face,
    util::{notnan::NotNanVec2, Axis3D},
};

use super::{
    data::{DataQuad, QVertexData},
    error::QuadError,
};

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
        project_to_3d(self.pos.vec(), self.face, self.magnitude.into())
    }

    #[inline]
    pub fn pos_2d(self) -> Vec2 {
        self.pos.vec()
    }
}

/*
    0---1
    |   |
    2---3
*/

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum QuadVertex {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
}

impl QuadVertex {
    pub const VERTICES: [Self; 4] = [Self::Zero, Self::One, Self::Two, Self::Three];

    #[inline]
    pub fn as_usize(self) -> usize {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }

    #[inline]
    pub fn from_usize(v: usize) -> Option<Self> {
        match v {
            0 => Some(Self::Zero),
            1 => Some(Self::One),
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PositionedQuad {
    pos: NotNanVec2,
    dataquad: DataQuad,
}

impl PositionedQuad {
    #[inline]
    pub fn new(pos: Vec2, dataquad: DataQuad) -> Result<Self, FloatIsNan> {
        Ok(Self {
            pos: pos.try_into()?,
            dataquad,
        })
    }

    #[inline]
    pub fn min(&self) -> Vec2 {
        self.pos() - (self.dataquad.quad.dims() / 2.0)
    }

    #[inline]
    pub fn max(&self) -> Vec2 {
        self.pos() + (self.dataquad.quad.dims() / 2.0)
    }

    #[inline]
    pub fn widen(&mut self, by: f32) -> Result<(), QuadError> {
        if by < 0.0 {
            // TODO: negative numbers in resizing functions should expand in the other direction, rather than shrink the quad
            todo!()
        }

        let widened = self.dataquad.quad.widened(by)?;
        let delta = widened.dims() - self.dataquad.quad.dims();

        self.dataquad.quad = widened;

        self.pos = (self.pos() + delta / 2.0).try_into()?;
        Ok(())
    }

    #[inline]
    pub fn heighten(&mut self, by: f32) -> Result<(), QuadError> {
        if by < 0.0 {
            // TODO: negative numbers in resizing functions should expand in the other direction, rather than shrink the quad
            todo!()
        }

        let heightened = self.dataquad.quad.heightened(by)?;
        let delta = heightened.dims() - self.dataquad.quad.dims();

        self.dataquad.quad = heightened;

        self.pos = (self.pos() + delta / 2.0).try_into()?;
        Ok(())
    }

    #[inline]
    pub fn pos(&self) -> Vec2 {
        self.pos.vec()
    }

    pub fn width(&self) -> f32 {
        self.dataquad.quad.x()
    }

    pub fn height(&self) -> f32 {
        self.dataquad.quad.y()
    }

    #[inline]
    pub fn vertex_pos(&self, vertex: QuadVertex) -> Vec2 {
        let pos = self.pos();

        match vertex {
            QuadVertex::Two => pos,
            QuadVertex::Three => vec2(pos.x + self.width(), pos.y),
            QuadVertex::Zero => vec2(pos.x, pos.y + self.height()),
            QuadVertex::One => vec2(pos.x + self.width(), pos.y + self.height()),
        }
    }

    #[inline]
    pub fn get_vertex(&self, vertex: QuadVertex) -> (Vec2, &QVertexData) {
        let pos = self.vertex_pos(vertex);
        let data = self.dataquad.data.get(vertex);

        (pos, data)
    }

    #[inline]
    pub fn get_vertex_mut(&mut self, vertex: QuadVertex) -> (Vec2, &mut QVertexData) {
        let pos = self.vertex_pos(vertex);
        let data = self.dataquad.data.get_mut(vertex);

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
pub struct IsometrizedQuad {
    pub isometry: QuadIsometry,
    pub quad: PositionedQuad,
}

impl IsometrizedQuad {
    pub fn pos_3d(&self, vertex: QuadVertex) -> Vec3 {
        let pos_2d = self.quad.vertex_pos(vertex);
        project_to_3d(pos_2d, self.isometry.face, self.isometry.magnitude.into())
    }

    pub fn data(&self) -> &[QVertexData; 4] {
        self.quad.dataquad.data.inner()
    }

    pub fn topology(&self) -> [QuadVertex; 6] {
        todo!()
    }

    pub fn get_vertex(&self, _vertex: QuadVertex) -> (Vec3, &QVertexData) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        data::{registries::RegistryId, texture::FaceTexture},
        render::quad::anon::Quad,
    };

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

    #[test]
    fn test_resizing() {
        let mut quad = PositionedQuad::new(
            vec2(0.5, 0.5),
            DataQuad::new(
                Quad::new(vec2(1.0, 1.0)).unwrap(),
                FaceTexture::new(RegistryId::new(0)),
            ),
        )
        .unwrap();

        assert_eq!(vec2(0.0, 0.0), quad.min());
        assert_eq!(vec2(1.0, 1.0), quad.max());
        assert_eq!(vec2(0.5, 0.5), quad.pos());

        quad.widen(1.0).unwrap();

        assert_eq!(vec2(0.0, 0.0), quad.min());
        assert_eq!(vec2(2.0, 1.0), quad.max());
        assert_eq!(vec2(1.0, 0.5), quad.pos());

        quad.heighten(2.0).unwrap();

        assert_eq!(vec2(0.0, 0.0), quad.min());
        assert_eq!(vec2(2.0, 3.0), quad.max());
        assert_eq!(vec2(1.0, 1.5), quad.pos());
    }
}
