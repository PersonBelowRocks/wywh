use std::ops;

use bevy::{log::info, math::Vec2, render::render_resource::ShaderType};

use super::{
    error::FaceTextureRotationParseError,
    registries::{texture::TextureRegistry, Registry, RegistryId},
};

#[derive(
    Default, Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize,
)]
pub struct FaceTextureRotation(u8);

impl ops::Add<Self> for FaceTextureRotation {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self((self.0 + rhs.0).rem_euclid(Self::TOTAL_ROTATIONS as _))
    }
}

impl ops::AddAssign<Self> for FaceTextureRotation {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl std::str::FromStr for FaceTextureRotation {
    type Err = FaceTextureRotationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s)
            .map(Self::new)
            .map_err(|_| Self::Err::new(s.to_string()))
    }
}

impl FaceTextureRotation {
    pub const TOTAL_ROTATIONS: i32 = 4;
    pub const ONE_TURN_DEG: i32 = 90;
    pub const ONE_TURN_RAD: f32 = 1.57079633;

    pub fn new(value: i32) -> Self {
        let value: u32 = value.rem_euclid(Self::TOTAL_ROTATIONS) as _;
        debug_assert!(value < Self::TOTAL_ROTATIONS as u32);

        Self(value as _)
    }

    pub fn rotate_by(self, rot: i32) -> Self {
        let new_rotation = self.0 as i32 + rot;
        Self::new(new_rotation)
    }

    pub fn degrees(self) -> i32 {
        self.0 as i32 * Self::ONE_TURN_DEG
    }

    pub fn radians(self) -> f32 {
        self.0 as f32 * Self::ONE_TURN_RAD
    }

    pub fn inner(self) -> u8 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct FaceTexture {
    pub rotation: FaceTextureRotation,
    pub texture: RegistryId<TextureRegistry>,
}

impl FaceTexture {
    pub fn color_tex_idx(&self, registry: &TextureRegistry) -> u32 {
        registry.get_by_id(self.texture).texture_idx
    }

    pub fn new(texture: RegistryId<TextureRegistry>) -> Self {
        Self {
            rotation: Default::default(),
            texture,
        }
    }

    pub fn new_rotated(
        texture: RegistryId<TextureRegistry>,
        rotation: FaceTextureRotation,
    ) -> Self {
        Self { rotation, texture }
    }
}

#[derive(Copy, Clone, Debug, Default, ShaderType)]
pub struct GpuFaceTexture {
    pub flags: u32,
    pub color_tex_idx: u32,
    pub normal_tex_idx: u32,
}

impl GpuFaceTexture {
    pub const HAS_NORMAL_MAP_BIT: u32 = 0b1;

    pub fn new(color_idx: u32, normal_idx: Option<u32>) -> Self {
        let mut flags = 0u32;

        if normal_idx.is_some() {
            flags |= Self::HAS_NORMAL_MAP_BIT;
            info!("flags are now: {flags}");
        }

        Self {
            flags,
            color_tex_idx: color_idx,
            normal_tex_idx: normal_idx.unwrap_or(0),
        }
    }
}
