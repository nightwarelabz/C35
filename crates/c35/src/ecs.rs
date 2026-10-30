//! The data-oriented core: a lean ECS plus a small job pool.
//!
//! Design rules, kept deliberately small (this is an engine core, not a
//! framework):
//! - Entities are plain IDs. Components live in **flat typed arenas** —
//!   one `Vec` per component type, indexed by entity ID. No hash maps in
//!   the scene graph, and the update path allocates nothing: arenas are
//!   sized at scene build and only overwritten afterwards.
//! - Systems are plain functions over the world, run in registration
//!   order, named so the ledger can say what ran.
//! - CPU-parallel work goes through [`JobPool`]: a channel-fed pool of
//!   std threads. It exists to run *batches* (visibility chunks today,
//!   anything tile-shaped tomorrow), not to pretend a 2-core VM is a
//!   workstation — the ledger prints the pool's size and job count, and
//!   the milestone report says plainly what parallelism did or didn't
//!   buy on the bench it ran on. Where the target has no threads at all
//!   (wasm32), the pool degrades to size 0 and batches run inline on the
//!   caller — the M3 web proof convicted the first cut for assuming
//!   threads; the core must never require them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::{
    any::Any,
    cell::Cell,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
};

thread_local! {
    // A nested batch must run inline: waiting for the same pool from
    // all of its workers would otherwise starve the queue.
    static IN_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// An entity: just an ID. Components are looked up by it in arenas.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Entity(pub u32);

/// A flat typed arena: one slot per entity ID, `None` where the entity
/// lacks the component. Iteration walks the vector in entity order —
/// deterministic on every target, which the proof runs rely on.
pub struct Arena<T> {
    slots: Vec<Option<T>>,
    live: usize,
}

impl<T> Arena<T> {
    pub fn new() -> Arena<T> {
        Arena {
            slots: Vec::new(),
            live: 0,
        }
    }

    pub fn insert(&mut self, e: Entity, value: T) {
        let i = e.0 as usize;
        if i >= self.slots.len() {
            self.slots.resize_with(i + 1, || None);
        }
        if self.slots[i].replace(value).is_none() {
            self.live += 1;
        }
    }

    pub fn get(&self, e: Entity) -> Option<&T> {
        self.slots.get(e.0 as usize).and_then(|s| s.as_ref())
    }

    pub fn get_mut(&mut self, e: Entity) -> Option<&mut T> {
        self.slots.get_mut(e.0 as usize).and_then(|s| s.as_mut())
    }

    /// Live components, in entity order.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|v| (Entity(i as u32), v)))
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn remove(&mut self, e: Entity) -> Option<T> {
        let value = self.slots.get_mut(e.0 as usize)?.take()?;
        self.live -= 1;
        Some(value)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_mut().map(|v| (Entity(i as u32), v)))
    }

    /// Remove all components while retaining arena capacity for reuse.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.live = 0;
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Job pool
// ---------------------------------------------------------------------------

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Latch {
    count: Mutex<usize>,
    done: Condvar,
}

impl Latch {
    fn new(n: usize) -> Latch {
        Latch {
            count: Mutex::new(n),
            done: Condvar::new(),
        }
    }
    fn count_down(&self) {
        let mut c = self.count.lock().unwrap();
        *c -= 1;
        if *c == 0 {
            self.done.notify_all();
        }
    }
    fn wait(&self) {
        let mut c = self.count.lock().unwrap();
        while *c > 0 {
            c = self.done.wait(c).unwrap();
        }
    }
}

/// A channel-fed pool of worker threads. Jobs are fire-and-forget
/// individually; the batch APIs block until the batch drains, which is
/// what makes shared-slice chunk work sound.
pub struct JobPool {
    tx: Sender<Job>,
    workers: usize,
    executed: Arc<AtomicU64>,
}

impl JobPool {
    /// Pool sized to the machine (available parallelism, clamped 1..=8).
    pub fn new() -> JobPool {
        let n = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(1)
            .clamp(1, 8);
        Self::with_workers(n)
    }

    pub fn with_workers(n: usize) -> JobPool {
        // Explicit zero-worker mode also makes headless deterministic
        // tests and single-threaded hosts independent of thread support.
        #[cfg(target_arch = "wasm32")]
        let n = {
            let _ = n;
            0
        };
        let (tx, rx) = channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let executed = Arc::new(AtomicU64::new(0));
        let mut spawned = 0usize;
        for i in 0..n {
            let rx = rx.clone();
            let executed = executed.clone();
            // A failed spawn is not fatal — most importantly on
            // wasm32-unknown-unknown, where std threads do not exist at
            // all: the pool then runs in inline mode (size 0), and every
            // batch executes on the caller's thread. The scene must be
            // buildable and updatable with no threads; parallelism is an
            // accelerator, never a requirement.
            let ok = std::thread::Builder::new()
                .name(format!("comigen-job-{i}"))
                .spawn(move || loop {
                    // The receiver is shared: workers contend on the lock
                    // only to dequeue, never while running a job.
                    let job = {
                        let lock = rx.lock().unwrap();
                        lock.recv()
                    };
                    match job {
                        Ok(job) => {
                            executed.fetch_add(1, Ordering::Relaxed);
                            IN_WORKER.with(|flag| flag.set(true));
                            job();
                            IN_WORKER.with(|flag| flag.set(false));
                        }
                        Err(_) => return, // pool dropped: shut down
                    }
                })
                .is_ok();
            if ok {
                spawned += 1;
            }
        }
        JobPool {
            tx,
            workers: spawned,
            executed,
        }
    }

    pub fn size(&self) -> usize {
        self.workers
    }

    /// Total jobs executed since the pool was built.
    pub fn executed(&self) -> u64 {
        self.executed.load(Ordering::Relaxed)
    }

    /// Run `f` over disjoint chunks of `buf` in parallel and block until
    /// every chunk is done. `f` is called once per chunk with the
    /// chunk's start index and its exclusive sub-slice; chunk count is
    /// clamped to the pool size and the buffer length.
    ///
    /// Soundness: the chunks come from splitting one slice into disjoint
    /// ranges by construction, `f` is `Sync`, and this call does not
    /// return until the latch drains — so no chunk reference outlives
    /// the call. (Pointers cross the channel as integers because `Job`
    /// is `'static`; the latch is what makes that honest.)
    pub fn for_each_chunk_mut<T, F>(&self, buf: &mut [T], chunks: usize, f: &F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Send + Sync,
    {
        let len = buf.len();
        if len == 0 {
            return;
        }
        // Pool size 0 = inline mode (no worker threads exist on this
        // target): the whole buffer is one chunk, run on the caller.
        let chunks = chunks.clamp(1, self.workers.max(1)).min(len);
        if chunks == 1 || IN_WORKER.with(Cell::get) {
            self.executed.fetch_add(1, Ordering::Relaxed);
            f(0, buf);
            return;
        }
        let base = buf.as_mut_ptr() as usize;
        let f_addr = f as *const F as usize;
        let latch = Arc::new(Latch::new(chunks));
        let panic: Arc<Mutex<Option<Box<dyn Any + Send>>>> = Arc::new(Mutex::new(None));
        let chunk_len = len.div_ceil(chunks);
        for c in 0..chunks {
            let start = c * chunk_len;
            if start >= len {
                latch.count_down();
                continue;
            }
            let n = chunk_len.min(len - start);
            let latch = latch.clone();
            let panic = panic.clone();
            let job: Job = Box::new(move || {
                // SAFETY: disjoint range of the caller's slice (see the
                // method docs); the caller blocks on the latch, so the
                // slice and `f` outlive every job.
                let slice =
                    unsafe { std::slice::from_raw_parts_mut((base as *mut T).add(start), n) };
                let f_ref = unsafe { &*(f_addr as *const F) };
                // Drain EVERY borrowed chunk before propagating a panic
                // on the caller. Workers survive and the scoped pointers
                // never outlive the borrowed buffer or closure.
                if let Err(payload) = catch_unwind(AssertUnwindSafe(|| f_ref(start, slice))) {
                    let mut first = panic.lock().unwrap();
                    if first.is_none() {
                        *first = Some(payload);
                    }
                }
                latch.count_down();
            });
            // A failed send means every worker is gone (impossible while
            // &self is alive — the pool owns the only sender — but stay
            // honest): recover the job and run it inline; it counts the
            // latch down itself.
            if let Err(e) = self.tx.send(job) {
                e.0();
            }
        }
        latch.wait();
        let payload = panic.lock().unwrap().take();
        if let Some(payload) = payload {
            resume_unwind(payload);
        }
    }
}

impl Default for JobPool {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Systems + core stats
// ---------------------------------------------------------------------------

/// A named system: a plain function over the scene world. Systems run in
/// registration order on the update thread; a system fans its own batch
/// work out to the job pool when it has any.
pub struct SystemDef<W> {
    pub name: &'static str,
    pub run: fn(&mut W, &JobPool),
}

/// What the ledger prints about the core after an update: how much scene
/// there is, what ran, what the pool did, and what it cost on the CPU.
/// (GPU costs live in the renderer's own ledger lines.)
#[derive(Copy, Clone, Default, Debug)]
pub struct CoreStats {
    pub entities: usize,
    pub systems: usize,
    pub jobs_executed: u64,
    pub pool_size: usize,
    pub update_ms: f64,
}
