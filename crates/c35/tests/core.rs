use c35::ecs::{Arena, Entity, JobPool};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn arena_lifecycle_and_order() {
    let mut arena = Arena::new();
    arena.insert(Entity(8), 1);
    arena.insert(Entity(2), 2);
    arena.insert(Entity(8), 3);
    assert_eq!(arena.len(), 2);
    assert_eq!(arena.remove(Entity(8)), Some(3));
    assert_eq!(arena.remove(Entity(8)), None);
    for (_, value) in arena.iter_mut() {
        *value += 10;
    }
    assert_eq!(
        arena.iter().map(|(e, v)| (e.0, *v)).collect::<Vec<_>>(),
        vec![(2, 12)]
    );
    arena.clear();
    assert!(arena.is_empty());
    arena.insert(Entity(8), 4);
    assert_eq!(arena.len(), 1);
}

#[test]
fn chunks_cover_each_element_once() {
    for workers in [0, 1, 2, 4] {
        let pool = JobPool::with_workers(workers);
        for len in 0..33 {
            let mut values = vec![0usize; len];
            pool.for_each_chunk_mut(&mut values, 4, &|start, chunk| {
                for (i, value) in chunk.iter_mut().enumerate() {
                    *value += start + i + 1;
                }
            });
            assert_eq!(values, (1..=len).collect::<Vec<_>>());
        }
    }
}

#[test]
fn worker_panic_drains_borrowed_jobs_and_pool_survives() {
    let pool = JobPool::with_workers(4);
    let mut values = [0; 16];
    let panic = catch_unwind(AssertUnwindSafe(|| {
        pool.for_each_chunk_mut(&mut values, 4, &|start, chunk| {
            if start == 0 {
                panic!("intentional batch failure");
            }
            chunk.fill(1);
        });
    }));
    assert!(panic.is_err());
    assert!(values[4..].iter().all(|v| *v == 1));
    pool.for_each_chunk_mut(&mut values, 4, &|_, chunk| chunk.fill(2));
    assert_eq!(values, [2; 16]);
}

#[test]
fn nested_batches_do_not_starve_workers() {
    let pool = JobPool::with_workers(2);
    let mut values = [0; 8];
    pool.for_each_chunk_mut(&mut values, 2, &|_, chunk| {
        pool.for_each_chunk_mut(chunk, 2, &|_, inner| inner.fill(7));
    });
    assert_eq!(values, [7; 8]);
}
