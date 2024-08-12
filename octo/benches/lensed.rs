use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use octo::lensed::{LensedStorage, STORAGE_DIMS};
use rand::Rng;

fn random_storage_index<R: Rng>(rng: &mut R) -> [u8; 3] {
    [
        rng.next_u32().rem_euclid(STORAGE_DIMS as u32) as u8,
        rng.next_u32().rem_euclid(STORAGE_DIMS as u32) as u8,
        rng.next_u32().rem_euclid(STORAGE_DIMS as u32) as u8,
    ]
}

fn random_lensed_storage<R: Rng>(rng: &mut R) -> LensedStorage<u64> {
    let mut s = LensedStorage::with_capacity(0, 8 * 8 * 8);
    let mut n = 4;

    for x in 0..STORAGE_DIMS {
        for y in 0..STORAGE_DIMS {
            for z in 0..STORAGE_DIMS {
                s.set([x, y, z], n);
                n *= 3;
            }
        }
    }

    s
}

fn lensed_storage(c: &mut Criterion) {
    let mut group = c.benchmark_group("lensed_storage ");

    group.bench_function(" random_single_read", |b| {
        let mut rng = rand::thread_rng();

        // FIXME: this benchmark behaves really strangely, fix it
        b.iter_batched_ref(
            || {
                (
                    black_box(random_lensed_storage(&mut rng)),
                    random_storage_index(&mut rng),
                )
            },
            |(storage, index)| {
                black_box(storage.get(*index));
            },
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, lensed_storage);

criterion_main!(benches);
