//! Spec section 6.1 transfer-set: validating a byte range fetches exactly one
//! hash-file block per layer along the path. Exercised via PersistedTree::path_blocks,
//! which shares validate's group arithmetic.
mod common;
use common::*;

use terrapin::{g, BuiltTree, PersistedTree, TreeBuilder, BLOCK, FANOUT};

/// Build a real 2-layer tree from FANOUT+1 synthetic leaves (no 128 GiB of data
/// needed — the tree is structural). length is the smallest that yields FANOUT+1
/// leaf blocks so PersistedTree::read accepts it.
fn two_layer_tree() -> (BuiltTree, u64) {
    let mut b = TreeBuilder::new();
    for i in 0..(FANOUT as u64 + 1) {
        b.push_leaf(&g(&i.to_le_bytes()));
    }
    let length = FANOUT as u64 * BLOCK as u64 + 1; // -> derive_counts == [FANOUT+1, 2]
    (b.build(length), length)
}

// Verifies: REQ-WE-004
#[test]
fn validate_reads_one_hash_file_block_per_layer() {
    let (tree, _len) = two_layer_tree();
    let base = TmpPath::new("transfer-tree");
    PersistedTree::write(base.path(), &tree).unwrap();
    let pt = PersistedTree::read(base.path()).unwrap();
    assert_eq!(pt.counts, vec![FANOUT as u64 + 1, 2], "expected a 2-layer tree");
    let nlayers = pt.counts.len();

    // Validating the FIRST data block touches exactly one hash-file block per
    // layer: the leaf group [0, FANOUT) and the single layer-1 group [0, 2).
    let first = pt.path_blocks(Some(0), Some(1)).unwrap();
    assert_eq!(first.len(), nlayers, "one hash-file block per layer");
    assert_eq!(first[0], (0, 0, FANOUT)); // leaf group
    assert_eq!(first[1], (1, 0, 2)); // top group
    // exactly one block per distinct layer
    let mut layers_seen: Vec<usize> = first.iter().map(|e| e.0).collect();
    layers_seen.dedup();
    assert_eq!(layers_seen, vec![0, 1]);

    // A block in the SECOND leaf group (index FANOUT) still touches one block per
    // layer: leaf group [FANOUT, FANOUT+1) and the same top group.
    let start = FANOUT as u64 * BLOCK as u64;
    let second = pt.path_blocks(Some(start), Some(start + 1)).unwrap();
    assert_eq!(second.len(), nlayers);
    assert_eq!(second[0], (0, FANOUT as u64, 1));
    assert_eq!(second[1], (1, 0, 2));

    // Transfer bound: total bytes fetched for a single-block validation is at
    // most one BLOCK per layer (spec section 6.1 / 10.0).
    let total_bytes: usize = first.iter().map(|(_, _, len)| len * 32).sum();
    assert!(total_bytes <= nlayers * BLOCK, "transfer set exceeds one block/layer");
}

// Supporting sanity checks for path_blocks (untagged; REQ-WE-004 is the primary).
#[test]
fn path_blocks_degenerate_shapes() {
    // 1-layer tree: a single-block range reads exactly the one leaf group.
    let data = fill(3 * BLOCK + 5, 7);
    let base = TmpPath::new("transfer-1layer");
    PersistedTree::write(base.path(), &build_tree(&data)).unwrap();
    let pt = PersistedTree::read(base.path()).unwrap();
    assert_eq!(pt.counts.len(), 1);
    let pb = pt.path_blocks(Some(BLOCK as u64), Some(BLOCK as u64 + 1)).unwrap();
    assert_eq!(pb.len(), 1, "1-layer: one leaf group");
    assert_eq!(pb[0].0, 0);

    // Single-leaf tree and empty range read no hash-file blocks.
    let small = fill(100, 1);
    let base2 = TmpPath::new("transfer-leaf");
    PersistedTree::write(base2.path(), &build_tree(&small)).unwrap();
    let pt2 = PersistedTree::read(base2.path()).unwrap();
    assert!(pt2.path_blocks(None, None).unwrap().is_empty(), "single leaf reads no groups");
    assert!(pt.path_blocks(Some(10), Some(10)).unwrap().is_empty(), "empty range reads nothing");
}
