//! Integration tests for Terrapin's canonical primitives: the GitOID-SHA256
//! primitive `g`, hex encoding, the canonical root manifest and its parser, the
//! recursive `tree_root`, and the identifier `terrapin-sha256:<hex>`.
//!
//! Spec: `terrapin/docs/spec.md` sections 3-5. Each `#[test]` carries a single
//! `// Verifies: REQ-...` tag immediately above its attribute for the
//! traceability gate. Tests use only the public API plus the `fill` helper.

mod common;
use common::*;

use terrapin::{
    g, identifier, identifier_from_parts, manifest_bytes, parse_manifest, to_hex, tree_root, BLOCK,
    FANOUT,
};

// Known anchor constants (see spec section 3.0 / 5.3).
const G_EMPTY: &str = "473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813";
const ID_EMPTY: &str = "terrapin-sha256:f4b8abc1cfd6ffec75b4070be5440706286b3a7af937ef5d020ca2c0c1210458";
const ID_HELLO: &str = "terrapin-sha256:7bc0163f32e5f6082308ae0dff3dc7c9b0488e5aa652d9de01418df5ec800c8c";

// A valid 64-lowercase-hex tree value used to assemble manifests for parsing.
// (= g("hello world"), but here it is just an arbitrary canonical tree field.)
const TREE: &str = "fee53a18d32820613c0527aa79be5cb30173c823a9b448fa4817767cc84c6f03";

// Independent in-test reimplementation of the spec recursion (section 4.3),
// used as an oracle for tree_root.
fn spec_tree_root(data: &[u8]) -> [u8; 32] {
    if data.len() <= BLOCK {
        return g(data);
    }
    let mut hash_file = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let end = (i + BLOCK).min(data.len());
        hash_file.extend_from_slice(&g(&data[i..end]));
        i = end;
    }
    spec_tree_root(&hash_file)
}

// ---------------------------------------------------------------------------
// Primitive g
// ---------------------------------------------------------------------------

// Verifies: REQ-G-002
#[test]
fn g_equals_blob_framed_sha256() {
    // g("") anchors git-compatibility: it is the git empty-blob object hash.
    assert_eq!(to_hex(&g(b"")), G_EMPTY);
    // Regression pin for a non-empty input (= sha256("blob 11\0hello world")).
    assert_eq!(
        to_hex(&g(b"hello world")),
        "fee53a18d32820613c0527aa79be5cb30173c823a9b448fa4817767cc84c6f03"
    );
}

// Verifies: REQ-G-003
#[test]
fn g_binds_input_length() {
    // Two inputs that differ only in length must produce different digests,
    // because the "blob <len>\0" framing binds the length into the hash.
    let one = to_hex(&g(&[0u8; 1]));
    let two = to_hex(&g(&[0u8; 2]));
    assert_eq!(
        one,
        "449e9b795420cd16fe60ad5298cf680f15a7cd2ac9b44adaf7ed3edc0d08dd78"
    );
    assert_eq!(
        two,
        "17ca6bc4a3f2f3ad91631815be503e333eb99f90f4f565e6f660e8534fa3d4f2"
    );
    assert_ne!(one, two);
}

// Verifies: REQ-G-004
#[test]
fn g_is_32_bytes_and_deterministic() {
    let data = fill(1000, 7);
    let a = g(&data);
    let b = g(&data);
    assert_eq!(a.len(), 32);
    assert_eq!(a, b);
    assert_eq!(g(b"").len(), 32);
}

// Verifies: REQ-G-005
#[test]
fn g_correct_across_block_boundary_sizes() {
    assert_eq!(
        to_hex(&g(&vec![0u8; BLOCK - 1])),
        "1024ef65054fcdb76a56b6fe00712dbc0007be8c65ee3902fa6c6b8c2fd7f09f"
    );
    assert_eq!(
        to_hex(&g(&vec![0u8; BLOCK])),
        "67cbed9b97ddabde2863f4daefa4f57176567a7c3ccfa1560c1065f9c8af74d6"
    );
    assert_eq!(
        to_hex(&g(&vec![0u8; BLOCK + 1])),
        "06af872af29b281ecd07e3a2fb76e1b6e73c233228b7c09e3c3937d188a556de"
    );
}

