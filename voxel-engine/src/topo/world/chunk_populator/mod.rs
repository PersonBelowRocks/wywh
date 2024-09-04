pub mod error;
pub mod events;
pub mod worldgen;

use std::{future::Future, sync::Arc};

use async_lock::Mutex;
use bevy::{prelude::*, tasks::TaskPool};
use events::PopulateChunkEvent;
use priority_queue::PriorityQueue;
use worldgen::{GenerateChunk, GenerateChunkActor};
use xtra::Address;

use super::{ChunkManager, ChunkPos};

pub struct ChunkPopulatorController;

impl Plugin for ChunkPopulatorController {
    fn build(&self, app: &mut App) {
        todo!()
    }
}

#[derive(Resource, Deref)]
pub struct PopulationQueue(Arc<Mutex<PriorityQueue<ChunkPos, u32, rustc_hash::FxBuildHasher>>>);

// TODO: docs
#[derive(xtra::Actor)]
pub struct PopulateEventBusActor {
    chunk_manager: Arc<ChunkManager>,
    generator_task_pool: Arc<TaskPool>,
    generator_actor_address: Option<Address<GenerateChunkActor>>,
}

impl PopulateEventBusActor {
    // TODO: docs
    pub fn cached_generator_actor_address(&mut self) -> Address<GenerateChunkActor> {
        self.generator_actor_address
            .get_or_insert_with(|| {
                let (address, mailbox) = xtra::Mailbox::<GenerateChunkActor>::unbounded();

                for id in 0..self.generator_task_pool.thread_num() {
                    let actor = GenerateChunkActor {
                        id,
                        chunk_manager: self.chunk_manager.clone(),
                    };

                    self.generator_task_pool
                        .spawn(xtra::run(mailbox.clone(), actor))
                        .detach();
                }

                address
            })
            .clone()
    }
}

// TODO: docs
#[derive(Copy, Clone, Debug)]
pub struct PopulateChunk {
    pub priority: u32,
    pub chunk_pos: ChunkPos,
}

// TODO: docs
#[derive(Copy, Clone)]
pub enum ChunkPopulationStatus {
    Generating,
    LoadingFromDisk,
}

impl xtra::Handler<PopulateChunk> for PopulateEventBusActor {
    type Return = ChunkPopulationStatus;

    async fn handle(
        &mut self,
        message: PopulateChunk,
        ctx: &mut xtra::Context<Self>,
    ) -> Self::Return {
        let address = self.cached_generator_actor_address();
        // TODO: error handling
        address
            .send(GenerateChunk(message.chunk_pos))
            .priority(message.priority)
            .detach()
            .await;

        ChunkPopulationStatus::Generating
    }
}
