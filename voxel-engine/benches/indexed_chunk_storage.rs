use std::{array, time::Duration};

use bevy::math::{ivec3, IVec3};
use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use rand::{thread_rng, Rng, RngCore};
use voxel_engine::topo::{chunk::Chunk, storage::data_structures::IndexedChunkStorage};

fn random_chunk_pos<R: Rng>(rng: &mut R) -> IVec3 {
    ivec3(
        (rng.next_u32() as i32).rem_euclid(Chunk::SIZE),
        (rng.next_u32() as i32).rem_euclid(Chunk::SIZE),
        (rng.next_u32() as i32).rem_euclid(Chunk::SIZE),
    )
}

fn ics_read_write(c: &mut Criterion) {
    let set_single = |storage: &mut IndexedChunkStorage<u32>, pos: IVec3, data: u32| {
        storage.set(pos, data).unwrap();
    };

    let get_single = |storage: &mut IndexedChunkStorage<u32>, pos: IVec3| -> Option<u32> {
        storage.get(pos).unwrap().copied()
    };

    let mut group = c.benchmark_group("ics-writes");
    // FIXME: performance for random value writes seems to improve with higher sample size... ugh i hate benchmarking
    group
        .sample_size(1000)
        .measurement_time(Duration::from_secs(10));
    group.bench_function("ics-write-random-value", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        let mut rng = thread_rng();

        b.iter_batched(
            || rng.next_u32(),
            |val| {
                set_single(
                    black_box(&mut storage),
                    black_box(ivec3(1, 1, 1)),
                    black_box(val),
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("ics-write-random-pos-and-value", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        let mut rng = thread_rng();

        b.iter_batched(
            || (random_chunk_pos(&mut rng), rng.next_u32()),
            |(pos, val)| set_single(black_box(&mut storage), black_box(pos), black_box(val)),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("ics-write-same", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        b.iter(|| {
            set_single(
                black_box(&mut storage),
                black_box(ivec3(1, 1, 1)),
                black_box(10),
            )
        });
    });

    group.bench_function("ics-write-random-pos", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        let mut rng = thread_rng();

        b.iter_batched(
            || random_chunk_pos(&mut rng),
            |pos: IVec3| set_single(black_box(&mut storage), black_box(pos), black_box(10)),
            BatchSize::SmallInput,
        );
    });

    group.finish();

    c.bench_function("ics-read", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        storage.set(ivec3(1, 1, 1), 10).unwrap();
        b.iter(|| get_single(black_box(&mut storage), black_box(ivec3(1, 1, 1))).unwrap());
    });

    c.bench_function("ics-read-empty", |b| {
        let mut storage = IndexedChunkStorage::<u32>::new();
        b.iter(|| get_single(black_box(&mut storage), black_box(ivec3(1, 1, 1))));
    });
}

fn random_chunk_idx_seq<R: Rng>(mut rng: &mut R) -> [i32; Chunk::USIZE] {
    use rand::seq::SliceRandom;

    let mut ordered = array::from_fn::<i32, { Chunk::USIZE }, _>(|n| n as i32);
    ordered.shuffle(&mut rng);
    ordered
}

fn generate_positions<R: Rng>(rng: &mut R, amount: usize) -> Vec<IVec3> {
    let mut positions = Vec::with_capacity(amount);

    let mut c = 0;
    for x in random_chunk_idx_seq(rng) {
        for y in random_chunk_idx_seq(rng) {
            for z in random_chunk_idx_seq(rng) {
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

fn generate_suboptimal_ics<R: Rng>(
    rng: &mut R,
    positions: usize,
    values_to_remove: usize,
    total_values: usize,
) -> IndexedChunkStorage<u32> {
    use rand::seq::SliceRandom;

    let positions = generate_positions(rng, positions);
    let mut ics = IndexedChunkStorage::<u32>::new();

    let values = (0..total_values).map(|v| v as u32).collect::<Vec<_>>();

    for (&pos, &value) in positions.iter().zip(values.iter().cycle()) {
        ics.set(pos, value).unwrap();
    }

    let remove = values
        .choose_multiple(rng, values_to_remove)
        .copied()
        .collect::<hashbrown::HashSet<_>>();
    for x in 0..Chunk::SIZE {
        for y in 0..Chunk::SIZE {
            for z in 0..Chunk::SIZE {
                let pos = ivec3(x, y, z);

                if let Some(v) = ics.get(pos).unwrap() {
                    if remove.contains(v) {
                        ics.clear(pos).unwrap();
                    }
                }
            }
        }
    }

    ics
}

fn ics_optimize(c: &mut Criterion) {
    let optimize = |storage: &mut IndexedChunkStorage<u32>| {
        storage.optimize();
    };

    const UNIQUE_POSITIONS: usize = 15 * 15 * 15;
    const UNIQUE_VALUES: usize = 16;
    const PERCENT_TO_GC: f32 = 0.25;

    let mut group = c.benchmark_group("optimize-ics");
    let mut rng = thread_rng();

    for value_count in 1..=UNIQUE_VALUES {
        group.bench_with_input(
            BenchmarkId::from_parameter(value_count),
            &value_count,
            |b, &value_count| {
                let values_to_gc = { (value_count as f32 * PERCENT_TO_GC) as usize };

                let storage =
                    generate_suboptimal_ics(&mut rng, UNIQUE_POSITIONS, values_to_gc, value_count);

                b.iter_batched(
                    || storage.clone(),
                    |mut bstorage| {
                        optimize(black_box(&mut bstorage));
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }
}

criterion_group!(benches, ics_read_write, ics_optimize);
criterion_main!(benches);
