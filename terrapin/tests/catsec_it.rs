//! Integration tests for the `cat` (validate + stream) model and the
//! security / adversarial properties (spec sections 5, 6, 7).
//!
//! Each test carries exactly one `// Verifies: REQ-...` comment placed
//! immediately above its `#[test]` attribute for the traceability gate.

mod common;
use common::*;

use std::io::{self, Write};
use std::path::Path;

use terrapin::{
    derive_counts, g, identifier, identifier_from_parts, to_hex, tree_root, PersistedTree, BLOCK,
    FANOUT,
};

// ---------------------------------------------------------------------------
// Local helpers (only public API + common helpers).
// ---------------------------------------------------------------------------

/// Write `data` to a temp file, build+persist its tree, and read it back.
/// The returned `TmpPath`s must outlive the `PersistedTree` (they own the
/// data file and the `.head`/`.blocks` artifacts and clean up on drop).
fn setup(data: &[u8]) -> (TmpPath, TmpPath, PersistedTree) {
    let dp = TmpPath::new("data");
    std::fs::write(dp.path(), data).unwrap();

    let bt = build_tree(data);
    let tp = TmpPath::new("tree");
    PersistedTree::write(tp.path(), &bt).unwrap();
    let pt = PersistedTree::read(tp.path()).unwrap();

    (dp, tp, pt)
}

/// cat the byte range `[s, e)` and assert the streamed output is exactly the
/// corresponding data slice.
fn cat_eq(pt: &PersistedTree, dp: &Path, data: &[u8], s: u64, e: u64) {
    let mut out = Vec::new();
    pt.validate(dp, Some(s), Some(e), Some(&mut out)).unwrap();
    assert_eq!(out, &data[s as usize..e as usize], "cat {}..{}", s, e);
}

