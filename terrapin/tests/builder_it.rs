//! Integration tests for `TreeBuilder` / `BuiltTree` (spec section 4).
//!
//! Each test is tagged with exactly one `// Verifies: REQ-...` comment placed
//! immediately above its `#[test]` attribute for the traceability gate.

mod common;
use common::*;

use terrapin::{
    g, identifier, identifier_from_parts, to_hex, tree_root, BuiltTree, TreeBuilder, BLOCK, FANOUT,
};

// Verifies: REQ-TB-003
#[test]
fn single_leaf_is_bare_leaf_and_matching_identifier() {
    // A dataset that fits in one block: one pushed leaf == g(data).
    let data = fill(1000, 7);
    let leaf = g(&data);

    let mut b = TreeBuilder::new();
    b.push_leaf(&leaf);
    let bt: BuiltTree = b.build(data.len() as u64);

    // Root is the bare leaf, not g(leaf).
    assert_eq!(bt.root, leaf, "root must be the bare leaf hash");
    // Exactly one layer holding exactly one 32-byte hash.
    assert_eq!(bt.layers.len(), 1, "single leaf yields one layer");
    assert_eq!(bt.layers[0].len(), 32, "leaf layer is one 32-byte hash");
    assert_eq!(&bt.layers[0][..], &leaf[..], "leaf layer equals the leaf");
    // For data <= BLOCK the BuiltTree identifier equals terrapin::identifier(data).
    assert_eq!(bt.identifier(), identifier(&data));
}

// Verifies: REQ-TB-004
#[test]
fn empty_path_yields_g_empty_root() {
    let empty = g(b"");

    let mut b = TreeBuilder::new();
    b.push_leaf(&empty);
    let bt = b.build(0);

    assert_eq!(bt.root, empty, "empty dataset root is g(\"\")");
    assert_eq!(bt.identifier(), identifier(b""));
}

// Verifies: REQ-TB-005
#[test]
fn multi_layer_structure_for_fanout_plus_one_leaves() {
    // Push FANOUT+1 distinct synthetic leaves directly: no data materialized.
    let n = FANOUT + 1;
    let mut b = TreeBuilder::new();
    for i in 0..n {
        b.push_leaf(&g(&(i as u64).to_le_bytes()));
    }
    let bt = b.build(n as u64 * BLOCK as u64);

    assert_eq!(bt.layers.len(), 2, "FANOUT+1 leaves produce two layers");
    assert_eq!(bt.layers[0].len(), n * 32, "leaf layer holds FANOUT+1 hashes");
    assert_eq!(bt.layers[1].len(), 2 * 32, "second layer holds two hashes");

    // First node is g over the first full FANOUT-hash (BLOCK-byte) group.
    assert_eq!(&bt.layers[1][0..32], &g(&bt.layers[0][0..FANOUT * 32])[..]);
    // Second node is g over the trailing single hash.
    assert_eq!(&bt.layers[1][32..64], &g(&bt.layers[0][FANOUT * 32..])[..]);
    // Root is g over the (<= FANOUT-hash) top layer.
    assert_eq!(bt.root, g(&bt.layers[1]));
}

// Verifies: REQ-TB-006
#[test]
fn internal_consistency_real_multi_block_tree() {
    let data = fill(3 * BLOCK + 9, 1);
    let bt = build_tree(&data);

    // Each upper layer is the concatenation of g over FANOUT-groups below it.
    for l in 0..bt.layers.len() - 1 {
        let expected: Vec<u8> = bt.layers[l].chunks(BLOCK).flat_map(g).collect();
        assert_eq!(bt.layers[l + 1], expected, "layer {} -> {}", l, l + 1);
    }

    // Root: bare leaf if a single leaf, else g over the top layer.
    let top = bt.layers.last().unwrap();
    if top.len() == 32 {
        assert_eq!(&bt.root[..], &top[..], "single-leaf root is the bare leaf");
    } else {
        assert_eq!(bt.root, g(top), "root is g(top layer)");
    }
}

// Verifies: REQ-TB-007
#[test]
fn leaf_count_accurate() {
    let mut b = TreeBuilder::new();
    assert_eq!(b.leaf_count(), 0);
    for i in 1..=10u64 {
        b.push_leaf(&g(&i.to_le_bytes()));
        assert_eq!(b.leaf_count(), i, "after {} pushes", i);
    }
}

// Verifies: REQ-TB-008
#[test]
fn leaf_order_significant() {
    let a = g(b"alpha");
    let z = g(b"omega");

    let mut b1 = TreeBuilder::new();
    b1.push_leaf(&a);
    b1.push_leaf(&z);
    let r1 = b1.build(2 * BLOCK as u64).root;

    let mut b2 = TreeBuilder::new();
    b2.push_leaf(&z);
    b2.push_leaf(&a);
    let r2 = b2.build(2 * BLOCK as u64).root;

    assert_ne!(r1, r2, "swapping leaf order must change the root");
}

// Verifies: REQ-TB-009
#[test]
fn length_independent_of_leaf_count_flows_to_identifier() {
    let leaves = [g(b"one"), g(b"two"), g(b"three")];

    let mut b1 = TreeBuilder::new();
    for h in &leaves {
        b1.push_leaf(h);
    }
    let bt1 = b1.build(100);

    let mut b2 = TreeBuilder::new();
    for h in &leaves {
        b2.push_leaf(h);
    }
    let bt2 = b2.build(200);

    // Same leaves -> same root, regardless of the committed length.
    assert_eq!(bt1.root, bt2.root, "root depends only on the leaves");
    // Different length -> different identifier.
    assert_ne!(bt1.identifier(), bt2.identifier(), "length is committed");
    // Identifier is G(manifest(length, root)).
    assert_eq!(bt1.identifier(), identifier_from_parts(100, &bt1.root));
    assert_eq!(bt2.identifier(), identifier_from_parts(200, &bt2.root));
}

// Verifies: REQ-TB-010
#[test]
fn tree_hex_equals_to_hex_tree_root() {
    let data = fill(3 * BLOCK + 9, 2);
    let bt = build_tree(&data);
    assert_eq!(bt.tree_hex(), to_hex(&tree_root(&data)));
}
