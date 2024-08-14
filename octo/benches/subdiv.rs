use criterion::{criterion_group, criterion_main, BatchSize, Bencher, Criterion, SamplingMode};
use octo::subdiv::SubdividedStorage;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::time::Duration;
use std::{array, cell::LazyCell, hint::black_box};

type BenchSubdivStorage = SubdividedStorage<16, 4, u32>;
const BSS_DIMS: u8 = 16 * 4;

fn random_index<R: Rng>(rng: &mut R) -> [u8; 3] {
    black_box([
        rng.gen_range(0..8),
        rng.gen_range(0..8),
        rng.gen_range(0..8),
    ])
}

fn random_index_not_border<R: Rng>(rng: &mut R) -> [u8; 3] {
    let p = [
        rng.gen_range(1..7),
        rng.gen_range(1..7),
        rng.gen_range(1..7),
    ];

    // Sanity check to make sure we're actually not on the border
    p.map(|val| assert!(val > 0 && val < 7));

    black_box(p)
}

fn populated_subdiv_storage<R: Rng>(rng: &mut R) -> BenchSubdivStorage {
    let mut s = BenchSubdivStorage::with_capacity(0, 8 * 8 * 8);

    for x in 0..BSS_DIMS {
        for y in 0..BSS_DIMS {
            for z in 0..BSS_DIMS {
                let value = rng.gen_range(0..(0b1 << (u32::BITS - 1)));
                s.set_mb([x, y, z], value).unwrap();
            }
        }
    }

    black_box(s)
}

fn seeded_rng() -> StdRng {
    let rng_seed = rand::random::<u64>();
    println!("rng_seed={rng_seed:#01x}");
    StdRng::seed_from_u64(rng_seed)
}

fn get_single_mb(s: &BenchSubdivStorage, p: [u8; 3]) -> u32 {
    s.get_mb(p).unwrap()
}

#[allow(clippy::declare_interior_mutable_const)]
const BOX_OFFSETS: LazyCell<[[i8; 3]; 28]> = LazyCell::new(|| {
    let mut offsets = [[0; 3]; 28];
    let mut i = 0;

    for p0 in -1..=1i8 {
        for p1 in -1..=1i8 {
            for p2 in -1..=1i8 {
                offsets[i] = [p0, p1, p2];
                i += 1;
            }
        }
    }

    offsets
});

fn get_3x3x3_mb(storage: &BenchSubdivStorage, p: [u8; 3]) -> [u32; 28] {
    let [p0, p1, p2] = p;

    let mut a = [0; 28];
    let mut i = 0;

    for o0 in -1..=1i8 {
        for o1 in -1..=1i8 {
            for o2 in -1..1i8 {
                let index = [
                    ((p0 as i8) + o0) as u8,
                    ((p1 as i8) + o1) as u8,
                    ((p2 as i8) + o2) as u8,
                ];

                a[i] = storage.get_mb(index).unwrap();
                i += 1;
            }
        }
    }

    a
}

fn get_3x3x3_rev_mb(storage: &BenchSubdivStorage, p: [u8; 3]) -> [u32; 28] {
    let [p0, p1, p2] = p;

    let mut a = [0; 28];
    let mut i = 0;

    for o0 in -1..=1i8 {
        for o1 in -1..=1i8 {
            for o2 in -1..1i8 {
                let index = [
                    ((p0 as i8) + o2) as u8,
                    ((p1 as i8) + o1) as u8,
                    ((p2 as i8) + o0) as u8,
                ];

                a[i] = storage.get_mb(index).unwrap();
                i += 1;
            }
        }
    }

    a
}

fn get_single_mb_routine<R: Rng>(bencher: &mut Bencher, mut rng: &mut R) {
    let storage = populated_subdiv_storage(rng);

    bencher.iter_batched_ref(
        || (storage.clone(), random_index(&mut rng)),
        |(storage, index)| get_single_mb(storage, *index),
        BatchSize::LargeInput,
    );
}

