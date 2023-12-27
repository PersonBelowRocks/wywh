use bevy::math::Vec2;


#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Quad {
    min: Vec2,
    max: Vec2
}