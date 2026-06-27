//! Conformance integration tests for the `terrapin` crate.
//!
//! These tests verify the implementation against published golden vectors:
//!   * a JSON fixture (`vectors-terrapin.json`) parsed with a tiny hand-written
//!     scanner (no serde, no external crates),
//!   * boundary + spec §5.4 example vectors, and
//!   * a frozen identifier corpus snapshot (regression guard).
//!
//! The two huge zero-data vectors (137 GiB) are NEVER materialized; their
//! identifiers are checked via `identifier_from_parts(length, &tree)`.

use terrapin::{derive_counts, g, identifier, identifier_from_parts, to_hex, tree_root};

// ---------------------------------------------------------------------------
// Deterministic pseudo-random fill (xorshift64*, mirrors tests/common/mod.rs).
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn next_u8(&mut self) -> u8 {
        (self.next_u64() >> 33) as u8
    }
}

/// Deterministic pseudo-random bytes of the given length.
fn fill(len: usize, seed: u64) -> Vec<u8> {
    let mut r = Rng::new(seed);
    (0..len).map(|_| r.next_u8()).collect()
}

// ---------------------------------------------------------------------------
// Tiny tolerant JSON scanner for the controlled fixture format.
//
// The fixture is a flat array of single-level objects of the shape:
//   { "name": "...", "kind": "input"|"zeros", "input": "...",
//     "length": N, "tree": "hex", "id": "hex" }
// This is NOT a general JSON parser; it only understands this controlled file.
// ---------------------------------------------------------------------------

/// A parsed fixture record. Absent fields are `None`.
struct Record {
    name: String,
    kind: String,
    input: Option<String>,
    length: Option<u64>,
    tree: Option<String>,
    id: Option<String>,
}

/// Split the array text into per-object slices on the (un-nested) `{` .. `}`.
fn split_records(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    out.push(text[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    out
}

/// Extract a quoted string value for `"field":`.
fn extract_str(record: &str, field: &str) -> Option<String> {
    let key = format!("\"{}\"", field);
    let kpos = record.find(&key)?;
    // Find the colon after the key.
    let after_key = &record[kpos + key.len()..];
    let colon = after_key.find(':')?;
    let rest = &after_key[colon + 1..];
    // First non-space char must be the opening quote.
    let mut bytes = rest.char_indices().skip_while(|(_, c)| c.is_whitespace());
    let (qstart, q) = bytes.next()?;
    if q != '"' {
        return None;
    }
    let value_region = &rest[qstart + 1..];
    let end = value_region.find('"')?;
    Some(value_region[..end].to_string())
}

/// Extract a decimal numeric value for `"field":`.
fn extract_u64(record: &str, field: &str) -> Option<u64> {
    let key = format!("\"{}\"", field);
    let kpos = record.find(&key)?;
    let after_key = &record[kpos + key.len()..];
    let colon = after_key.find(':')?;
    let rest = &after_key[colon + 1..];
    let digits: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn parse_fixture(text: &str) -> Vec<Record> {
    split_records(text)
        .iter()
        .map(|r| Record {
            name: extract_str(r, "name").unwrap_or_default(),
            kind: extract_str(r, "kind").expect("every record has a kind"),
            input: extract_str(r, "input"),
            length: extract_u64(r, "length"),
            tree: extract_str(r, "tree"),
            id: extract_str(r, "id"),
        })
        .collect()
}

/// Convert 64 lowercase hex chars into a 32-byte array.
fn hex_to_32(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64, "tree hex must be 64 chars");
    let mut out = [0u8; 32];
    let bytes = hex.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = (bytes[2 * i] as char).to_digit(16).expect("hex digit") as u8;
        let lo = (bytes[2 * i + 1] as char)
            .to_digit(16)
            .expect("hex digit") as u8;
        *slot = (hi << 4) | lo;
    }
    out
}

/// Largest zero-data length we are willing to materialize for tree_root checks.
const MATERIALIZE_LIMIT: u64 = 8 * 1024 * 1024; // 8 MiB

// Verifies: REQ-CF-001
#[test]
fn fixture_vectors_all_verify() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/vectors-terrapin.json"
    ))
    .expect("fixture vectors-terrapin.json is readable");

    let records = parse_fixture(&text);
    let mut checked = 0usize;

    for rec in &records {
        let id = rec.id.as_ref().unwrap_or_else(|| {
            panic!("record {} missing id", rec.name);
        });
        let expected = format!("terrapin-sha256:{}", id);

        match rec.kind.as_str() {
            "input" => {
                let input = rec
                    .input
                    .as_ref()
                    .unwrap_or_else(|| panic!("input record {} missing input", rec.name));
                assert_eq!(
                    identifier(input.as_bytes()),
                    expected,
                    "identifier mismatch for {}",
                    rec.name
                );
                checked += 1;
            }
            "zeros" => {
                let length = rec
                    .length
                    .unwrap_or_else(|| panic!("zeros record {} missing length", rec.name));
                let tree_hex = rec
                    .tree
                    .as_ref()
                    .unwrap_or_else(|| panic!("zeros record {} missing tree", rec.name));
                let tree = hex_to_32(tree_hex);

                if length <= MATERIALIZE_LIMIT {
                    // Small enough to materialize: verify tree_root end-to-end.
                    let data = vec![0u8; length as usize];
                    assert_eq!(
                        to_hex(&tree_root(&data)),
                        *tree_hex,
                        "tree_root mismatch for {}",
                        rec.name
                    );
                    assert_eq!(
                        identifier(&data),
                        expected,
                        "identifier mismatch for {}",
                        rec.name
                    );
                }
                // Huge vectors: NEVER materialize, only verify from parts.
                assert_eq!(
                    identifier_from_parts(length, &tree),
                    expected,
                    "identifier_from_parts mismatch for {}",
                    rec.name
                );
                checked += 1;
            }
            other => panic!("record {} has unknown kind {:?}", rec.name, other),
        }
    }

    assert!(checked > 0, "fixture produced zero checked vectors");
    assert_eq!(checked, records.len(), "every record must be checked");
}

