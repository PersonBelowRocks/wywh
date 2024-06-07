use crate::topo::world::ChunkPos;

#[derive(te::Error, Clone, Debug)]
#[error("Tried to merge event for chunk pos {this} with event for chunk pos {other}")]
pub struct EventPosMismatch {
    pub this: ChunkPos,
    pub other: ChunkPos,
}
