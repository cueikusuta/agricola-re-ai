pub mod api;
pub mod benchmark;
pub mod breeding;
pub mod card_pool;
pub mod card_zones;
pub mod card_events;
pub mod cards;
pub mod dataset_split;
pub mod decision_budget;
pub mod draft;
pub mod draft_ai;
pub mod draft_game;
pub mod engine;
pub mod episode_data;
pub mod evaluation;
pub mod harvest;
pub mod hints;
pub mod mcts;
pub mod mcts_puct;
pub mod model;
pub mod pending;
pub mod policy;
pub mod registry;
pub mod score;
pub mod validation;
pub mod verification;
pub mod web;

#[cfg(feature = "perf-count-alloc")]
pub mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Counts heap allocations/bytes. Only used in the dedicated allocation
    /// run; timing runs use the plain build and the `System` allocator.
    ///
    /// The counters are thread-local: `cargo test` runs test threads in
    /// parallel, and a process-global counter would make any measured window
    /// depend on which unrelated tests happen to allocate concurrently. The
    /// counting allocator itself stays process-wide (one `#[global_allocator]`),
    /// but each thread tallies only its own allocations, so a test that
    /// measures `allocation_count()` before/after a call measures that call
    /// alone regardless of parallel siblings.
    struct CountingAllocator;

    thread_local! {
        static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
        static ALLOCATED_BYTES: Cell<u64> = const { Cell::new(0) };
    }

    // Process-wide totals for `perf_baseline`, which is single-threaded per
    // measurement but reads the counters from the main thread.
    static TOTAL_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
    static TOTAL_ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATIONS.with(|c| c.set(c.get() + 1));
            ALLOCATED_BYTES.with(|c| c.set(c.get() + layout.size() as u64));
            TOTAL_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            TOTAL_ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            ALLOCATIONS.with(|c| c.set(c.get() + 1));
            ALLOCATED_BYTES.with(|c| c.set(c.get() + new_size as u64));
            TOTAL_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            TOTAL_ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    /// Allocations made by the calling thread since thread start.
    pub fn allocation_count() -> u64 {
        ALLOCATIONS.with(|c| c.get())
    }
    /// Bytes allocated by the calling thread since thread start.
    pub fn allocated_byte_count() -> u64 {
        ALLOCATED_BYTES.with(|c| c.get())
    }
    /// Process-wide allocation total (all threads).
    pub fn total_allocation_count() -> u64 {
        TOTAL_ALLOCATIONS.load(Ordering::Relaxed)
    }
    /// Process-wide allocated-byte total (all threads).
    pub fn total_allocated_byte_count() -> u64 {
        TOTAL_ALLOCATED_BYTES.load(Ordering::Relaxed)
    }
    pub const INSTRUMENTED: bool = true;
}

#[cfg(not(feature = "perf-count-alloc"))]
pub mod counting {
    pub fn allocation_count() -> u64 {
        0
    }
    pub fn allocated_byte_count() -> u64 {
        0
    }
    pub const INSTRUMENTED: bool = false;
}

// Declared at the end of the list on purpose: #168 adds `decision_budget` in the
// alphabetical position above, and keeping this line in a different hunk lets
// the two PRs merge without a conflict.
pub use model::{Config, Game, Observation};