// Verifies: REQ-G-006
#[test]
fn g_avalanche_on_single_bit_flip() {
    for seed in 0..8u64 {
        let data = fill(512, seed + 100);
        let base = g(&data);
        let mut flipped = data.clone();
        let idx = (seed as usize) % flipped.len();
        flipped[idx] ^= 1; // flip the lowest bit of one byte
        assert_ne!(base, g(&flipped), "seed {} produced no change", seed);
    }
}

// ---------------------------------------------------------------------------
// Hex helper
// ---------------------------------------------------------------------------

// Verifies: REQ-HEX-001
#[test]
fn to_hex_lowercase_zero_padded_and_empty() {
    assert_eq!(to_hex(&[]), "");
    assert_eq!(to_hex(&[0x05]), "05");
    assert_eq!(to_hex(&[0xab, 0xcd, 0xef]), "abcdef");
    assert_eq!(to_hex(&[0u8; 32]), "0".repeat(64));
    assert_eq!(to_hex(&[0xff]), "ff");
}

// ---------------------------------------------------------------------------
// Manifest encoding
// ---------------------------------------------------------------------------

// Verifies: REQ-MAN-001
#[test]
fn manifest_bytes_exact_shape() {
    let m = manifest_bytes(11, TREE);
    let s = std::str::from_utf8(&m).unwrap();
    assert_eq!(
        s,
        format!("terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: {}\n", TREE)
    );
    assert!(s.ends_with('\n'));
    // Exactly four LF-terminated lines.
    assert_eq!(s.matches('\n').count(), 4);
    let lines: Vec<&str> = s.split('\n').collect();
    assert_eq!(lines.len(), 5); // four content lines + trailing empty
    assert_eq!(lines[4], "");
    assert_eq!(lines[0], "terrapin: sha256");
}

// Verifies: REQ-MAN-002
#[test]
fn manifest_field_value_distinct_from_digest_prefix() {
    let m = manifest_bytes(0, TREE);
    let s = std::str::from_utf8(&m).unwrap();
    // The manifest field value is the bare algorithm name "sha256".
    assert!(s.contains("terrapin: sha256\n"));
    // The digest carries the longer prefix "terrapin-sha256:".
    let id = identifier(b"");
    assert!(id.starts_with("terrapin-sha256:"));
    // The two tokens are distinct: the manifest does not contain the digest form.
    assert!(!s.contains("terrapin-sha256:"));
}

// Verifies: REQ-MAN-003
#[test]
fn manifest_length_is_byte_length_and_block_size_literal() {
    // Constant sanity: 2 MiB exactly, fanout = BLOCK/32.
    assert_eq!(BLOCK, 2_097_152);
    assert_eq!(FANOUT, BLOCK / 32);

    let data = fill(1234, 3);
    let m = manifest_bytes(data.len() as u64, TREE);
    let s = std::str::from_utf8(&m).unwrap();
    assert!(s.contains("\nlength: 1234\n"));
    assert!(s.contains("\nblock_size: 2097152\n"));
}

// Verifies: REQ-MAN-004
#[test]
fn parse_manifest_accepts_canonical_and_roundtrips() {
    for &len in &[0u64, 1, 11, 2_097_152, u64::MAX] {
        let m = manifest_bytes(len, TREE);
        let (got_len, got_tree) = parse_manifest(&m).expect("canonical manifest must parse");
        assert_eq!(got_len, len);
        assert_eq!(got_tree, TREE);
    }
}

// Verifies: REQ-MAN-006
#[test]
fn parse_manifest_rejects_spacing_defects() {
    let defects: &[String] = &[
        // no space after colon
        format!("terrapin: sha256\nblock_size: 2097152\nlength:11\ntree: {}\n", TREE),
        // double space after colon
        format!("terrapin: sha256\nblock_size: 2097152\nlength:  11\ntree: {}\n", TREE),
        // tab instead of space
        format!("terrapin: sha256\nblock_size: 2097152\nlength:\t11\ntree: {}\n", TREE),
        // leading whitespace on a line
        format!(" terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: {}\n", TREE),
        // trailing whitespace before the LF
        format!("terrapin: sha256\nblock_size: 2097152\nlength: 11 \ntree: {}\n", TREE),
    ];
    for (i, d) in defects.iter().enumerate() {
        assert!(
            parse_manifest(d.as_bytes()).is_err(),
            "spacing defect {} wrongly accepted",
            i
        );
    }
}

