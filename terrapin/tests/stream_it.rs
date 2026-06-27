//! Integration tests for the streaming + parallel build path
//! (`build_from_reader` / `identifier_from_reader`). Covers the BlockReader
//! splitting rules (spec §4.1) and the streaming/parallel guarantees (§2.1),
//! comparing every streamed result against the in-memory reference.

mod common;
use common::*;

use std::io::{self, Cursor, Read};

use terrapin::{
    build_from_reader, g, identifier, identifier_from_reader, tree_root, BuiltTree, BLOCK, FANOUT,
};

// ---------------------------------------------------------------------------
// Local helpers.
// ---------------------------------------------------------------------------

/// Algebraic zero-tree root for `n` zero bytes, computed without materializing
/// the dataset (used by the slow 2-layer oracle, REQ-SB-011).
fn tree_root_zero(n: u64) -> [u8; 32] {
    if n <= BLOCK as u64 {
        return tree_root(&vec![0u8; n as usize]);
    }
    let full = n / BLOCK as u64;
    let rem = (n % BLOCK as u64) as usize;
    let leaf = g(&vec![0u8; BLOCK]);
    let mut hf = Vec::new();
    for _ in 0..full {
        hf.extend_from_slice(&leaf);
    }
    if rem > 0 {
        hf.extend_from_slice(&g(&vec![0u8; rem]));
    }
    // Recurse like tree_root over the layer-1 hash file.
    tree_root(&hf)
}

/// A reader that delivers `prefix`, then a single premature `Ok(0)`, then
/// `trailing` bytes that a correct EOF interpretation must never read.
struct StopReader {
    prefix: Vec<u8>,
    ppos: usize,
    sent_zero: bool,
    trailing: Vec<u8>,
    tpos: usize,
}
impl Read for StopReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.ppos < self.prefix.len() {
            let n = (self.prefix.len() - self.ppos).min(buf.len());
            buf[..n].copy_from_slice(&self.prefix[self.ppos..self.ppos + n]);
            self.ppos += n;
            return Ok(n);
        }
        if !self.sent_zero {
            self.sent_zero = true;
            return Ok(0);
        }
        let n = (self.trailing.len() - self.tpos).min(buf.len());
        buf[..n].copy_from_slice(&self.trailing[self.tpos..self.tpos + n]);
        self.tpos += n;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// BlockReader splitting rules (§4.1).
// ---------------------------------------------------------------------------

// Verifies: REQ-BR-001
#[tokio::test]
async fn exact_multiple_yields_exactly_k_leaves() {
    for k in [1usize, 2, 3] {
        let data = fill(k * BLOCK, 100 + k as u64);
        let bt = build_from_reader(Cursor::new(data)).await.unwrap();
        assert_eq!(bt.layers[0].len() / 32, k, "k={} (no spurious empty leaf)", k);
    }
}

// Verifies: REQ-BR-002
#[tokio::test]
async fn short_final_block_yields_extra_leaf() {
    let k = 2usize;
    let r = 777usize;
    let data = fill(k * BLOCK + r, 42);
    let bt = build_from_reader(Cursor::new(data.clone())).await.unwrap();
    assert_eq!(bt.layers[0].len() / 32, k + 1, "k full + 1 short leaf");
    let last = &bt.layers[0][bt.layers[0].len() - 32..];
    assert_eq!(last, &g(&data[k * BLOCK..])[..], "last leaf is g(tail)");
}

// Verifies: REQ-BR-003
#[tokio::test]
async fn empty_reader_yields_one_leaf() {
    let bt = build_from_reader(Cursor::new(Vec::new())).await.unwrap();
    assert_eq!(bt.layers[0].len() / 32, 1, "exactly one empty leaf");
    let got = identifier_from_reader(Cursor::new(Vec::new())).await.unwrap();
    assert_eq!(got, identifier(b""), "empty identifier");
}

// Verifies: REQ-BR-004
#[tokio::test]
async fn choppy_reader_reassembles() {
    let data = fill(2 * BLOCK + 1234, 7);
    let want = identifier(&data);
    for chunk in [1usize, 3, 1000] {
        let got = identifier_from_reader(Choppy::new(data.clone(), chunk))
            .await
            .unwrap();
        assert_eq!(got, want, "chunk {}", chunk);
    }
}

// Verifies: REQ-BR-005
#[tokio::test]
async fn interrupted_is_retried() {
    let data = fill(BLOCK + 500, 9);
    let want = identifier(&data);
    let got = identifier_from_reader(InterruptOnce::new(data))
        .await
        .unwrap();
    assert_eq!(got, want);
}

// Verifies: REQ-BR-006
#[tokio::test]
async fn hard_error_surfaces() {
    let res = build_from_reader(ErrAfter::new(BLOCK + 10)).await;
    assert!(res.is_err(), "hard read error must surface as Err");
}

