use std::array;

use bevy::math::{ivec3, IVec3};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::thread_rng;
use voxel_engine::topo::{chunk::Chunk, storage::data_structures::IndexedChunkStorage};

fn ics_write(c: &mut Criterion) {
    let set_single = |storage: &mut IndexedChunkStorage<u32>, pos: IVec3, data: u32| {
        storage.set(pos, data).unwrap();
    };

    let set_many = |storage: &mut IndexedChunkStorage<u32>, positions: &[IVec3], data: u32| {
        storage.set_many(positions, data).unwrap()
    };

    c.bench_function("set-single", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        b.iter(|| {
            set_single(
                black_box(&mut storage),
                black_box(ivec3(1, 1, 1)),
                black_box(10),
            )
        });
    });

    c.bench_function("set-16-y", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        let positions = std::array::from_fn::<IVec3, 16, _>(|i| ivec3(0, i as _, 0));

        b.iter(|| {
            set_many(
                black_box(&mut storage),
                black_box(&positions),
                black_box(10),
            )
        });
    });
}

fn random_chunk_idx_seq() -> [i32; Chunk::USIZE] {
    use rand::seq::SliceRandom;
    let mut rng = thread_rng();

    let mut ordered = array::from_fn::<i32, { Chunk::USIZE }, _>(|n| n as i32);
    ordered.shuffle(&mut rng);
    ordered
}

fn generate_positions(amount: usize) -> Vec<IVec3> {
    let mut positions = Vec::with_capacity(amount);

    let mut c = 0;
    for x in random_chunk_idx_seq() {
        for y in random_chunk_idx_seq() {
            for z in random_chunk_idx_seq() {
                if c >= amount {
                    return positions;
                }

                positions.push(ivec3(x, y, z));
                c += 1
            }
        }
    }

    panic!();
}

fn ics_optimize(c: &mut Criterion) {
    let optimize = |storage: &mut IndexedChunkStorage<u32>| {
        storage.optimize();
    };

    const UNIQUE_POSITIONS: usize = 15 * 15 * 15;
    const UNIQUE_VALUES: usize = 64;

    let positions = generate_positions(UNIQUE_POSITIONS);

    let mut group = c.benchmark_group("optimize-ics");

    for value_count in 1..=UNIQUE_VALUES {
        group.bench_with_input(
            BenchmarkId::from_parameter(value_count),
            &value_count,
            |b, &value_count| {
                let values = (0..value_count)
                    .map(|_| rand::random::<u32>())
                    .collect::<Vec<_>>();
                let mut storage = IndexedChunkStorage::<u32>::new();

                for (&pos, &val) in positions.iter().zip(values.iter().cycle()) {
                    storage.set(pos, val).unwrap();
                }

                b.iter(|| {
                    let mut bstorage = storage.clone();
                    optimize(black_box(&mut bstorage));
                });
            },
        );
    }
}

criterion_group!(benches, ics_write, ics_optimize);
criterion_main!(benches);