// Verifies: REQ-MAN-007
#[test]
fn parse_manifest_rejects_value_defects() {
    let mut defects: Vec<Vec<u8>> = vec![
        // length with a sign
        format!("terrapin: sha256\nblock_size: 2097152\nlength: +11\ntree: {}\n", TREE).into_bytes(),
        // length with a leading zero
        format!("terrapin: sha256\nblock_size: 2097152\nlength: 011\ntree: {}\n", TREE).into_bytes(),
        // length with a separator
        format!("terrapin: sha256\nblock_size: 2097152\nlength: 1_1\ntree: {}\n", TREE).into_bytes(),
        // empty length
        format!("terrapin: sha256\nblock_size: 2097152\nlength: \ntree: {}\n", TREE).into_bytes(),
        // wrong block size (2,000,000 instead of 2,097,152)
        format!("terrapin: sha256\nblock_size: 2000000\nlength: 11\ntree: {}\n", TREE).into_bytes(),
        // tree wrong length (too short)
        "terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: abcd\n".to_string().into_bytes(),
        // tree uppercase hex
        format!(
            "terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: {}\n",
            TREE.to_uppercase()
        )
        .into_bytes(),
        // tree non-hex character
        format!("terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: {}\n", "z".repeat(64))
            .into_bytes(),
        // terrapin algorithm value != sha256
        format!("terrapin: sha512\nblock_size: 2097152\nlength: 11\ntree: {}\n", TREE).into_bytes(),
    ];
    // non-ASCII byte anywhere (here: clobber a tree-line byte with 0xFF).
    let mut non_ascii = manifest_bytes(11, TREE);
    let pos = non_ascii.len() - 5;
    non_ascii[pos] = 0xFF;
    defects.push(non_ascii);

    for (i, d) in defects.iter().enumerate() {
        assert!(parse_manifest(d).is_err(), "value defect {} wrongly accepted", i);
    }
}

// Verifies: REQ-MAN-008
#[test]
fn parse_manifest_rejects_never_normalizes() {
    // A manifest with one extra space must be rejected outright, not silently
    // repaired into the canonical form.
    let extra_space =
        format!("terrapin: sha256\nblock_size: 2097152\nlength:  11\ntree: {}\n", TREE);
    assert!(parse_manifest(extra_space.as_bytes()).is_err());
    // The canonical sibling still parses, proving the rejection is specific to
    // the defect and not a wholesale failure.
    assert!(parse_manifest(&manifest_bytes(11, TREE)).is_ok());
}

