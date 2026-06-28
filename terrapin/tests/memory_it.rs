//! Memory-bound and bench-smoke checks for the streaming path. This integration
//! test installs a process-global counting allocator (it is its own test crate,
//! so it does not perturb other suites). Both tests are `#[ignore]` (they hash
//! hundreds of MiB and the allocator adds overhead); run with:
//!   cargo test -p terrapin --test memory_it -- --ignored
mod common;
use common::*;

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicI64, Ordering};

use terrapin::build_from_reader;

/// Counting allocator: tracks live bytes (CUR) and the high-water mark (PEAK).
struct Counting;
static CUR: AtomicI64 = AtomicI64::new(0);
static PEAK: AtomicI64 = AtomicI64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let cur = CUR.fetch_add(layout.size() as i64, Ordering::Relaxed) + layout.size() as i64;
            let mut peak = PEAK.load(Ordering::Relaxed);
            while cur > peak {
                match PEAK.compare_exchange_weak(peak, cur, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(e) => peak = e,
                }
            }
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CUR.fetch_sub(layout.size() as i64, Ordering::Relaxed);
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

const MIB: u64 = 1024 * 1024;

/// Peak bytes allocated above the pre-build baseline while hashing `n` zero bytes
/// streamed from a ZeroReader (which allocates nothing itself).
fn peak_above_baseline(rt: &tokio::runtime::Runtime, n: u64) -> i64 {
    let base = CUR.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let t = rt
        .block_on(build_from_reader(ZeroReader { remaining: n }))
        .unwrap();
    black_box(t.root);
    PEAK.load(Ordering::Relaxed) - base
}

// Verifies: REQ-SB-012
#[test]
#[ignore]
fn streaming_holds_no_dataset_memory() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let peak_128 = peak_above_baseline(&rt, 128 * MIB);
    let peak_256 = peak_above_baseline(&rt, 256 * MIB);

    // Never resident: peak stays far below the dataset size (core-count-safe bound).
    assert!(
        (peak_256 as u64) < 256 * MIB,
        "peak {} >= dataset 256 MiB — dataset appears resident",
        peak_256
    );
    // Sub-linear: doubling the dataset (128 -> 256 MiB) does not grow peak
    // proportionally; the in-flight window + leaf hash file are independent of
    // dataset length. Allow generous slack for runtime/thread-pool variance.
    let delta = (peak_256 - peak_128).abs();
    assert!(
        (delta as u64) < 32 * MIB,
        "peak grew by {} bytes when the dataset doubled — not O(1) in dataset size",
        delta
    );
}

// Verifies: REQ-PERF-001
#[test]
#[ignore]
fn bench_smoke_streaming_completes() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let t = rt
        .block_on(build_from_reader(ZeroReader { remaining: 32 * MIB }))
        .unwrap();
    assert!(t.identifier().starts_with("terrapin-sha256:"));
    assert_eq!(t.length, 32 * MIB);
}
