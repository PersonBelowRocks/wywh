use std::cmp::max;

use bevy::{
    prelude::*,
    tasks::{available_parallelism, TaskPool, TaskPoolBuilder},
};

use crate::{data::registries::Registries, topo::world::VoxelRealm};

use super::{generator::GenerateChunk, GeneratorWorkerPool};

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
        seed.0,
        task_pool.thread_num(),
        &task_pool,
        registries.clone(),
        realm.clone_cm(),
    );

    cmds.insert_resource(worker_pool);
    cmds.insert_resource(GeneratorWorkerTaskPool(task_pool));
}

pub fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk>,
    workers: Res<GeneratorWorkerPool>,
) {
    let mut total = 0;
    for event in reader.read() {
        workers.queue_job(event.pos);
        total += 1;
    }

    if total > 0 {
        debug!("Queued {} generation jobs from events", total);
    }
}
