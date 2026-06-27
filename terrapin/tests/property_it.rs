//! Property-based integration tests for the `terrapin` crate (spec sections
//! 2.1, 4.3, 6, 7). No external crates: randomized iterations are driven by the
//! deterministic `Rng` helper in `tests/common`, looping over fixed seeds so any
//! failure reproduces exactly.
//!
//! Each test is tagged with exactly one `// Verifies: REQ-...` comment placed
//! immediately above its test attribute for the traceability gate.

mod common;
use common::*;

use std::io::Cursor;

use terrapin::{
    build_from_reader, g, identifier, identifier_from_reader, PersistedTree, TreeBuilder, BLOCK,
    FANOUT,
};

/// Pick a pseudo-random dataset length: mostly small (fast), occasionally a
/// multi-block size that exercises the recursion above a single leaf.
fn rand_len(r: &mut Rng) -> usize {
    if r.below(8) == 0 {
        // Rare multi-block case: BLOCK..=2*BLOCK + a little.
        (BLOCK as u64 + r.below(BLOCK as u64 + 257)) as usize
    } else {
        // Common small case: 0..=4096.
        r.below(4097) as usize
    }
}

// Verifies: REQ-PR-001
#[tokio::test]
async fn streaming_id_equals_in_memory_id() {
    for seed in 0..40u64 {
        let mut r = Rng::new(seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xD1B5);
        let len = rand_len(&mut r);
        let data = fill(len, seed ^ 0xABCD);

        let from_reader = identifier_from_reader(Cursor::new(data.clone()))
            .await
            .expect("identifier_from_reader");
        let in_memory = identifier(&data);
        let from_builder = build_tree(&data).identifier();

        assert_eq!(from_reader, in_memory, "seed {} len {}", seed, len);
        assert_eq!(from_builder, in_memory, "seed {} len {}", seed, len);
    }
}

// Verifies: REQ-PR-002
#[tokio::test]
async fn random_chunking_does_not_change_identifier() {
    for seed in 0..40u64 {
        let mut r = Rng::new(seed.wrapping_mul(0x100000001B3) ^ 0x55AA);
        let len = rand_len(&mut r);
        let data = fill(len, seed ^ 0x1234);

        // Random chunk size in [1, len+1]; Choppy delivers <= chunk bytes/read.
        let chunk = 1 + r.below(len as u64 + 1) as usize;
        let reader = Choppy::new(data.clone(), chunk);

        let chunked = identifier_from_reader(reader).await.expect("chunked id");
        assert_eq!(
            chunked,
            identifier(&data),
            "seed {} len {} chunk {}",
            seed,
            len,
            chunk
        );
    }
}

// Verifies: REQ-PR-003
#[tokio::test]
async fn random_valid_range_validates_and_cat_equals_slice() {
    for seed in 0..30u64 {
        let mut r = Rng::new(seed.wrapping_mul(0xC2B2AE3D27D4EB4F) ^ 0x77);
        // At least one block of data.
        let len = (1 + r.below(2 * BLOCK as u64 + 257)) as usize;
        let data = fill(len, seed ^ 0xBEEF);

        let tree = build_from_reader(Cursor::new(data.clone()))
            .await
            .expect("build_from_reader");

        let base = TmpPath::new("pr003-tree");
        PersistedTree::write(base.path(), &tree).expect("write tree");
        let pt = PersistedTree::read(base.path()).expect("read tree");

        let data_tmp = TmpPath::new("pr003-data");
        std::fs::write(data_tmp.path(), &data).expect("write data");

        // Random valid [start, end): 0 <= start <= end <= len.
        let a = r.below(len as u64 + 1);
        let b = r.below(len as u64 + 1);
        let (start, end) = if a <= b { (a, b) } else { (b, a) };

        // Plain validation of the range succeeds.
        pt.validate(data_tmp.path(), Some(start), Some(end), None)
            .unwrap_or_else(|e| panic!("validate seed {} {}..{}: {}", seed, start, end, e));

        // `cat` the range via a Vec writer: bytes equal the data slice.
        let mut out: Vec<u8> = Vec::new();
        pt.validate(data_tmp.path(), Some(start), Some(end), Some(&mut out))
            .unwrap_or_else(|e| panic!("cat seed {} {}..{}: {}", seed, start, end, e));
        assert_eq!(
            out,
            &data[start as usize..end as usize],
            "seed {} range {}..{}",
            seed,
            start,
            end
        );
    }
}

