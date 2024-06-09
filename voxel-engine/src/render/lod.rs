use enum_map::{Enum, EnumMap};

/// Level of detail of a chunk mesh.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Enum)]
pub enum LevelOfDetail {
    /// Chunk is rendered as a 1x1x1 cube
    /// Lowest level of detail, only 1 quad per face is allowed, making the entire chunk one big "block"
    X1 = 0,
    /// Chunk is rendered as a 2x2x2 cube
    X2 = 1,
    /// Chunk is rendered as a 4x4x4 cube
    X4 = 2,
    /// Chunk is rendered as a 8x8x8 cube
    X8 = 3,
    /// Chunk is rendered as a 16x16x16 cube without any microblocks
    X16 = 4,
    /// Chunk is rendered as a 16x16x16 cube with microblocks. Highest level of detail.
    /// This is the "true" appearence of a chunk.
    X16Subdiv = 5,
}

/// Type alias for an enum map that maps levels of detail to values of a type.
pub type LodMap<T> = EnumMap<LevelOfDetail, T>;