// Verifies: REQ-CF-002
#[test]
fn boundary_and_section_5_4_example() {
    // BLOCK-1 / BLOCK / BLOCK+1 zero vectors: materialize and check
    // tree_root + identifier against the published golden values.
    let cases: &[(u64, &str, &str)] = &[
        (
            2097151,
            "1024ef65054fcdb76a56b6fe00712dbc0007be8c65ee3902fa6c6b8c2fd7f09f",
            "dc7f0a33cf02e7a84fc380a41d396b451c96325a633a87528ebf797621befad7",
        ),
        (
            2097152,
            "67cbed9b97ddabde2863f4daefa4f57176567a7c3ccfa1560c1065f9c8af74d6",
            "6fbd6447c2d8d70a83ae159461847a1a410679900702433dd2b04d063a3b2f9b",
        ),
        (
            2097153,
            "18010af5fe70aa45e486608a97516f30410dc75c934c2486f985494990b54602",
            "5ba8049ae8f68a47acd4fad265c8a963aa82735e90f209dd79ff8d6d2188fdc5",
        ),
    ];
    for &(length, tree_hex, id_hex) in cases {
        let data = vec![0u8; length as usize];
        assert_eq!(to_hex(&tree_root(&data)), tree_hex, "tree_root @ {}", length);
        assert_eq!(
            identifier(&data),
            format!("terrapin-sha256:{}", id_hex),
            "identifier @ {}",
            length
        );
    }

    // §5.4 example: a 1,203,942-byte dataset is a single block, so:
    //   derive_counts == [1], tree_root == g(data),
    //   identifier == identifier_from_parts(length, &g(data)).
    const N: u64 = 1_203_942;
    let data = fill(N as usize, 0x5454);
    assert_eq!(derive_counts(N), vec![1], "single-block dataset has one leaf");
    let root = g(&data);
    assert_eq!(tree_root(&data), root, "single-block tree root is g(data)");
    assert_eq!(
        identifier(&data),
        identifier_from_parts(N, &root),
        "identifier equals identifier_from_parts(len, g(data))"
    );
}

// Verifies: REQ-CF-004
#[test]
fn frozen_identifier_corpus_snapshot() {
    // Hardcoded (input, expected identifier) table. The empty and "hello world"
    // anchors are spec golden vectors; the fill() vectors are pinned regression
    // guards. Any change to the identifier algorithm trips this snapshot.
    let corpus: &[(&[u8], &str)] = &[
        (
            b"",
            "terrapin-sha256:f4b8abc1cfd6ffec75b4070be5440706286b3a7af937ef5d020ca2c0c1210458",
        ),
        (
            b"hello world",
            "terrapin-sha256:7bc0163f32e5f6082308ae0dff3dc7c9b0488e5aa652d9de01418df5ec800c8c",
        ),
    ];
    for (input, expected) in corpus {
        assert_eq!(identifier(input), *expected, "frozen corpus mismatch");
    }

    // Pinned fill() vectors (computed once, then frozen).
    let fill_corpus: &[(usize, u64, &str)] = &[
        (
            100,
            1,
            "terrapin-sha256:b1fa1822d0de1488766f9ee2398060409dee8524b9fad43868faf4c91b627fd5",
        ),
        (
            4096,
            7,
            "terrapin-sha256:7aa6d1f7786d024c53e286a5b494ff269dc76e376fee03fcfb05bbc251e4101f",
        ),
        (
            3_000_000,
            42,
            "terrapin-sha256:85711c8b0af5e213a812a073ebf0dcea55599ef145085de32b588b0bc271c503",
        ),
    ];
    for &(len, seed, expected) in fill_corpus {
        let data = fill(len, seed);
        assert_eq!(
            identifier(&data),
            expected,
            "frozen fill corpus mismatch (len={}, seed={})",
            len,
            seed
        );
    }
}
