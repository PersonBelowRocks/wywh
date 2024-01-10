use crate::data::{
    registries::{texture::TextureRegistry, RegistryId},
    texture::FaceTexture,
};

use super::{anon::Quad, isometric::QuadVertex};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct QVertexData;

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub struct QData([QVertexData; 4]);

impl QData {
    #[inline]
    pub fn new() -> Self {
        Self([QVertexData::default(); 4])
    }

    #[inline]
    pub fn get(&self, vertex: QuadVertex) -> &QVertexData {
        &self.0[vertex.as_usize()]
    }

    #[inline]
    pub fn get_mut(&mut self, vertex: QuadVertex) -> &mut QVertexData {
        &mut self.0[vertex.as_usize()]
    }

    #[inline]
    pub fn inner(&self) -> &[QVertexData; 4] {
        &self.0
    }
}

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub struct DataQuad {
    pub quad: Quad,
    pub texture: FaceTexture,
    pub data: QData,
}

impl DataQuad {
    pub fn new(quad: Quad, texture: FaceTexture) -> Self {
        Self {
            quad,
            texture,
            data: QData::new(),
        }
    }
}
