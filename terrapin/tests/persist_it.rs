//! Integration tests for `derive_counts` (spec §6 / §4.3) and the two-file
//! `PersistedTree` artifact (spec §6: `<name>.head` text + `<name>.blocks`
//! raw 32-byte hashes). Only the public API plus the shared `common` helpers.

mod common;
use common::*;

use std::path::{Path, PathBuf};

use terrapin::{derive_counts, identifier, BuiltTree, PersistedTree, TreeBuilder, BLOCK, FANOUT};

// ---------------------------------------------------------------------------
// Local helpers (public API + std only).
// ---------------------------------------------------------------------------

/// Per-layer hash counts read straight off a built tree's layers.
fn counts_from_layers(t: &BuiltTree) -> Vec<u64> {
    t.layers.iter().map(|l| (l.len() / 32) as u64).collect()
}

/// `<path>.<ext>` as a sibling path (mirrors the crate's own naming).
fn add_ext(p: &Path, ext: &str) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

/// Read the `.head` text, transform it, write it back, then `read` the tree.
fn read_with_mangled_head(
    base: &TmpPath,
    f: impl FnOnce(String) -> String,
) -> Result<PersistedTree, String> {
    let hp = base.with_ext("head");
    let text = std::fs::read_to_string(&hp).unwrap();
    std::fs::write(&hp, f(text)).unwrap();
    PersistedTree::read(base.path())
}

/// Write a tree and return its read-back handle.
fn persist_and_read(base: &TmpPath, t: &BuiltTree) -> PersistedTree {
    PersistedTree::write(base.path(), t).unwrap();
    PersistedTree::read(base.path()).unwrap()
}

// ===========================================================================
// derive_counts (§6 / §4.3) — pure arithmetic, no allocation for huge sizes.
// ===========================================================================

// Verifies: REQ-DC-001
#[test]
fn derive_counts_small_sizes() {
    let block = BLOCK as u64;
    assert_eq!(derive_counts(0), vec![1]); // empty dataset => one empty leaf
    assert_eq!(derive_counts(1), vec![1]);
    assert_eq!(derive_counts(block), vec![1]); // exactly one full block
    assert_eq!(derive_counts(block + 1), vec![2]); // spills into a second block
}

// Verifies: REQ-DC-002
#[test]
fn derive_counts_exact_fit_boundaries() {
    let block = BLOCK as u64;
    let fanout = FANOUT as u64;
    // FANOUT blocks fill exactly one BLOCK-sized hash file: a single wrap, one layer.
    assert_eq!(derive_counts(fanout * block), vec![fanout]);
    // FANOUT^2 blocks: leaf layer collapses to exactly FANOUT in the next layer.
    assert_eq!(
        derive_counts(fanout * fanout * block),
        vec![fanout * fanout, fanout]
    );
}

// Verifies: REQ-DC-003
#[test]
fn derive_counts_multi_layer() {
    let block = BLOCK as u64;
    let fanout = FANOUT as u64;
    // One block past a full hash file => 2 layers above the leaves.
    assert_eq!(derive_counts(fanout * block + 1), vec![fanout + 1, 2]);
    // One block past FANOUT^2 => 3 layers.
    assert_eq!(
        derive_counts(fanout * fanout * block + 1),
        vec![fanout * fanout + 1, fanout + 1, 2]
    );
}

// Verifies: REQ-DC-004
#[test]
fn derive_counts_one_pib() {
    // 1 PiB worked example from spec §4.3.
    let one_pib: u64 = 1_125_899_906_842_624;
    assert_eq!(derive_counts(one_pib), vec![536_870_912, 8_192]);
}

// Verifies: REQ-DC-005
#[test]
fn derive_counts_u64_max_terminates() {
    let counts = derive_counts(u64::MAX); // must not overflow or panic
    let block = BLOCK as u64;
    let fanout = FANOUT as u64;
    assert_eq!(counts[0], u64::MAX.div_ceil(block));
    // Strictly shrinking layer-by-layer, so the recursion is finite.
    for w in counts.windows(2) {
        assert!(w[0] > w[1], "counts must strictly shrink: {:?}", counts);
    }
    // The top layer fits within a single hash-file block.
    assert!(*counts.last().unwrap() <= fanout);
    assert!(!counts.is_empty());
}