// Verifies: REQ-PR-004
#[test]
fn single_byte_flip_changes_identifier() {
    for seed in 0..40u64 {
        let mut r = Rng::new(seed.wrapping_mul(0x2545F4914F6CDD1D) ^ 0x3C3C);
        // Non-empty so there is a byte to flip.
        let len = 1 + rand_len(&mut r);
        let data = fill(len, seed ^ 0x0F0F);
        let original = identifier(&data);

        let pos = r.below(len as u64) as usize;
        let mut tampered = data.clone();
        // XOR a non-zero mask so the byte definitely changes.
        let mask = (r.next_u8() | 1) as u8;
        tampered[pos] ^= mask;

        assert_ne!(
            identifier(&tampered),
            original,
            "seed {} len {} flip@{}",
            seed,
            len,
            pos
        );
    }
}

// Verifies: REQ-PR-005
#[test]
fn write_read_validate_roundtrip() {
    for seed in 0..30u64 {
        let mut r = Rng::new(seed.wrapping_mul(0xFF51AFD7ED558CCD) ^ 0x9119);
        let len = rand_len(&mut r);
        let data = fill(len, seed ^ 0x7A7A);

        let tree = build_tree(&data);

        let base = TmpPath::new("pr005-tree");
        PersistedTree::write(base.path(), &tree).expect("write tree");
        let pt = PersistedTree::read(base.path()).expect("read tree");

        // Identifier roundtrips through persistence.
        assert_eq!(pt.identifier, identifier(&data), "seed {} len {}", seed, len);

        let data_tmp = TmpPath::new("pr005-data");
        std::fs::write(data_tmp.path(), &data).expect("write data");

        // Whole-dataset validation succeeds.
        pt.validate(data_tmp.path(), None, None, None)
            .unwrap_or_else(|e| panic!("validate seed {} len {}: {}", seed, len, e));
    }
}

// Verifies: REQ-PR-006
#[test]
fn random_leaf_streams_satisfy_layer_relation() {
    for seed in 0..30u64 {
        let mut r = Rng::new(seed.wrapping_mul(0x9E3779B185EBCA87) ^ 0x2222);

        // Vary K, occasionally exceeding FANOUT to force a multi-layer tree.
        // Synthetic leaves are cheap (no dataset materialized).
        let k = match r.below(10) {
            0 => 1usize,
            1 => FANOUT + 1 + r.below(8) as usize, // just over one full group
            2 => FANOUT,                           // exact single full group
            _ => 1 + r.below(2000) as usize,       // small/medium
        };

        let mut b = TreeBuilder::new();
        // Distinct leaves: g over a unique counter keeps every leaf different.
        for i in 0..k {
            b.push_leaf(&g(&(i as u64).to_le_bytes()));
        }
        let bt = b.build(k as u64 * BLOCK as u64);

        // Layer relation: each upper layer is the concatenation of g over the
        // BLOCK-byte (FANOUT-hash) groups of the layer below it.
        for l in 0..bt.layers.len() - 1 {
            let expected: Vec<u8> = bt.layers[l].chunks(BLOCK).flat_map(g).collect();
            assert_eq!(
                bt.layers[l + 1],
                expected,
                "seed {} k {} layer {}->{}",
                seed,
                k,
                l,
                l + 1
            );
        }

        // Root: bare leaf when there is a single leaf, else g over the top layer.
        let top = bt.layers.last().expect("at least one layer");
        if top.len() == 32 {
            assert_eq!(&bt.root[..], &top[..], "seed {} k {} bare leaf", seed, k);
        } else {
            assert_eq!(bt.root, g(top), "seed {} k {} root=g(top)", seed, k);
        }
    }
}
