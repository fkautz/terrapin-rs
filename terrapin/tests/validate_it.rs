//! Integration tests for `PersistedTree::validate` (spec section 6, section 7).
//!
//! Each test carries exactly one `// Verifies: REQ-...` comment placed
//! immediately above its `#[test]` attribute for the traceability gate.
//!
//! Validation model (spec section 6): `validate` first binds the head to its
//! identifier (`G(manifest) == identifier`), then re-hashes the data blocks
//! covering the requested range and recomputes `G` upward to the tree root.
//! For a single-layer tree (data <= FANOUT*BLOCK) validating any non-empty range
//! reads the entire leaf hash file to recompute the root, so corrupting a leaf
//! hash fails every range while data tampered outside the read range is invisible.

mod common;
use common::*;

use terrapin::{g, identifier_from_parts, to_hex, BuiltTree, PersistedTree, TreeBuilder, BLOCK, FANOUT};

// ---------------------------------------------------------------------------
// Local helpers (only public API + common helpers).
// ---------------------------------------------------------------------------

/// Persist a built tree under a fresh base name and open it for reading.
fn persist(data: &[u8], tag: &str) -> (TmpPath, TmpPath, PersistedTree) {
    let dp = TmpPath::new(tag);
    std::fs::write(dp.path(), data).unwrap();
    let base = TmpPath::new("tree");
    PersistedTree::write(base.path(), &build_tree(data)).unwrap();
    let pt = PersistedTree::read(base.path()).unwrap();
    (dp, base, pt)
}

/// A multi-block (4 leaves, single-layer) dataset with a short final block.
fn multi() -> Vec<u8> {
    fill(3 * BLOCK + 1234, 1)
}

// ===========================================================================
// Success cases (spec section 6).
// ===========================================================================

// Verifies: REQ-VAL-001
#[test]
fn whole_multi_block_file_validates() {
    let data = multi();
    let (dp, _base, pt) = persist(&data, "data");
    pt.validate(dp.path(), None, None, None).unwrap();
}

// Verifies: REQ-VAL-004
#[test]
fn single_block_file_validates() {
    // data <= BLOCK -> single-leaf tree, root is the bare leaf.
    let data = fill(1000, 5);
    let (dp, _base, pt) = persist(&data, "data");
    pt.validate(dp.path(), None, None, None).unwrap();
}

// Verifies: REQ-VAL-005
#[test]
fn default_and_one_sided_ranges_agree() {
    let data = multi();
    let len = data.len() as u64;
    let (dp, _base, pt) = persist(&data, "data");

    // None/None whole file.
    let mut a = Vec::new();
    pt.validate(dp.path(), None, None, Some(&mut a)).unwrap();
    // start=Some(0), end=None -> to the end.
    let mut b = Vec::new();
    pt.validate(dp.path(), Some(0), None, Some(&mut b)).unwrap();
    // start=None, end=Some(len) -> from the start.
    let mut c = Vec::new();
    pt.validate(dp.path(), None, Some(len), Some(&mut c)).unwrap();

    assert_eq!(a, data, "None/None streams the whole dataset");
    assert_eq!(a, b, "start=Some(0) agrees with None/None");
    assert_eq!(a, c, "end=Some(len) agrees with None/None");
}

// Verifies: REQ-VAL-006
#[test]
fn empty_ranges_succeed() {
    let data = multi();
    let len = data.len() as u64;
    let (dp, _base, pt) = persist(&data, "data");

    pt.validate(dp.path(), Some(0), Some(0), None).unwrap();
    let mid = len / 2;
    pt.validate(dp.path(), Some(mid), Some(mid), None).unwrap();
    pt.validate(dp.path(), Some(len), Some(len), None).unwrap();
}

// Verifies: REQ-VAL-007
#[test]
fn block_aligned_and_straddling_ranges_succeed() {
    let data = multi();
    let (dp, _base, pt) = persist(&data, "data");

    // Exactly one whole block.
    pt.validate(dp.path(), Some(BLOCK as u64), Some(2 * BLOCK as u64), None)
        .unwrap();
    // Straddling a block boundary.
    pt.validate(dp.path(), Some(BLOCK as u64 - 1), Some(BLOCK as u64 + 1), None)
        .unwrap();
}