// Verifies: REQ-DC-006
#[test]
fn derive_counts_matches_builder_layers() {
    // Real multi-block datasets: derive_counts == the builder's actual layers.
    for &len in &[2 * BLOCK, 3 * BLOCK + 10, 5 * BLOCK + 1] {
        let data = fill(len, 9);
        let t = build_tree(&data);
        assert_eq!(
            derive_counts(len as u64),
            counts_from_layers(&t),
            "len {}",
            len
        );
    }
    // Synthetic FANOUT+1 leaves force a genuine 2-layer structure ([FANOUT+1, 2])
    // without allocating a multi-gigabyte dataset.
    let n = FANOUT + 1;
    let mut b = TreeBuilder::new();
    for i in 0..n {
        b.push_leaf(&terrapin::g(&(i as u64).to_le_bytes()));
    }
    let length = n as u64 * BLOCK as u64;
    let t = b.build(length);
    assert_eq!(counts_from_layers(&t), vec![FANOUT as u64 + 1, 2]);
    assert_eq!(derive_counts(length), counts_from_layers(&t));
}

// ===========================================================================
// PersistedTree write / read (§6).
// ===========================================================================

// Verifies: REQ-PT-001
#[test]
fn blocks_file_size_and_content() {
    // Synthetic FANOUT+1 leaf tree => two layers, so we exercise the
    // concatenation of more than one layer in the .blocks file.
    let n = FANOUT + 1;
    let mut b = TreeBuilder::new();
    for i in 0..n {
        b.push_leaf(&terrapin::g(&(i as u64).to_le_bytes()));
    }
    let t = b.build(n as u64 * BLOCK as u64);

    let base = TmpPath::new("blocks");
    PersistedTree::write(base.path(), &t).unwrap();

    let blocks = std::fs::read(base.with_ext("blocks")).unwrap();
    let counts = counts_from_layers(&t);
    let expected: Vec<u8> = t.layers.concat();

    assert_eq!(blocks.len() as u64, counts.iter().sum::<u64>() * 32);
    assert_eq!(blocks, expected, ".blocks must be layers[0]++layers[1]++...");
}

// Verifies: REQ-PT-002
#[test]
fn head_exact_text_format() {
    let data = fill(100, 3); // single block => layer_counts "1"
    let t = build_tree(&data);
    let base = TmpPath::new("headfmt");
    PersistedTree::write(base.path(), &t).unwrap();

    let text = std::fs::read_to_string(base.with_ext("head")).unwrap();
    let expected = format!(
        "terrapin-tree: 1\n\
         algorithm: terrapin-sha256\n\
         block_size: 2097152\n\
         length: {}\n\
         tree: {}\n\
         identifier: {}\n\
         layer_counts: 1\n",
        t.length,
        t.tree_hex(),
        t.identifier(),
    );
    assert_eq!(text, expected);
    // Exactly seven "key: value" lines.
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 7);
    for line in lines {
        assert!(line.contains(": "), "line not key: value -> {:?}", line);
    }
}

// Verifies: REQ-PT-003
#[test]
fn artifact_is_byte_reproducible() {
    let data = fill(3 * BLOCK + 10, 4);
    let t = build_tree(&data);

    let a = TmpPath::new("repro-a");
    let b = TmpPath::new("repro-b");
    PersistedTree::write(a.path(), &t).unwrap();
    PersistedTree::write(b.path(), &t).unwrap();

    assert_eq!(
        std::fs::read(a.with_ext("head")).unwrap(),
        std::fs::read(b.with_ext("head")).unwrap()
    );
    assert_eq!(
        std::fs::read(a.with_ext("blocks")).unwrap(),
        std::fs::read(b.with_ext("blocks")).unwrap()
    );
}

// Verifies: REQ-PT-004
#[test]
fn with_ext_naming_for_dotted_base() {
    // A base name that already contains a dot must yield foo.bin.head /
    // foo.bin.blocks (extension appended, not replaced).
    let tp = TmpPath::new("dotted");
    let base = add_ext(tp.path(), "bin"); // ".../dotted-N.bin"
    let t = build_tree(&fill(100, 5));
    PersistedTree::write(&base, &t).unwrap();

    let head = add_ext(&base, "head"); // ".../dotted-N.bin.head"
    let blocks = add_ext(&base, "blocks");
    assert!(head.exists(), "{} should exist", head.display());
    assert!(blocks.exists(), "{} should exist", blocks.display());

    // Clean up the dotted siblings (TmpPath only knows its own .head/.blocks).
    let _ = std::fs::remove_file(&head);
    let _ = std::fs::remove_file(&blocks);
}

