use bevy::{
    prelude::*,
    tasks::{TaskPool, TaskPoolBuilder},
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
    realm: Res<VoxelRealm>,
) {
    let task_pool = TaskPoolBuilder::new()
        .thread_name("Generator Worker Task Pool".into())
        .build();

    let worker_pool = GeneratorWorkerPool::new(
        seed.0,
        task_pool.thread_num(),
        &task_pool,
        registries.clone(),
        realm.chunk_manager.clone(),
    );

    cmds.insert_resource(worker_pool);
    cmds.insert_resource(GeneratorWorkerTaskPool(task_pool));
}

pub fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk>,
    workers: Res<GeneratorWorkerPool>,
) {
    for event in reader.read() {
        workers.queue_job(event.pos);
    }
}
