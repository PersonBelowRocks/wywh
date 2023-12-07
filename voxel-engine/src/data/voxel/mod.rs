use bevy::math::Vec2;

use crate::util::FaceMap;

use self::rotations::{BlockModelFace, BlockModelFaceMap, BlockModelRotation};

use super::{
    texture::{FaceTexture, FaceTextureRotation},
    tile::{Face, Transparency},
};

pub mod descriptor;
pub mod rotations;
pub mod serialization;

#[derive(Clone)]
pub struct VoxelProperties {
    pub transparency: Transparency,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockModel {
    pub textures: BlockModelFaceMap<FaceTexture>,
}

impl BlockModel {
    pub fn from_textures(textures: BlockModelFaceMap<FaceTexture>) -> Self {
        Self { textures }
    }

    pub fn filled_with_tex_pos(tex_pos: Vec2) -> Self {
        Self {
            textures: BlockModelFaceMap::filled(FaceTexture::new(tex_pos)),
        }
    }

    pub fn faces_for_rotation(&self, rotation: BlockModelRotation) -> FaceMap<FaceTexture> {
        let mut map = FaceMap::new();
        for face in BlockModelFace::FACES {
            if let Some(mut tex) = self.texture(face) {
                tex.rotation = match face {
                    BlockModelFace::Up | BlockModelFace::Down => {
                        FaceTextureRotation::new(rotation.yaw())
                    }
                    BlockModelFace::Front => FaceTextureRotation::new(-rotation.roll()),
                    BlockModelFace::Back => FaceTextureRotation::new(rotation.roll()),
                    BlockModelFace::Left => FaceTextureRotation::new(-rotation.pitch()),
                    BlockModelFace::Right => FaceTextureRotation::new(rotation.pitch()),
                };

                map.set(rotation.get_cardinal_face(face), tex);
            }
        }

        map
    }

    pub fn texture(&self, face: BlockModelFace) -> Option<FaceTexture> {
        self.textures.get(face).copied()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoxelModel {
    Block(BlockModel),
}

impl VoxelModel {
    pub fn as_block_model(self) -> Option<BlockModel> {
        match self {
            Self::Block(model) => Some(model),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::vec2;

    use super::*;

    const UP: Vec2 = vec2(0.0, 0.0);
    const DOWN: Vec2 = vec2(1.0, 0.0);
    const LEFT: Vec2 = vec2(2.0, 0.0);
    const RIGHT: Vec2 = vec2(3.0, 0.0);
    const FRONT: Vec2 = vec2(4.0, 0.0);
    const BACK: Vec2 = vec2(5.0, 0.0);

    fn default_model() -> BlockModel {
        use BlockModelFace::*;

        let mut textures = BlockModelFaceMap::<FaceTexture>::new();

        textures.set(Up, FaceTexture::new(UP));
        textures.set(Down, FaceTexture::new(DOWN));
        textures.set(Left, FaceTexture::new(LEFT));
        textures.set(Right, FaceTexture::new(RIGHT));
        textures.set(Front, FaceTexture::new(FRONT));
        textures.set(Back, FaceTexture::new(BACK));
        BlockModel::from_textures(textures)
    }

    #[test]
    fn test_block_model_default_rotation() {
        let model = default_model();

        let rotation = BlockModelRotation::new(Face::North, Face::Top).unwrap();
        let textures = model.faces_for_rotation(rotation);

        let top = *textures.get(Face::Top).unwrap();
        let bottom = *textures.get(Face::Bottom).unwrap();
        let north = *textures.get(Face::North).unwrap();
        let east = *textures.get(Face::East).unwrap();
        let south = *textures.get(Face::South).unwrap();
        let west = *textures.get(Face::West).unwrap();

        assert_eq!(UP, top.tex_pos());
        assert_eq!(DOWN, bottom.tex_pos());
        assert_eq!(FRONT, north.tex_pos());
        assert_eq!(RIGHT, east.tex_pos());
        assert_eq!(BACK, south.tex_pos());
        assert_eq!(LEFT, west.tex_pos());

        assert_eq!(FaceTextureRotation::new(0), top.rotation);
        assert_eq!(FaceTextureRotation::new(0), bottom.rotation);
        assert_eq!(FaceTextureRotation::new(0), north.rotation);
        assert_eq!(FaceTextureRotation::new(0), east.rotation);
        assert_eq!(FaceTextureRotation::new(0), south.rotation);
        assert_eq!(FaceTextureRotation::new(0), west.rotation);
    }

    #[test]
    fn test_block_model_facing_east() {
        let model = default_model();

        let rotation = BlockModelRotation::new(Face::East, Face::Top).unwrap();
        let textures = model.faces_for_rotation(rotation);

        let top = *textures.get(Face::Top).unwrap();
        let bottom = *textures.get(Face::Bottom).unwrap();
        let north = *textures.get(Face::North).unwrap();
        let east = *textures.get(Face::East).unwrap();
        let south = *textures.get(Face::South).unwrap();
        let west = *textures.get(Face::West).unwrap();

        assert_eq!(UP, top.tex_pos());
        assert_eq!(DOWN, bottom.tex_pos());
        assert_eq!(LEFT, north.tex_pos());
        assert_eq!(FRONT, east.tex_pos());
        assert_eq!(RIGHT, south.tex_pos());
        assert_eq!(BACK, west.tex_pos());

        assert_eq!(FaceTextureRotation::new(1), top.rotation);
        assert_eq!(FaceTextureRotation::new(1), bottom.rotation);
        assert_eq!(FaceTextureRotation::new(0), north.rotation);
        assert_eq!(FaceTextureRotation::new(0), east.rotation);
        assert_eq!(FaceTextureRotation::new(0), south.rotation);
        assert_eq!(FaceTextureRotation::new(0), west.rotation);
    }

    #[test]
    fn test_block_model_facing_top() {
        let model = default_model();

        let rotation = BlockModelRotation::new(Face::Top, Face::South).unwrap();
        let textures = model.faces_for_rotation(rotation);

        let top = *textures.get(Face::Top).unwrap();
        let bottom = *textures.get(Face::Bottom).unwrap();
        let north = *textures.get(Face::North).unwrap();
        let east = *textures.get(Face::East).unwrap();
        let south = *textures.get(Face::South).unwrap();
        let west = *textures.get(Face::West).unwrap();

        assert_eq!(FRONT, top.tex_pos());
        assert_eq!(BACK, bottom.tex_pos());
        assert_eq!(DOWN, north.tex_pos());
        assert_eq!(RIGHT, east.tex_pos());
        assert_eq!(UP, south.tex_pos());
        assert_eq!(LEFT, west.tex_pos());

        assert_eq!(FaceTextureRotation::new(0), top.rotation);
        assert_eq!(FaceTextureRotation::new(0), bottom.rotation);
        assert_eq!(FaceTextureRotation::new(0), north.rotation);
        assert_eq!(FaceTextureRotation::new(1), east.rotation);
        assert_eq!(FaceTextureRotation::new(0), south.rotation);
        assert_eq!(FaceTextureRotation::new(-1), west.rotation);
    }
}