// Verifies: REQ-PT-005
#[test]
fn read_roundtrip_preserves_fields() {
    let len = 3 * BLOCK + 10;
    let data = fill(len, 1);
    let t = build_tree(&data);
    let base = TmpPath::new("roundtrip");
    let pt = persist_and_read(&base, &t);

    assert_eq!(pt.length, t.length);
    assert_eq!(pt.tree_hex, t.tree_hex());
    assert_eq!(pt.identifier, t.identifier());
    assert_eq!(pt.identifier, identifier(&data));
    assert_eq!(pt.counts, derive_counts(len as u64));
}

// Verifies: REQ-PT-006
#[test]
fn read_rejects_missing_head() {
    let base = TmpPath::new("missing"); // nothing written
    assert!(PersistedTree::read(base.path()).is_err());
}

// Verifies: REQ-PT-007
#[test]
fn read_rejects_malformed_unknown_and_missing_key() {
    let t = build_tree(&fill(100, 2));

    // (a) A line with no ": " separator.
    let base = TmpPath::new("malformed");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| format!("{}not-a-kv-line\n", s)).is_err());

    // (b) An unknown key.
    let base = TmpPath::new("unknownkey");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| format!("{}bogus: 1\n", s)).is_err());

    // (c) A missing required key (drop the identifier line).
    let base = TmpPath::new("missingkey");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| s
        .lines()
        .filter(|l| !l.starts_with("identifier: "))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
    .is_err());
}

// Verifies: REQ-PT-008
#[test]
fn read_rejects_bad_version_block_size_algorithm() {
    let t = build_tree(&fill(100, 6));

    let base = TmpPath::new("badver");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| s.replace("terrapin-tree: 1", "terrapin-tree: 2")).is_err());

    let base = TmpPath::new("badblk");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| s.replace("block_size: 2097152", "block_size: 4096")).is_err());

    let base = TmpPath::new("badalg");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| s
        .replace("algorithm: terrapin-sha256", "algorithm: terrapin-md5"))
    .is_err());
}

// Verifies: REQ-PT-009
#[test]
fn read_rejects_inconsistent_layer_counts() {
    // Single-block tree has layer_counts "1"; forge "2" which disagrees with
    // derive_counts(length).
    let t = build_tree(&fill(100, 7));
    let base = TmpPath::new("badcounts");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| s.replace("layer_counts: 1", "layer_counts: 2")).is_err());
}

// Verifies: REQ-PT-010
#[test]
fn read_rejects_non_numeric_layer_counts() {
    let t = build_tree(&fill(100, 8));
    let base = TmpPath::new("nonnum");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(read_with_mangled_head(&base, |s| s.replace("layer_counts: 1", "layer_counts: x")).is_err());
}

// Verifies: REQ-PT-011
#[test]
fn head_whitespace_and_crlf_policy() {
    let t = build_tree(&fill(100, 11));

    // Documented behavior #1: an EXTRA trailing newline is REJECTED. The parser
    // uses str::lines(); the canonical single trailing LF is fine, but a second
    // LF yields an empty line that has no ": " separator and is rejected.
    let base = TmpPath::new("trailnl");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(
        read_with_mangled_head(&base, |s| format!("{}\n", s)).is_err(),
        "extra trailing newline should be rejected"
    );

    // Documented behavior #2: full CRLF line endings are TOLERATED. str::lines()
    // strips the trailing \r from each line, so values do not carry \r and the
    // header still parses to the canonical fields.
    let base = TmpPath::new("crlf");
    PersistedTree::write(base.path(), &t).unwrap();
    assert!(
        read_with_mangled_head(&base, |s| s.replace('\n', "\r\n")).is_ok(),
        "uniform CRLF line endings should be tolerated by str::lines()"
    );
}

// Verifies: REQ-HEX-004
#[test]
fn head_tree_hex_case_policy() {
    // Documented behavior: read() stores the `tree:` value verbatim and performs
    // NO case normalization and NO rejection of uppercase hex at read time
    // (identifier binding is reconstructed canonically only during validation).
    // So an uppercased tree hex is accepted and stored as-is.
    let data = fill(100, 12);
    let t = build_tree(&data);
    let base = TmpPath::new("hexcase");
    PersistedTree::write(base.path(), &t).unwrap();

    let lower = t.tree_hex();
    let upper = lower.to_uppercase();
    let pt = read_with_mangled_head(&base, |s| s.replace(&lower, &upper))
        .expect("uppercase tree hex is accepted at read time");
    assert_eq!(pt.tree_hex, upper, "read stores the tree hex verbatim");
}