// Verifies: REQ-MAN-009
#[test]
fn parse_manifest_rejects_random_single_byte_mutations() {
    let valid = manifest_bytes(11, TREE);
    let (l0, t0) = parse_manifest(&valid).unwrap();
    let mut rng = Rng::new(0xC0FFEE);
    for _ in 0..3000 {
        let mut m = valid.clone();
        let idx = rng.below(m.len() as u64) as usize;
        m[idx] = rng.next_u8();
        if m == valid {
            // Mutation reproduced the exact bytes; it still parses identically.
            assert_eq!(parse_manifest(&m).unwrap(), (l0, t0.clone()));
        } else {
            // Otherwise it is never normalized back to the original meaning:
            // either rejected, or parsed to a genuinely different value.
            match parse_manifest(&m) {
                Err(_) => {}
                Ok((l, t)) => assert!(
                    (l, t.as_str()) != (l0, t0.as_str()),
                    "mutated bytes normalized back to the original (idx {})",
                    idx
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// tree_root
// ---------------------------------------------------------------------------

// Verifies: REQ-TR-003
#[test]
fn tree_root_empty_is_g_empty() {
    assert_eq!(tree_root(b""), g(b""));
    assert_eq!(to_hex(&tree_root(b"")), G_EMPTY);
}

// Verifies: REQ-TR-004
#[test]
fn tree_root_multi_block_matches_spec_recursion() {
    for (i, &len) in [2 * BLOCK, 2 * BLOCK + 1, 3 * BLOCK + 7].iter().enumerate() {
        let data = fill(len, 1000 + i as u64);
        assert_eq!(tree_root(&data), spec_tree_root(&data), "len {}", len);
    }
}

// Verifies: REQ-TR-005
#[test]
fn tree_root_single_leaf_is_bare_leaf() {
    let data = fill(4096, 42);
    assert!(data.len() <= BLOCK);
    let leaf = g(&data);
    assert_eq!(tree_root(&data), leaf);
    // It is the bare leaf, not a wrap g(g(data)).
    assert_ne!(tree_root(&data), g(&leaf));
}

// Verifies: REQ-TR-006
#[test]
fn tree_root_block_order_is_significant() {
    let a = fill(BLOCK, 11);
    let b = fill(BLOCK, 22);
    assert_ne!(a, b);
    let mut ab = a.clone();
    ab.extend_from_slice(&b);
    let mut ba = b.clone();
    ba.extend_from_slice(&a);
    assert_eq!(ab.len(), 2 * BLOCK);
    assert_ne!(tree_root(&ab), tree_root(&ba));
}

// Verifies: REQ-TR-007
#[test]
fn tree_root_avalanche_on_single_byte_change() {
    let data = fill(2 * BLOCK + 5, 77);
    let base = tree_root(&data);
    let mut tampered = data.clone();
    let idx = BLOCK + 3; // a byte in the second block
    tampered[idx] = tampered[idx].wrapping_add(1);
    assert_ne!(base, tree_root(&tampered));
}

// ---------------------------------------------------------------------------
// Identifier
// ---------------------------------------------------------------------------

// Verifies: REQ-ID-002
#[test]
fn identifier_zero_data_vectors() {
    let cases: &[(usize, &str)] = &[
        (1, "dce39f984d9c140e4ad8f4b448a2ae6ae5398ed1adbb4d07ed8bedbc5b3b4598"),
        (BLOCK - 1, "dc7f0a33cf02e7a84fc380a41d396b451c96325a633a87528ebf797621befad7"),
        (BLOCK, "6fbd6447c2d8d70a83ae159461847a1a410679900702433dd2b04d063a3b2f9b"),
        (BLOCK + 1, "5ba8049ae8f68a47acd4fad265c8a963aa82735e90f209dd79ff8d6d2188fdc5"),
    ];
    for (len, id_hex) in cases {
        let data = vec![0u8; *len];
        assert_eq!(identifier(&data), format!("terrapin-sha256:{}", id_hex), "len {}", len);
    }
}

// Verifies: REQ-ID-003
#[test]
fn identifier_equals_identifier_from_parts() {
    for (i, &len) in [0usize, 1, 4096, BLOCK, BLOCK + 1, 2 * BLOCK + 9].iter().enumerate() {
        let data = fill(len, 500 + i as u64);
        let from_data = identifier(&data);
        let from_parts = identifier_from_parts(data.len() as u64, &tree_root(&data));
        assert_eq!(from_data, from_parts, "len {}", len);
    }
}

// Verifies: REQ-ID-004
#[test]
fn identifier_prefix_and_hex_shape() {
    let id = identifier(&fill(300, 9));
    let rest = id.strip_prefix("terrapin-sha256:").expect("must carry the digest prefix");
    assert_eq!(rest.len(), 64);
    assert!(rest.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
}

// Verifies: REQ-ID-005
#[test]
fn identifier_is_manifest_gitoid_not_bare_root() {
    let data = fill(BLOCK + 1, 13);
    let id = identifier(&data);
    let root_hex = to_hex(&tree_root(&data));
    // The identifier is G(manifest); it is not the bare tree root hex...
    assert_ne!(id, root_hex);
    assert!(!id.ends_with(&root_hex));
    // ...nor a gitoid-style digest of the dataset.
    let gitoid_form = format!("gitoid:blob:sha256:{}", root_hex);
    assert_ne!(id, gitoid_form);
}

// Verifies: REQ-ID-006
#[test]
fn identifier_commits_length() {
    let tree = tree_root(&fill(4096, 1));
    let id1 = identifier_from_parts(100, &tree);
    let id2 = identifier_from_parts(200, &tree);
    assert_ne!(id1, id2);
}

// Verifies: REQ-ID-007
#[test]
fn identifier_distinct_inputs_distinct() {
    let id_empty = identifier(b"");
    let id_one = identifier(&fill(1, 1));
    let id_block = identifier(&fill(BLOCK, 2));
    assert_ne!(id_empty, id_one);
    assert_ne!(id_empty, id_block);
    assert_ne!(id_one, id_block);
}

// Verifies: REQ-ID-008
#[test]
fn identifier_regression_snapshot() {
    assert_eq!(identifier(b""), ID_EMPTY);
    assert_eq!(identifier(b"hello world"), ID_HELLO);
}