fn get_3x3x3_mb_routine<R: Rng>(bencher: &mut Bencher, mut rng: &mut R) {
    let storage = populated_subdiv_storage(rng);

    bencher.iter_batched_ref(
        || (storage.clone(), random_index_not_border(&mut rng)),
        |(storage, index)| get_3x3x3_mb(storage, *index),
        BatchSize::LargeInput,
    );
}

fn get_3x3x3_rev_mb_routine<R: Rng>(bencher: &mut Bencher, mut rng: &mut R) {
    let storage = populated_subdiv_storage(rng);

    bencher.iter_batched_ref(
        || (storage.clone(), random_index_not_border(&mut rng)),
        |(storage, index)| get_3x3x3_rev_mb(storage, *index),
        BatchSize::LargeInput,
    );
}

fn get_entire_sum_mb(s: &BenchSubdivStorage, indices: &Vec<[u8; 3]>) -> u64 {
    let mut sum = 0u64;

    for &index in indices {
        sum += u64::from(s.get_mb(index).unwrap());
    }

    sum
}

fn all_indices() -> Vec<[u8; 3]> {
    let mut indices = Vec::<[u8; 3]>::with_capacity((BSS_DIMS as usize).pow(3));

    for p0 in 0..BSS_DIMS {
        for p1 in 0..BSS_DIMS {
            for p2 in 0..BSS_DIMS {
                indices.push([p0, p1, p2]);
            }
        }
    }

    black_box(indices)
}

fn all_indices_rev() -> Vec<[u8; 3]> {
    let mut indices = Vec::<[u8; 3]>::with_capacity((BSS_DIMS as usize).pow(3));

    for p0 in 0..BSS_DIMS {
        for p1 in 0..BSS_DIMS {
            for p2 in 0..BSS_DIMS {
                indices.push([p2, p1, p0]);
            }
        }
    }

    black_box(indices)
}

fn get_entire_sum_mb_routine<R: Rng>(bencher: &mut Bencher, mut rng: &mut R) {
    let storage = populated_subdiv_storage(rng);

    bencher.iter_batched_ref(
        || storage.clone(),
        |storage| {
            let mut sum = 0;

            for p0 in 0..BSS_DIMS {
                for p1 in 0..BSS_DIMS {
                    for p2 in 0..BSS_DIMS {
                        sum += storage.get_mb([p0, p1, p2]).unwrap();
                    }
                }
            }

            sum
        },
        BatchSize::LargeInput,
    );
}

fn get_entire_sum_rev_mb_routine<R: Rng>(bencher: &mut Bencher, mut rng: &mut R) {
    let storage = populated_subdiv_storage(rng);

    bencher.iter_batched_ref(
        || storage.clone(),
        |storage| {
            let mut sum = 0;

            for p0 in 0..BSS_DIMS {
                for p1 in 0..BSS_DIMS {
                    for p2 in 0..BSS_DIMS {
                        // The order of these components is reversed. Might be a more cache friendly
                        // way to access the data.
                        sum += storage.get_mb([p2, p1, p0]).unwrap();
                    }
                }
            }

            sum
        },
        BatchSize::LargeInput,
    );
}

fn benchmarks(c: &mut Criterion) {
    let mut rng = seeded_rng();

    let mut group = c.benchmark_group("subdivided_storage");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("get_single_mb", |bencher| {
        get_single_mb_routine(bencher, &mut rng)
    });

    group.bench_function("get_3x3x3_mb", |bencher| {
        get_3x3x3_mb_routine(bencher, &mut rng)
    });

    group.bench_function("get_3x3x3_rev_mb", |bencher| {
        get_3x3x3_rev_mb_routine(bencher, &mut rng)
    });

    group.bench_function("get_entire_sum_mb", |bencher| {
        get_entire_sum_mb_routine(bencher, &mut rng)
    });

    group.bench_function("get_entire_sum_rev_mb", |bencher| {
        get_entire_sum_rev_mb_routine(bencher, &mut rng)
    });
}

criterion_group!(benches, benchmarks);

criterion_main!(benches);
