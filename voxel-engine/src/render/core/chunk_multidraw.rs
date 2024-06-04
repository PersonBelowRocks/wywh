use std::ops::Range;

use bevy::{prelude::*, render::render_resource::Buffer};

use crate::util::ChunkMap;

#[derive(Clone)]
pub struct ChunkBufferBounds {
    pub indices: Range<u32>,
    pub quads: Range<u32>,
}

#[derive(Resource, Clone)]
pub struct ChunkMultidrawData {
    md_index_buffer: Buffer,
    md_quad_buffer: Buffer,
}
