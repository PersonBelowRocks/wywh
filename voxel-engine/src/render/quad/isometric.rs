use bevy::math::{ivec2, vec2, vec3, IVec2, IVec3, Vec2, Vec3};

use crate::{data::tile::Face, topo::ivec_project_to_3d, util::Axis3D};

use super::{
    data::{DataQuad, QVertexData},
    error::QuadError,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct QuadIsometry {
    pub face: Face,
    magnitude: i32,
    pos: IVec2,
}

impl QuadIsometry {
    #[inline]
    pub fn new(pos: IVec2, magnitude: i32, face: Face) -> Self {
        Self {
            face,
            magnitude,
            pos,
        }
    }

    #[inline]
    pub fn pos_3d(self) -> IVec3 {
        ivec_project_to_3d(self.pos, self.face, self.magnitude)
    }

    #[inline]
    pub fn pos_2d(self) -> IVec2 {
        self.pos
    }

    #[inline]
    pub fn magnitude(self) -> i32 {
        self.magnitude
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
    pub pos: IVec2,
    pub dataquad: DataQuad,
}

impl PositionedQuad {
    #[inline]
    pub fn new(pos: IVec2, dataquad: DataQuad) -> Self {
        Self { pos, dataquad }
    }

    #[inline]
    pub fn min(&self) -> IVec2 {
        self.pos()
    }

    #[inline]
    pub fn max(&self) -> IVec2 {
        self.pos() + (self.dataquad.quad.dims() - IVec2::ONE)
    }

    #[inline]
    pub fn widen(&mut self, by: i32) -> Result<(), QuadError> {
        if by < 0 {
            // TODO: negative numbers in resizing functions should expand in the other direction, rather than shrink the quad
            todo!()
        }

        self.dataquad.quad = self.dataquad.quad.widened(by)?;
        Ok(())
    }

    #[inline]
    pub fn heighten(&mut self, by: i32) -> Result<(), QuadError> {
        if by < 0 {
            // TODO: negative numbers in resizing functions should expand in the other direction, rather than shrink the quad
            todo!()
        }

        self.dataquad.quad = self.dataquad.quad.heightened(by)?;
        Ok(())
    }

    #[inline]
    pub fn pos(&self) -> IVec2 {
        self.pos
    }

    pub fn width(&self) -> i32 {
        self.dataquad.quad.x()
    }

    pub fn height(&self) -> i32 {
        self.dataquad.quad.y()
    }

    #[inline]
    pub fn vertex_pos(&self, vertex: QuadVertex) -> IVec2 {
        /*
            0---1
            |   |
            2---3
        */

        let min = self.min();
        let max = self.max();

        match vertex {
            QuadVertex::Zero => ivec2(min.x, max.y),
            QuadVertex::One => max,
            QuadVertex::Two => min,
            QuadVertex::Three => ivec2(max.x, min.y),
        }
    }

    #[inline]
    pub fn get_vertex(&self, vertex: QuadVertex) -> (IVec2, &QVertexData) {
        let pos = self.vertex_pos(vertex);
        let data = self.dataquad.data.get(vertex);

        (pos, data)
    }

    #[inline]
    pub fn get_vertex_mut(&mut self, vertex: QuadVertex) -> (IVec2, &mut QVertexData) {
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
    pub fn new(isometry: QuadIsometry, quad: PositionedQuad) -> Self {
        Self { isometry, quad }
    }

    pub fn vertex_position_3d(&self, vertex: QuadVertex) -> IVec3 {
        let pos_2d = self.quad.vertex_pos(vertex);
        ivec_project_to_3d(pos_2d, self.isometry.face, self.isometry.magnitude)
    }

    pub fn min_2d(&self) -> IVec2 {
        self.quad.min()
    }

    pub fn max_2d(&self) -> IVec2 {
        self.quad.max()
    }

    pub fn min(&self) -> IVec3 {
        ivec_project_to_3d(self.quad.min(), self.isometry.face, self.isometry.magnitude)
    }

    pub fn max(&self) -> IVec3 {
        ivec_project_to_3d(self.quad.max(), self.isometry.face, self.isometry.magnitude)
    }

    pub fn data(&self) -> &[QVertexData; 4] {
        self.quad.dataquad.data.inner()
    }

    pub fn topology(&self) -> [QuadVertex; 6] {
        match self.isometry.face {
            Face::Bottom | Face::East | Face::North => [0, 2, 1, 1, 2, 3],
            _ => [0, 1, 2, 1, 3, 2],
        }
        .map(QuadVertex::from_usize)
        .map(Option::unwrap)
    }

    #[inline]
    pub fn get_vertex(&self, vertex: QuadVertex) -> (IVec3, &QVertexData) {
        (
            self.vertex_position_3d(vertex),
            self.quad.dataquad.data.get(vertex),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        data::{
            registries::{texture::TextureRegistry, Registry},
            texture::FaceTexture,
        },
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
            ivec2(0, 0),
            DataQuad::new(
                Quad::ONE,
                FaceTexture::new(<TextureRegistry as Registry>::Id::new(0)),
            ),
        );

        assert_eq!(ivec2(0, 0), quad.min());
        assert_eq!(ivec2(0, 0), quad.max());
        assert_eq!(ivec2(0, 0), quad.pos());

        quad.widen(1).unwrap();

        assert_eq!(ivec2(0, 0), quad.min());
        assert_eq!(ivec2(1, 0), quad.max());
        assert_eq!(ivec2(0, 0), quad.pos());

        quad.heighten(2).unwrap();

        assert_eq!(ivec2(0, 0), quad.min());
        assert_eq!(ivec2(1, 2), quad.max());
        assert_eq!(ivec2(0, 0), quad.pos());
    }
}