/// Replace the value of a single `key: value` line in a `.head` file.
fn rewrite_head_line(head_path: &Path, key: &str, new_val: &str) {
    let text = std::fs::read_to_string(head_path).unwrap();
    let mut out = String::new();
    for line in text.lines() {
        if let Some((k, _)) = line.split_once(": ") {
            if k == key {
                out.push_str(key);
                out.push_str(": ");
                out.push_str(new_val);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    std::fs::write(head_path, out).unwrap();
}

/// A writer that accepts at most `limit` bytes total, then errors.
struct FailWriter {
    limit: usize,
    written: usize,
}
impl Write for FailWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written >= self.limit {
            return Err(io::Error::new(io::ErrorKind::Other, "writer full"));
        }
        let n = buf.len().min(self.limit - self.written);
        self.written += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// cat (validate + stream) — REQ-CAT-001 .. REQ-CAT-007
// ---------------------------------------------------------------------------

// Verifies: REQ-CAT-001
#[test]
fn cat_whole_file_equals_bytes() {
    let data = fill(2 * BLOCK + 333, 1);
    let (dp, _tp, pt) = setup(&data);

    let mut out = Vec::new();
    pt.validate(dp.path(), None, None, Some(&mut out)).unwrap();
    assert_eq!(out, data);
}

// Verifies: REQ-CAT-002
#[test]
fn cat_multi_block_range_equals_slice() {
    let data = fill(2 * BLOCK + 333, 2);
    let (dp, _tp, pt) = setup(&data);

    let s = (BLOCK / 2) as u64;
    let e = 2 * BLOCK as u64 + 10;
    cat_eq(&pt, dp.path(), &data, s, e);
}

// Verifies: REQ-CAT-003
#[test]
fn cat_range_variants_including_empty() {
    let data = fill(2 * BLOCK + 333, 3);
    let (dp, _tp, pt) = setup(&data);

    // Within a single block.
    cat_eq(&pt, dp.path(), &data, 100, 250);
    // Straddling a block boundary.
    cat_eq(&pt, dp.path(), &data, BLOCK as u64 - 30, BLOCK as u64 + 30);
    // The last, partial block to end of file.
    cat_eq(&pt, dp.path(), &data, 2 * BLOCK as u64, data.len() as u64);
    // Empty range [k, k): no output.
    let mut out = Vec::new();
    pt.validate(
        dp.path(),
        Some(BLOCK as u64),
        Some(BLOCK as u64),
        Some(&mut out),
    )
    .unwrap();
    assert!(out.is_empty(), "empty range must emit no bytes");
}

// Verifies: REQ-CAT-004
#[test]
fn cat_slice_math_has_no_off_by_one() {
    let data = fill(2 * BLOCK + 333, 4);
    let (dp, _tp, pt) = setup(&data);

    // Odd offsets, especially right at block boundaries.
    cat_eq(&pt, dp.path(), &data, 1, 7);
    cat_eq(&pt, dp.path(), &data, BLOCK as u64 - 1, BLOCK as u64 + 1);
    cat_eq(&pt, dp.path(), &data, BLOCK as u64, 2 * BLOCK as u64);
    cat_eq(&pt, dp.path(), &data, 2 * BLOCK as u64 - 5, 2 * BLOCK as u64 + 9);
    cat_eq(&pt, dp.path(), &data, 0, data.len() as u64);
}

// Verifies: REQ-CAT-005
#[test]
fn cat_output_is_binary_safe() {
    // Data containing NUL, LF, CR, and 0xFF bytes interleaved with noise.
    let mut data = vec![0x00, 0x0a, 0x0d, 0xff, 0x00, 0xff, 0x0d, 0x0a];
    data.extend_from_slice(&fill(500, 5));
    data.extend_from_slice(&[0x00, 0xff, 0x0a, 0x0d]);
    let (dp, _tp, pt) = setup(&data);

    let mut out = Vec::new();
    pt.validate(dp.path(), None, None, Some(&mut out)).unwrap();
    assert_eq!(out, data, "cat must be byte-exact for binary data");
}

// Verifies: REQ-CAT-006
#[test]
fn cat_emits_no_bytes_past_first_failure() {
    let data = fill(2 * BLOCK + 100, 6);
    let (dp, _tp, pt) = setup(&data);

    // Tamper a byte inside block 1 (the second block) on disk only.
    let mut tampered = data.clone();
    tampered[BLOCK + 5] ^= 0xff;
    std::fs::write(dp.path(), &tampered).unwrap();

    // Range spans an earlier good block and the tampered block.
    let s = BLOCK as u64 - 10;
    let e = BLOCK as u64 + 50;
    let mut out = Vec::new();
    let err = pt
        .validate(dp.path(), Some(s), Some(e), Some(&mut out))
        .unwrap_err();
    assert!(err.contains("validation failed"), "got: {}", err);

    // Only the verified earlier block was emitted: exactly data[s..BLOCK].
    assert_eq!(out, &data[s as usize..BLOCK]);
    // It is a strict prefix of the expected slice...
    let expected = &data[s as usize..e as usize];
    assert!(expected.starts_with(&out[..]));
    assert!(out.len() < expected.len());
    // ...and contains no bytes from at/after the tampered block boundary.
    assert_eq!(out.len() as u64, BLOCK as u64 - s);
}

// Verifies: REQ-CAT-007
#[test]
fn cat_surfaces_writer_errors() {
    let data = fill(5000, 7);
    let (dp, _tp, pt) = setup(&data);

    // Writer accepts 100 bytes then fails; the block is larger than that.
    let mut w = FailWriter {
        limit: 100,
        written: 0,
    };
    let err = pt
        .validate(dp.path(), None, None, Some(&mut w))
        .unwrap_err();
    assert!(err.contains("write output"), "got: {}", err);
}

// ---------------------------------------------------------------------------
// Security / adversarial — REQ-SEC-001 .. REQ-SEC-007
// ---------------------------------------------------------------------------

// Verifies: REQ-SEC-001
#[test]
fn length_reinterpretation_is_prevented() {
    let data = fill(1500, 8);
    let root = tree_root(&data);

    // Same tree root, different declared length -> different identifier.
    assert_ne!(
        identifier_from_parts(100, &root),
        identifier_from_parts(200, &root)
    );

    // A tree commits its length: it cannot validate a different-length dataset.
    let (_dp, _tp, pt) = setup(&data); // pt.length == 1500
    let dp2 = TmpPath::new("data2");
    std::fs::write(dp2.path(), &fill(2000, 8)).unwrap();
    let err = pt.validate(dp2.path(), None, None, None).unwrap_err();
    assert!(err.contains("length"), "got: {}", err);
}

// Verifies: REQ-SEC-002
#[test]
fn forged_tree_fails_against_trusted_identifier() {
    let data_a = fill(1234, 100);
    let data_b = fill(1234, 200);
    let id_a = identifier(&data_a);

    // A tree built for B has its own valid head, but a different identifier.
    let (_dp_b, tp_b, pt_b) = setup(&data_b);
    assert!(pt_b.check_against(&id_a).is_err(), "B must not match A's id");
    pt_b.check_against(&pt_b.identifier).unwrap(); // sanity: matches itself

    // Tampering B's head identifier to A's value makes G(manifest) != identifier.
    rewrite_head_line(&tp_b.with_ext("head"), "identifier", &id_a);
    let pt_forged = PersistedTree::read(tp_b.path()).unwrap();
    let err = pt_forged
        .validate(_dp_b.path(), None, None, None)
        .unwrap_err();
    assert!(
        err.contains("identifier does not match manifest"),
        "got: {}",
        err
    );
}

// Verifies: REQ-SEC-003
#[test]
fn bare_tree_root_is_not_the_identifier() {
    let data = fill(1000, 9);
    let root = tree_root(&data);
    let id = identifier(&data);

    // The identifier is G(manifest), never the bare root nor a gitoid-form hex.
    assert_ne!(id, to_hex(&root));
    assert_ne!(id, format!("terrapin-sha256:{}", to_hex(&root)));
}

// Verifies: REQ-SEC-004
#[test]
fn truncation_at_the_hasher_is_detected() {
    let data = fill(5000, 10);
    let (dp, _tp, pt) = setup(&data);

    // Truncate the data file by a few bytes.
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(dp.path())
        .unwrap();
    f.set_len(data.len() as u64 - 17).unwrap();

    let err = pt.validate(dp.path(), None, None, None).unwrap_err();
    assert!(err.contains("length"), "got: {}", err);
}

// Verifies: REQ-SEC-005
#[test]
fn swapped_or_duplicated_blocks_are_detected() {
    let data = fill(2 * BLOCK + 777, 11);
    let (dp, _tp, pt) = setup(&data);

    // Duplicate block 0 over block 1 (positions are committed).
    let mut tampered = data.clone();
    let block0 = data[0..BLOCK].to_vec();
    tampered[BLOCK..2 * BLOCK].copy_from_slice(&block0);
    std::fs::write(dp.path(), &tampered).unwrap();

    let err = pt.validate(dp.path(), None, None, None).unwrap_err();
    assert!(err.contains("validation failed"), "got: {}", err);
}

// Verifies: REQ-SEC-007
#[test]
fn non_canonical_manifest_rejected_at_validation_boundary() {
    let data = fill(4096, 12);

    // (a) A non-canonical (wrong-length) tree value is rejected at validate().
    {
        let (dp, tp, _pt) = setup(&data);
        rewrite_head_line(&tp.with_ext("head"), "tree", "abcdef"); // not 64 hex
        let pt = PersistedTree::read(tp.path()).unwrap();
        let err = pt.validate(dp.path(), None, None, None).unwrap_err();
        assert!(err.contains("64 hex"), "got: {}", err);
    }

    // (b) validate recomputes G(manifest) canonically: a mismatched (but
    // well-formed) identifier fails the binding check.
    {
        let (dp, tp, _pt) = setup(&data);
        let other_id = identifier(&fill(4096, 99));
        rewrite_head_line(&tp.with_ext("head"), "identifier", &other_id);
        let pt = PersistedTree::read(tp.path()).unwrap();
        let err = pt.validate(dp.path(), None, None, None).unwrap_err();
        assert!(
            err.contains("identifier does not match manifest"),
            "got: {}",
            err
        );
    }
}

// ---------------------------------------------------------------------------
// Spec worked examples — REQ-WE-002, REQ-WE-003
// ---------------------------------------------------------------------------

// Verifies: REQ-WE-002
#[test]
fn we_single_block_example_tree_is_g_of_dataset() {
    // §5.4: a 1,203,942-byte dataset is one block (< 2 MiB).
    let len: u64 = 1_203_942;
    assert_eq!(derive_counts(len), vec![1]);

    let data = fill(len as usize, 13);
    assert_eq!(tree_root(&data), g(&data));
    assert_eq!(identifier(&data), identifier_from_parts(len, &g(&data)));
}

// Verifies: REQ-WE-003
#[test]
fn we_path_index_arithmetic() {
    // §6.1: locating leaf 250,000,000 in the layer-1 hash file.
    let leaf: usize = 250_000_000;
    assert_eq!(leaf / FANOUT, 3814);
    assert_eq!(leaf - 3814 * FANOUT, 45_696);
}
