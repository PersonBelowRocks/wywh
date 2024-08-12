use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use glam::uvec3;
use octo::octree::{MaxDepth, NPos, Octree, X1, X2, X3, X4, X5, X6};
use rand::Rng;

/// Create a "fluffy" octree. An octree can be considered "fluffy" if it has
/// lots of very deep leaf nodes with very little variety (maybe just 2 different values).
fn fluffy_octree<D: MaxDepth>() -> Octree<D, u64> {
    let mut octree = Octree::<D, _>::new(0);

    for x in (0..D::DIMENSIONS).step_by(2) {
        for y in (0..D::DIMENSIONS).step_by(2) {
            for z in (0..D::DIMENSIONS).step_by(2) {
                octree.insert(NPos::new(D::DEPTH, uvec3(x, y, z)), 1);
            }
        }
    }

    octree
}

/// Random node position within the max depth level provided.
fn random_node_pos<D: MaxDepth, R: Rng>(rng: &mut R) -> NPos {
    NPos::new(
        D::DEPTH,
        uvec3(
            rng.next_u32().rem_euclid(D::DIMENSIONS),
            rng.next_u32().rem_euclid(D::DIMENSIONS),
            rng.next_u32().rem_euclid(D::DIMENSIONS),
        ),
    )
}

/// Read and assert that a value is 0 or 1.
fn octree_read_0_or_1<D: MaxDepth>(octree: &Octree<D, u64>, npos: NPos) {
    let out = *black_box(octree).get(black_box(npos));
    // assert!(out == 0 || out == 1);
}

fn octree_read_full_cartesian<D: MaxDepth>(octree: &Octree<D, u64>) {
    for x in 0..D::DIMENSIONS {
        for y in 0..D::DIMENSIONS {
            for z in 0..D::DIMENSIONS {
                let npos = NPos::new(D::DEPTH, uvec3(x, y, z));

                let out = *octree.get(npos);
                assert!(out == 0 || out == 1);
            }
        }
    }
}

/// Fluffy octree benchmarking
fn benchmark_fluffy_octree<D: MaxDepth>(c: &mut Criterion) {
    let group_name = format!("fluffy_octree MAX_DEPTH={} ", D::DEPTH);
    let mut group = c.benchmark_group(group_name);

    group.bench_function(" single_read", |b| {
        let mut rng = rand::thread_rng();

        b.iter_batched_ref(
            || (fluffy_octree::<D>(), random_node_pos::<D, _>(&mut rng)),
            |(octree, npos)| octree_read_0_or_1(octree, *npos),
            BatchSize::LargeInput,
        );
    });

    group.bench_function(" full_octree_read", |b| {
        b.iter_batched_ref(
            || fluffy_octree::<D>(),
            |octree| octree_read_full_cartesian(black_box(octree)),
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(
    benches,
    benchmark_fluffy_octree::<X1>,
    benchmark_fluffy_octree::<X2>,
    benchmark_fluffy_octree::<X3>,
    benchmark_fluffy_octree::<X4>,
    benchmark_fluffy_octree::<X5>,
    benchmark_fluffy_octree::<X6>,
);

criterion_main!(benches);