// Verifies: REQ-BR-007
#[tokio::test]
async fn premature_zero_is_eof() {
    let prefix = fill(BLOCK, 11); // exactly one block, so Ok(0) lands on a boundary
    let trailing = fill(500, 22); // must be ignored
    let reader = StopReader {
        prefix: prefix.clone(),
        ppos: 0,
        sent_zero: false,
        trailing,
        tpos: 0,
    };
    let got = identifier_from_reader(reader).await.unwrap();
    assert_eq!(got, identifier(&prefix), "only bytes before first Ok(0)");
}

// ---------------------------------------------------------------------------
// Streaming + parallel build guarantees (§2.1, §5.1, §5.3).
// ---------------------------------------------------------------------------

// Verifies: REQ-SB-003
#[tokio::test]
async fn nonzero_multiblock_matches_in_memory_and_builttree() {
    for len in [2 * BLOCK, 3 * BLOCK + 7] {
        let data = fill(len, 55 + len as u64);
        let want = identifier(&data);
        let from_reader = identifier_from_reader(Cursor::new(data.clone()))
            .await
            .unwrap();
        let bt: BuiltTree = build_from_reader(Cursor::new(data.clone())).await.unwrap();
        assert_eq!(from_reader, want, "identifier_from_reader len {}", len);
        assert_eq!(bt.identifier(), want, "BuiltTree.identifier len {}", len);
    }
}

// Verifies: REQ-SB-004
#[tokio::test]
async fn length_equals_byte_length() {
    let len = BLOCK + 333; // short final block included
    let data = fill(len, 3);
    let bt = build_from_reader(Cursor::new(data)).await.unwrap();
    assert_eq!(bt.length, len as u64);
}

// Verifies: REQ-SB-005
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deterministic_across_runs() {
    let data = fill(2 * BLOCK + 7, 88);
    let want = identifier(&data);
    for _ in 0..8 {
        let got = identifier_from_reader(Cursor::new(data.clone()))
            .await
            .unwrap();
        assert_eq!(got, want, "stable across repeated streaming runs");
    }
}

// Verifies: REQ-SB-006
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn block_order_preserved() {
    // Four distinct blocks; a reordering bug would change the identifier.
    let mut data = Vec::new();
    for i in 0..4u8 {
        data.extend(std::iter::repeat(i.wrapping_mul(37).wrapping_add(1)).take(BLOCK));
    }
    let want = identifier(&data);
    let got = identifier_from_reader(Cursor::new(data)).await.unwrap();
    assert_eq!(got, want);
}

// Verifies: REQ-SB-007
#[tokio::test]
async fn from_reader_equals_builttree_identifier() {
    let data = fill(BLOCK + 64, 17);
    let a = identifier_from_reader(Cursor::new(data.clone()))
        .await
        .unwrap();
    let b = build_from_reader(Cursor::new(data)).await.unwrap().identifier();
    assert_eq!(a, b);
}

// Verifies: REQ-SB-008
#[tokio::test]
async fn mid_stream_error_returns_err() {
    let res = identifier_from_reader(ErrAfter::new(BLOCK / 2)).await;
    assert!(res.is_err(), "mid-stream error must return Err");
}

// Verifies: REQ-SB-009
#[tokio::test(flavor = "current_thread")]
async fn works_on_single_threaded_runtime() {
    let data = fill(2 * BLOCK + 99, 21);
    let got = identifier_from_reader(Cursor::new(data.clone()))
        .await
        .unwrap();
    assert_eq!(got, identifier(&data), "no deadlock on current_thread");
}

// Verifies: REQ-SB-010
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn large_64mib_matches_in_memory() {
    let data = fill(64 * 1024 * 1024, 1234);
    let want = identifier(&data);
    let got = identifier_from_reader(Cursor::new(data)).await.unwrap();
    assert_eq!(got, want);
}

// Verifies: REQ-SB-011
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zeroreader_two_layer_matches_oracle() {
    let n = FANOUT as u64 * BLOCK as u64 + 1;
    let bt = build_from_reader(ZeroReader { remaining: n }).await.unwrap();
    assert_eq!(bt.root, tree_root_zero(n));
}

// ---------------------------------------------------------------------------
// Concurrency (§2.1, §6).
// ---------------------------------------------------------------------------

// Verifies: REQ-RT-001
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn correct_under_multi_thread_runtime() {
    let data = fill(3 * BLOCK + 11, 64);
    let got = identifier_from_reader(Cursor::new(data.clone()))
        .await
        .unwrap();
    assert_eq!(got, identifier(&data));
}

// Verifies: REQ-RT-003
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_builds_agree() {
    let data = fill(2 * BLOCK + 5, 71);
    let want = identifier(&data);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let d = data.clone();
        handles.push(tokio::spawn(async move {
            build_from_reader(Cursor::new(d)).await.unwrap().identifier()
        }));
    }
    for h in handles {
        assert_eq!(h.await.unwrap(), want);
    }
}
