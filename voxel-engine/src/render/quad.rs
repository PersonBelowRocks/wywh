use std::array;

use bevy::prelude::Vec2;
use bevy::prelude::Vec3;

use crate::data::tile::Face;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Quad {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Quad {
    fn from_min_max(min: Vec2, max: Vec2) -> Self {
        let [width, height] = (max - min).abs().to_array();

        Self {
            x: min.x,
            y: min.y,
            width,
            height,
        }
    }

    pub fn from_points(p1: Vec2, p2: Vec2) -> Self {
        Self::from_min_max(p1.min(p2), p1.max(p2))
    }

    pub fn min(self) -> Vec2 {
        [self.x, self.y].into()
    }

    pub fn max(self) -> Vec2 {
        [self.x + self.width, self.y + self.height].into()
    }

    pub fn positions(self, face: Face) -> [Vec3; 4] {
        let non_rotated: [Vec2; 4] = {
            let min = self.min();
            let max = self.max();

            [
                [min.x, max.y],
                [max.x, max.y],
                [min.x, min.y],
                [max.x, min.y],
            ]
            .map(Into::into)
        };

        array::from_fn(|i| {
            let v = non_rotated[i];

            match face {
                Face::Top => [v.x, 1.0, v.y],
                Face::Bottom => [v.x, 0.0, v.y],
                Face::North => [1.0, v.x, v.y],
                Face::East => [v.x, v.y, 1.0],
                Face::South => [0.0, v.x, v.y],
                Face::West => [v.x, v.y, 0.0],
            }
            .into()
        })
    }

    #[rustfmt::skip]
    pub fn uvs(self) -> [Vec2; 4] {
        let span = (self.max() - self.min()).abs();

        [
            [0.0, span.y],
            [span.x, span.y],
            [0.0, 0.0],
            [span.x, 0.0]
        ].map(Into::into)
    }
}