// Verifies: REQ-VAL-008
#[test]
fn single_byte_ranges_succeed() {
    let data = multi();
    let len = data.len() as u64;
    let (dp, _base, pt) = persist(&data, "data");

    pt.validate(dp.path(), Some(0), Some(1), None).unwrap();
    pt.validate(dp.path(), Some(BLOCK as u64), Some(BLOCK as u64 + 1), None)
        .unwrap();
    pt.validate(dp.path(), Some(len - 1), Some(len), None).unwrap();
}

// Verifies: REQ-VAL-009
#[test]
fn last_partial_block_range_succeeds() {
    let data = multi();
    let len = data.len() as u64;
    let (dp, _base, pt) = persist(&data, "data");

    // The short final block: [3*BLOCK, len).
    let mut out = Vec::new();
    pt.validate(dp.path(), Some(3 * BLOCK as u64), Some(len), Some(&mut out))
        .unwrap();
    assert_eq!(out, &data[3 * BLOCK..]);
}

// Verifies: REQ-VAL-010
#[test]
fn content_addressed_copy_validates() {
    let data = multi();
    let (_dp, base, pt) = persist(&data, "data");

    // A byte-identical copy at a different path validates against the same tree.
    let copy = TmpPath::new("copy");
    std::fs::write(copy.path(), &data).unwrap();
    pt.validate(copy.path(), None, None, None).unwrap();
    let _ = &base;
}

// Verifies: REQ-VAL-011
#[test]
fn validation_is_idempotent() {
    let data = multi();
    let (dp, _base, pt) = persist(&data, "data");
    pt.validate(dp.path(), None, None, None).unwrap();
    pt.validate(dp.path(), None, None, None).unwrap();
}

// Verifies: REQ-VAL-012
#[test]
#[ignore]
fn two_layer_sparse_file_range_validates() {
    // Best-effort large-scale case (requires >= 128 GiB of sparse-file support).
    // 128 GiB / 2 MiB == 65536 == FANOUT leaves -> a single leaf layer.
    let length: u64 = 128 * 1024 * 1024 * 1024;
    let nblocks = length / BLOCK as u64; // 65536
    assert_eq!(nblocks, FANOUT as u64);

    // Stream zeros: every full block hashes to g(zero_block); no data held.
    let zero_block = vec![0u8; BLOCK];
    let zero_leaf = g(&zero_block);
    let mut b = TreeBuilder::new();
    for _ in 0..nblocks {
        b.push_leaf(&zero_leaf);
    }
    let tree: BuiltTree = b.build(length);

    let base = TmpPath::new("tree");
    PersistedTree::write(base.path(), &tree).unwrap();
    let pt = PersistedTree::read(base.path()).unwrap();

    // A sparse zero file of the committed length.
    let dp = TmpPath::new("sparse");
    let f = std::fs::File::create(dp.path()).unwrap();
    f.set_len(length).unwrap();
    drop(f);

    // Validate a single-block range; reads block 0 (zeros) + the leaf hash file.
    pt.validate(dp.path(), Some(0), Some(BLOCK as u64), None).unwrap();
}

// ===========================================================================
// Failure cases (spec section 6, section 7).
// ===========================================================================

