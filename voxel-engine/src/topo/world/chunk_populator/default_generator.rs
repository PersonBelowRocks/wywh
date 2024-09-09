use bevy::math::ivec3;
use noise::{utils::NoiseMapBuilder, Perlin};

use crate::{
    data::{
        registries::{
            block::{BlockVariantId, BlockVariantRegistry},
            Registries, Registry,
        },
        resourcepath::rpath,
    },
    topo::world::{chunk::ChunkFlags, ChunkPos},
    util::sync::LockStrategy,
};

use super::worldgen::{WorldgenContext, WorldgenWorker};

/// Palette of blocks to be used by the default world generator.
#[derive(Clone, Debug)]
pub struct WorldGeneratorPalette {
    pub stone: BlockVariantId,
    pub void: BlockVariantId,
    pub debug: BlockVariantId,
    pub water: BlockVariantId,
}

/// The default world generator
pub struct WorldGenerator {
    palette: WorldGeneratorPalette,
    perlin_noise: Perlin,
}

/// Example worldgen seed for easy debugging.
pub const DEBUG_WORLDGEN_SEED: u32 = 0xff;

impl WorldGenerator {
    /// Create a new world generator initialized from the registries.
    /// The noise function will use [`DEBUG_WORLDGEN_SEED`].
    pub fn new(registries: &Registries) -> Self {
        let registry = registries.get_registry::<BlockVariantRegistry>().unwrap();

        let palette = WorldGeneratorPalette {
            stone: registry.get_id(&rpath("stone")).unwrap(),
            void: registry.get_id(&rpath("void")).unwrap(),
            debug: registry.get_id(&rpath("debug")).unwrap(),
            water: registry.get_id(&rpath("water")).unwrap(),
        };

        Self {
            palette,
            perlin_noise: Perlin::new(DEBUG_WORLDGEN_SEED),
        }
    }
}

impl WorldgenWorker for WorldGenerator {
    fn run<'a>(&mut self, chunk_pos: ChunkPos, cx: WorldgenContext<'a>) {
        let Some(chunk_ref) = cx.loaded_primordial_chunk(chunk_pos) else {
            // If we can't get a chunk reference we return early and just ignore this event.
            return;
        };

        chunk_ref
            .update_flags(LockStrategy::Blocking, |flags| {
                flags.insert(ChunkFlags::GENERATING)
            })
            .unwrap();

        // Make an own scope for the write handle here so the borrowchecker is happy.
        {
            let mut write_handle = chunk_ref
                .chunk()
                .write_handle(LockStrategy::Blocking)
                .unwrap();
            write_handle
                .set(ivec3(0, 0, 0), self.palette.stone)
                .unwrap();
        }

        chunk_ref
            .update_flags(LockStrategy::Blocking, |flags| {
                flags.remove(ChunkFlags::PRIMORDIAL | ChunkFlags::GENERATING);
                flags.insert(
                    ChunkFlags::REMESH
                        | ChunkFlags::REMESH_NEIGHBORS
                        | ChunkFlags::FRESHLY_GENERATED,
                )
            })
            .unwrap();

        cx.notify_done(chunk_pos).unwrap();
    }
}
