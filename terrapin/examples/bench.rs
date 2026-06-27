// Throughput bench for Terrapin (gitoid boringssl backend).
// Run: cargo run -p terrapin --example bench --release
use std::hint::black_box;
use std::io::Cursor;
use std::time::Instant;
use terrapin::{build_from_reader, g, tree_root, BLOCK};

fn bench<F: Fn()>(name: &str, bytes: usize, iters: usize, f: F) {
    f(); // warmup
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let dur = start.elapsed();
    let gbps = (bytes as f64 * iters as f64) / dur.as_secs_f64() / 1e9;
    println!(
        "{:<28} {:>6.2} GB/s   ({} iters, {:.3}s)",
        name,
        gbps,
        iters,
        dur.as_secs_f64()
    );
}

fn main() {
    let block = vec![0x5au8; BLOCK]; // 2 MiB
    bench("G 2MiB", block.len(), 3000, || {
        black_box(g(black_box(&block)));
    });

    let obj = vec![0x37u8; 16 * BLOCK]; // 32 MiB
    bench("tree_root 32MiB (1 thread)", obj.len(), 200, || {
        black_box(tree_root(black_box(&obj)));
    });

    // Streaming + parallel construction over the same 32 MiB object.
    let rt = tokio::runtime::Runtime::new().unwrap();
    bench("build_from_reader 32MiB (par)", obj.len(), 200, || {
        let t = rt
            .block_on(build_from_reader(Cursor::new(obj.clone())))
            .unwrap();
        black_box(t.root);
    });
}
