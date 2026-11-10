pub mod api;
pub mod benchmark;
pub mod breeding;
pub mod card_pool;
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
pub mod hooks;
pub mod mcts;
pub mod mcts_puct;
pub mod model;
pub mod policy;
pub mod registry;
pub mod score;
pub mod validation;
pub mod verification;
pub mod web;

#[cfg(feature = "perf-count-alloc")]
pub mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Counts heap allocations/bytes. Only used in the dedicated allocation
    /// run; timing runs use the plain build and the `System` allocator.
    struct CountingAllocator;

    static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
    static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    pub fn allocation_count() -> u64 {
        ALLOCATIONS.load(Ordering::Relaxed)
    }
    pub fn allocated_byte_count() -> u64 {
        ALLOCATED_BYTES.load(Ordering::Relaxed)
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
