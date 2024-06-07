use std::cmp::max;

use bevy::{
    prelude::*,
    tasks::{available_parallelism, TaskPool, TaskPoolBuilder},
};

use crate::{
    data::registries::Registries,
    topo::{world::VoxelRealm, worldgen::GeneratorPoolSettings},
    util::ChunkMap,
};

use super::{generator::GenerateChunk, GeneratorCommand, GeneratorWorkerPool};

#[derive(Resource, Deref, Clone)]
pub struct GeneratorSeed(pub u32);

#[derive(Resource, Deref)]
pub struct GeneratorWorkerTaskPool(TaskPool);

pub fn setup_terrain_generator_workers(
    mut cmds: Commands,
    seed: Res<GeneratorSeed>,
    registries: Res<Registries>,
    realm: VoxelRealm,
) {
    info!("Setting up terrain generator workers");

    let task_pool = TaskPoolBuilder::new()
        .thread_name("Generator Worker Task Pool".into())
        .num_threads(max(1, available_parallelism() / 2))
        .build();

    let worker_pool = GeneratorWorkerPool::new(
        GeneratorPoolSettings {
            workers: task_pool.thread_num(),
            job_channel_capacity: task_pool.thread_num() * 4,
        },
        seed.0,
        &task_pool,
        registries.clone(),
        realm.clone_cm(),
    );

    cmds.insert_resource(worker_pool);
    cmds.insert_resource(GeneratorWorkerTaskPool(task_pool));
}

pub fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk>,
    mut workers: ResMut<GeneratorWorkerPool>,
) {
    // TODO: generator commands should be sorted by distance to closest observer like what
    // the mesh controller does when queuing mesh building jobs.

    let mut commands = ChunkMap::<GeneratorCommand>::new();

    for event in reader.read() {
        let cmd = GeneratorCommand {
            pos: event.pos,
            priority: event.priority,
        };

        commands
            .entry(event.pos)
            .and_modify(|c| c.priority = max(c.priority, cmd.priority))
            .or_insert(cmd);
    }

    let total = commands.len();

    workers.queue_jobs(commands.into_iter().map(|(_, c)| c));

    if total > 0 {
        debug!("Queued {} generation jobs from events", total);
    }
}