// Verifies: REQ-VF-001
#[test]
fn tampered_data_inside_range_fails() {
    let data = multi();
    let (_dp, _base, pt) = persist(&data, "data");

    // Tamper a byte in block 1, then validate the whole file (covers block 1).
    let mut bad = data.clone();
    bad[BLOCK + 7] ^= 0xff;
    let badp = TmpPath::new("bad");
    std::fs::write(badp.path(), &bad).unwrap();
    assert!(pt.validate(badp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-003
#[test]
fn data_length_mismatch_fails() {
    let data = multi();
    let (_dp, _base, pt) = persist(&data, "data");

    // Truncated data file.
    let short = TmpPath::new("short");
    std::fs::write(short.path(), &data[..data.len() - 100]).unwrap();
    assert!(pt.validate(short.path(), None, None, None).is_err());

    // Extended data file.
    let long = TmpPath::new("long");
    let mut more = data.clone();
    more.extend_from_slice(&[0u8; 100]);
    std::fs::write(long.path(), &more).unwrap();
    assert!(pt.validate(long.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-004
#[test]
fn out_of_bounds_range_errors_not_panics() {
    let data = multi();
    let len = data.len() as u64;
    let (dp, _base, pt) = persist(&data, "data");

    // start > end.
    assert!(pt.validate(dp.path(), Some(100), Some(50), None).is_err());
    // end > length.
    assert!(pt.validate(dp.path(), None, Some(len + 10), None).is_err());
    // start > length (with default end == length this is also start > end).
    assert!(pt.validate(dp.path(), Some(len + 1), None, None).is_err());
}

// Verifies: REQ-VF-005
#[test]
fn corrupt_leaf_hash_fails_any_range() {
    let data = multi();
    let (dp, base, pt) = persist(&data, "data");

    // Flip a byte in leaf hash index 2 inside the .blocks file.
    let blocks_path = base.with_ext("blocks");
    let mut blocks = std::fs::read(&blocks_path).unwrap();
    blocks[2 * 32] ^= 0xff;
    std::fs::write(&blocks_path, &blocks).unwrap();

    // A single-layer tree recomputes the root from the whole leaf file, so even
    // a range not covering block 2 fails.
    assert!(pt.validate(dp.path(), None, None, None).is_err());
    assert!(pt.validate(dp.path(), Some(0), Some(1), None).is_err());
}

// Verifies: REQ-VF-006
#[test]
fn data_tamper_outside_range_still_validates() {
    let data = multi();
    let (_dp, _base, pt) = persist(&data, "data");

    // Tamper a byte in block 2, but validate only block 0's range. Block 2 is
    // never read, and the published leaf file (untouched) still recomputes the
    // root, so validation succeeds (slice independence, spec section 6).
    let mut bad = data.clone();
    bad[2 * BLOCK + 5] ^= 0xff;
    let badp = TmpPath::new("bad");
    std::fs::write(badp.path(), &bad).unwrap();
    pt.validate(badp.path(), Some(0), Some(100), None).unwrap();
}

// Verifies: REQ-VF-007
#[test]
fn truncated_blocks_errors_not_panics() {
    let data = multi();
    let (dp, base, pt) = persist(&data, "data");

    // Truncate .blocks so the leaf group read runs past EOF.
    let blocks_path = base.with_ext("blocks");
    let blocks = std::fs::read(&blocks_path).unwrap();
    std::fs::write(&blocks_path, &blocks[..32]).unwrap();

    assert!(pt.validate(dp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-008
#[test]
fn missing_blocks_file_errors() {
    let data = multi();
    let (dp, base, pt) = persist(&data, "data");

    std::fs::remove_file(base.with_ext("blocks")).unwrap();
    assert!(pt.validate(dp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-009
#[test]
fn missing_data_file_errors() {
    let data = multi();
    let (_dp, _base, pt) = persist(&data, "data");

    let missing = TmpPath::new("nope");
    // Intentionally never create the file.
    assert!(pt.validate(missing.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-010
#[test]
fn different_data_same_length_fails() {
    let data = multi();
    let (_dp, _base, pt) = persist(&data, "data");

    // Different content, identical length.
    let other = fill(data.len(), 99);
    assert_ne!(other, data);
    let op = TmpPath::new("other");
    std::fs::write(op.path(), &other).unwrap();
    assert!(pt.validate(op.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-011
#[test]
fn swapped_blocks_fail() {
    let data = multi();
    let (_dp, _base, pt) = persist(&data, "data");

    // Swap whole blocks 0 and 1 (same bytes, different positions).
    let mut swapped = data.clone();
    let (a, b) = swapped.split_at_mut(BLOCK);
    a[..BLOCK].swap_with_slice(&mut b[..BLOCK]);
    let sp = TmpPath::new("swap");
    std::fs::write(sp.path(), &swapped).unwrap();
    assert!(pt.validate(sp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-012
#[test]
fn corrupt_head_identifier_fails() {
    let data = vec![7u8; BLOCK + 10];
    let dp = TmpPath::new("data");
    std::fs::write(dp.path(), &data).unwrap();
    let base = TmpPath::new("tree");
    let bt = build_tree(&data);
    PersistedTree::write(base.path(), &bt).unwrap();

    // Corrupt only the identifier field in the head (leave tree root intact).
    let head_path = base.with_ext("head");
    let text = std::fs::read_to_string(&head_path).unwrap();
    let bad_id = format!("terrapin-sha256:{}", "0".repeat(64));
    let corrupted = text.replace(&bt.identifier(), &bad_id);
    assert_ne!(corrupted, text, "identifier field must have been present");
    std::fs::write(&head_path, corrupted).unwrap();

    let pt = PersistedTree::read(base.path()).unwrap();
    // G(manifest) recomputes to the true identifier, which no longer matches.
    assert!(pt.validate(dp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-013
#[test]
fn single_leaf_tree_tamper_fails() {
    let data = fill(1234, 3); // <= BLOCK -> single-leaf tree.
    let (_dp, _base, pt) = persist(&data, "data");

    let mut bad = data.clone();
    bad[0] ^= 0xff;
    let badp = TmpPath::new("bad");
    std::fs::write(badp.path(), &bad).unwrap();
    assert!(pt.validate(badp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-014
#[test]
fn empty_tree_vs_nonempty_file_fails() {
    // Tree for the empty dataset.
    let base = TmpPath::new("tree");
    PersistedTree::write(base.path(), &build_tree(b"")).unwrap();
    let pt = PersistedTree::read(base.path()).unwrap();

    // Validate against a non-empty data file -> length mismatch.
    let dp = TmpPath::new("data");
    std::fs::write(dp.path(), b"not empty").unwrap();
    assert!(pt.validate(dp.path(), None, None, None).is_err());
}

// Verifies: REQ-VF-015
#[test]
#[ignore]
fn two_layer_upper_node_tamper_fails() {
    // Best-effort large-scale case (requires >= 128 GiB of sparse-file support).
    // FANOUT+1 leaves force a second layer: counts == [65537, 2].
    let nblocks = FANOUT as u64 + 1;
    let length = nblocks * BLOCK as u64;

    let zero_block = vec![0u8; BLOCK];
    let zero_leaf = g(&zero_block);
    let mut b = TreeBuilder::new();
    for _ in 0..nblocks {
        b.push_leaf(&zero_leaf);
    }
    let tree = b.build(length);

    let base = TmpPath::new("tree");
    PersistedTree::write(base.path(), &tree).unwrap();
    let pt = PersistedTree::read(base.path()).unwrap();
    assert_eq!(pt.counts.len(), 2, "FANOUT+1 leaves form a two-layer tree");

    // Corrupt the first hash of the upper (layer 1) hash file, which lies on
    // block 0's validation path. Layer 1 begins right after the leaf file.
    let blocks_path = base.with_ext("blocks");
    let mut blocks = std::fs::read(&blocks_path).unwrap();
    let layer1_off = (nblocks * 32) as usize;
    blocks[layer1_off] ^= 0xff;
    std::fs::write(&blocks_path, &blocks).unwrap();

    // Sanity: the published root is unchanged, so the identifier binding holds
    // and the failure must come from the corrupted upper node, not the head.
    assert_eq!(pt.tree_hex, to_hex(&tree.root));
    let _ = identifier_from_parts(length, &tree.root);

    let dp = TmpPath::new("sparse");
    let f = std::fs::File::create(dp.path()).unwrap();
    f.set_len(length).unwrap();
    drop(f);

    assert!(pt.validate(dp.path(), Some(0), Some(BLOCK as u64), None).is_err());
}
