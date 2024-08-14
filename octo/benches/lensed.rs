use std::{
    any::type_name,
    array,
    cell::{LazyCell, OnceCell},
    hint::black_box,
    time::Duration,
};

use criterion::{criterion_group, criterion_main, BatchSize, Bencher, Criterion, SamplingMode};
use octo::lensed::{LensedStorage, STORAGE_DIMS};
use rand::{
    distributions::Standard, prelude::Distribution, rngs::StdRng, Rng, RngCore, SeedableRng,
};

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

fn populated_lensed_storage<R: Rng, T: Copy + Default>(rng: &mut R) -> LensedStorage<T>
where
    Standard: Distribution<T>,
{
    let mut s = LensedStorage::with_capacity(T::default(), 8 * 8 * 8);

    for x in 0..STORAGE_DIMS {
        for y in 0..STORAGE_DIMS {
            for z in 0..STORAGE_DIMS {
                let value = rng.gen();
                s.set([x, y, z], value);
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

fn get_single<T: Copy>(s: &LensedStorage<T>, p: [u8; 3]) -> T {
    *s.get(p)
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

fn get_3x3x3<T: Copy>(s: &LensedStorage<T>, p: [u8; 3]) -> [T; 28] {
    array::from_fn(|i| {
        let offset_p = [
            ((p[0] as i8) + BOX_OFFSETS[i][0]) as u8,
            ((p[1] as i8) + BOX_OFFSETS[i][1]) as u8,
            ((p[2] as i8) + BOX_OFFSETS[i][2]) as u8,
        ];

        *s.get(offset_p)
    })
}

fn get_single_for_integer<R: Rng, T: Copy + Default>(bencher: &mut Bencher, mut rng: &mut R)
where
    Standard: Distribution<T>,
{
    let storage = populated_lensed_storage::<_, T>(rng);

    bencher.iter_batched_ref(
        || (storage.clone(), random_index(&mut rng)),
        |(storage, index)| get_single(storage, *index),
        BatchSize::LargeInput,
    );
}

fn get_3x3x3_for_integer<R: Rng, T: Copy + Default>(bencher: &mut Bencher, mut rng: &mut R)
where
    Standard: Distribution<T>,
{
    let storage = populated_lensed_storage::<_, T>(rng);

    bencher.iter_batched_ref(
        || (storage.clone(), random_index_not_border(&mut rng)),
        |(storage, index)| get_3x3x3(storage, *index),
        BatchSize::LargeInput,
    );
}

fn get_entire_sum<T: Copy>(s: &LensedStorage<T>, indices: &Vec<[u8; 3]>) -> u128
where
    u128: From<T>,
{
    let mut sum = 0u128;

    for &index in indices {
        sum += u128::from(*s.get(index));
    }

    sum
}

fn all_indices() -> Vec<[u8; 3]> {
    let mut indices = Vec::<[u8; 3]>::with_capacity((STORAGE_DIMS as usize).pow(3));

    for p0 in 0..STORAGE_DIMS {
        for p1 in 0..STORAGE_DIMS {
            for p2 in 0..STORAGE_DIMS {
                indices.push([p0, p1, p2]);
            }
        }
    }

    black_box(indices)
}

fn get_entire_sum_for_integer<R: Rng, T: Copy + Default>(bencher: &mut Bencher, mut rng: &mut R)
where
    Standard: Distribution<T>,
    u128: From<T>,
{
    let storage = populated_lensed_storage::<_, T>(rng);

    bencher.iter_batched_ref(
        || (storage.clone(), all_indices()),
        |(storage, indices)| get_entire_sum(storage, indices),
        BatchSize::LargeInput,
    );
}

macro_rules! for_int_width {
    ($width:expr, $func:ident, $bencher:expr, $rng:expr) => {
        match $width {
            8 => $func::<_, u8>(($bencher), &mut $rng),
            16 => $func::<_, u16>(($bencher), &mut $rng),
            32 => $func::<_, u32>(($bencher), &mut $rng),
            64 => $func::<_, u64>(($bencher), &mut $rng),
            128 => $func::<_, u128>(($bencher), &mut $rng),
            _ => panic!("Invalid integer width: {}", $width),
        }
    };
}

macro_rules! for_int_width_lt_128 {
    ($width:expr, $func:ident, $bencher:expr, $rng:expr) => {
        match $width {
            8 => $func::<_, u8>(($bencher), &mut $rng),
            16 => $func::<_, u16>(($bencher), &mut $rng),
            32 => $func::<_, u32>(($bencher), &mut $rng),
            64 => $func::<_, u64>(($bencher), &mut $rng),
            _ => panic!("Invalid integer width: {}", $width),
        }
    };
}

fn lensed_storage_get_single(c: &mut Criterion) {
    let mut rng = seeded_rng();

    let mut group = c.benchmark_group("lensed_storage<get_single>");
    group.sampling_mode(SamplingMode::Flat);

    for width in [8, 16, 32, 64, 128u32] {
        let name = format!("width={width}");
        group.bench_with_input(name, &width, |bencher, &width| {
            for_int_width!(width, get_single_for_integer, bencher, rng);
        });
    }
}

fn lensed_storage_get_3x3x3(c: &mut Criterion) {
    // Force the initialization of our offsets early so it doesn't contaminate our benchmarks
    LazyCell::force(&BOX_OFFSETS);
    let mut rng = seeded_rng();

    let mut group = c.benchmark_group("lensed_storage<get_3x3x3>");
    group.sampling_mode(SamplingMode::Flat);

    for width in [8, 16, 32, 64, 128u32] {
        let name = format!("width={width}");
        group.bench_with_input(name, &width, |bencher, &width| {
            for_int_width!(width, get_3x3x3_for_integer, bencher, rng);
        });
    }
}

fn lensed_storage_get_entire(c: &mut Criterion) {
    let mut rng = seeded_rng();

    let mut group = c.benchmark_group("lensed_storage<get_entire_sum>");
    group.sampling_mode(SamplingMode::Flat);

    for width in [8, 16, 32, 64u32] {
        let name = format!("width={width}");
        group.bench_with_input(name, &width, |bencher, &width| {
            for_int_width_lt_128!(width, get_entire_sum_for_integer, bencher, rng);
        });
    }
}

criterion_group!(
    benches,
    lensed_storage_get_single,
    lensed_storage_get_3x3x3,
    lensed_storage_get_entire
);

criterion_main!(benches);
