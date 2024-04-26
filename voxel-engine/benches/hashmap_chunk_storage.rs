use bevy::math::ivec3;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use voxel_engine::topo::{storage::data_structures::HashmapChunkStorage, world::Chunk};

// TODO: dont duplicate so much code
fn prefill_storage(storage: &mut HashmapChunkStorage<u32>) {
    for x in 0..Chunk::SIZE {
        for y in 0..Chunk::SIZE {
            for z in 0..Chunk::SIZE {
                let pos = ivec3(x, y, z);

                storage.set(pos, rand::random::<u32>()).unwrap();
            }
        }
    }
}

fn hashmap_chunk_storage_write(c: &mut Criterion) {
    let mut storage = HashmapChunkStorage::<u32>::new();

    c.bench_function("fill-storage", |bencher| {
        bencher.iter(|| {
            for x in 0..Chunk::SIZE {
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        let pos = ivec3(x, y, z);

                        storage.set(black_box(pos), black_box(10)).unwrap();
                    }
                }
            }
        });
    });

    c.bench_function("write-single-value", |bencher| {
        bencher.iter(|| {
            storage
                .set(black_box(ivec3(10, 10, 10)), black_box(10))
                .unwrap();
        });
    });
}

fn hashmap_chunk_storage_read(c: &mut Criterion) {
    let mut storage = HashmapChunkStorage::<u32>::new();
    prefill_storage(&mut storage);

    c.bench_function("read-entire", |bencher| {
        bencher.iter(|| {
            for x in 0..Chunk::SIZE {
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        let pos = ivec3(x, y, z);

                        storage.get(black_box(pos)).unwrap();
                    }
                }
            }
        });
    });

    c.bench_function("read-single-value", |bencher| {
        bencher.iter(|| {
            storage.get(black_box(ivec3(10, 10, 10))).unwrap();
        });
    });

    let storage = HashmapChunkStorage::<u32>::new();

    c.bench_function("read-entire-empty", |bencher| {
        bencher.iter(|| {
            for x in 0..Chunk::SIZE {
                for y in 0..Chunk::SIZE {
                    for z in 0..Chunk::SIZE {
                        let pos = ivec3(x, y, z);

                        storage.get(black_box(pos));
                    }
                }
            }
        });
    });

    c.bench_function("read-single-value-empty", |bencher| {
        bencher.iter(|| {
            storage.get(black_box(ivec3(10, 10, 10)));
        });
    });
}

criterion_group!(
    benches,
    hashmap_chunk_storage_write,
    hashmap_chunk_storage_read
);
criterion_main!(benches);
