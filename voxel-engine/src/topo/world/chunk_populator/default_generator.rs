use bevy::{log::error, math::IVec3};
use noise::{NoiseFn, Perlin};

use crate::{
    cartesian_grid,
    data::{
        registries::{
            block::{BlockVariantId, BlockVariantRegistry},
            Registries, Registry,
        },
        resourcepath::rpath,
        tile::Transparency,
    },
    topo::{
        chunkspace_to_mb_worldspace_min,
        world::{
            chunk::{ChunkFlags, ChunkWriteHandle},
            ChunkHandleError, ChunkPos,
        },
        CHUNK_MICROBLOCK_DIMS,
    },
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

    pub fn write_to_chunk<'a>(
        &self,
        chunk_pos: ChunkPos,
        mut chunk: ChunkWriteHandle<'a>,
    ) -> Result<Option<Transparency>, ChunkHandleError> {
        const THRESHOLD: f64 = 0.5;

        let ws_min = chunk_pos.worldspace_min();
        let mb_ws_min = chunkspace_to_mb_worldspace_min(chunk_pos.as_ivec3());

        let mut has_terrain = false;

        // If no corners have any terrain, then we skip the rest of the generation because theres *probably* nothing in this chunk.
        for corner in cartesian_grid!(IVec3::ZERO..=IVec3::ONE) {
            let corner_mb = (corner * CHUNK_MICROBLOCK_DIMS as i32) - 1;
            let mut noise_pos = (mb_ws_min + corner_mb).as_dvec3();
            noise_pos *= 0.002;
            let noise = self.perlin_noise.get(noise_pos.to_array());

            if noise > THRESHOLD {
                has_terrain = true;
                break;
            }
        }

        if has_terrain {
            for mb_ls_pos in cartesian_grid!(IVec3::ZERO..IVec3::splat(CHUNK_MICROBLOCK_DIMS as _))
            {
                let mut noise_pos = (mb_ws_min + mb_ls_pos).as_dvec3();
                noise_pos *= 0.002;
                let noise = self.perlin_noise.get(noise_pos.to_array());

                if noise > THRESHOLD {
                    chunk.set_mb(mb_ls_pos, self.palette.stone)?;
                }
            }

            chunk.deflate(Some(32));
            let all_stone = chunk
                .inner_ref()
                .all_full_blocks_and(|id| id == self.palette.stone);

            if all_stone {
                Ok(Some(Transparency::Opaque))
            } else {
                Ok(None)
            }
        } else {
            Ok(Some(Transparency::Transparent))
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

        let write_handle = chunk_ref
            .chunk()
            .write_handle(LockStrategy::Blocking)
            .unwrap();

        match self.write_to_chunk(chunk_pos, write_handle) {
            Ok(transparency) => {
                // Only do this cleanup stuff if we didn't have any errors.
                chunk_ref
                    .update_flags(LockStrategy::Blocking, |flags| {
                        flags.remove(ChunkFlags::PRIMORDIAL | ChunkFlags::GENERATING);
                        flags.insert(
                            ChunkFlags::REMESH
                                | ChunkFlags::REMESH_NEIGHBORS
                                | ChunkFlags::FRESHLY_GENERATED,
                        );

                        // Hint that a chunk is transparent or opaque.
                        match transparency {
                            Some(Transparency::Opaque) => flags.insert(ChunkFlags::OPAQUE),
                            Some(Transparency::Transparent) => {
                                flags.insert(ChunkFlags::TRANSPARENT)
                            }
                            _ => (),
                        }
                    })
                    .unwrap();

                // The error here is likely because the app is shutting down, in which case there's no handling to be done.
                let _ = cx.notify_done(chunk_pos);
            }
            Err(error) => error!("Error running worldgen for chunk {chunk_pos}: {error}"),
        }
    }
}
