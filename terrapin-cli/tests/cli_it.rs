//! Black-box integration tests for the `terrapin-cli` binary.
//!
//! These drive the freshly built binary via `std::process::Command` (no
//! external test crates) and use the `terrapin` library only to compute
//! expected values. `cargo test` builds the binary dependency automatically.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const BLOCK: usize = 2 * 1024 * 1024; // 2_097_152

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique, process-scoped temp path (not created on disk).
fn unique_path(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "terrapin_cli_it_{}_{}_{}",
        std::process::id(),
        n,
        label
    ));
    p
}

/// Run the built CLI with the given args and capture its output.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_terrapin-cli"))
        .args(args)
        .output()
        .expect("failed to spawn terrapin-cli")
}

/// Deterministic pseudo-random bytes via a small xorshift64.
fn xorshift_bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.push((x & 0xff) as u8);
    }
    v
}

/// Write `data` to a fresh temp file and return its path.
fn write_temp(label: &str, data: &[u8]) -> PathBuf {
    let p = unique_path(label);
    std::fs::write(&p, data).expect("write temp file");
    p
}

fn s(p: &PathBuf) -> &str {
    p.to_str().expect("path is valid utf8")
}

/// Attest `data_path` to base `base` and assert it succeeds.
fn attest_to(data_path: &PathBuf, base: &PathBuf) {
    let out = run(&["attest", s(data_path), "--out", s(base)]);
    assert!(
        out.status.success(),
        "attest failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn cleanup_base(base: &PathBuf) {
    let mut head = base.as_os_str().to_os_string();
    head.push(".head");
    let mut blocks = base.as_os_str().to_os_string();
    blocks.push(".blocks");
    let _ = std::fs::remove_file(PathBuf::from(head));
    let _ = std::fs::remove_file(PathBuf::from(blocks));
}

// Verifies: REQ-CLI-001
#[test]
fn id_prints_library_identifier() {
    let data = xorshift_bytes(5000, 1);
    let f = write_temp("id", &data);
    let out = run(&["id", s(&f)]);
    assert!(out.status.success(), "id should exit 0");
    let printed = stdout_str(&out);
    let expected = terrapin::identifier(&data);
    assert_eq!(printed.trim(), expected);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-002
#[test]
fn id_missing_file_nonzero() {
    let missing = unique_path("missing");
    let out = run(&["id", s(&missing)]);
    assert!(!out.status.success(), "id on missing file must exit non-zero");
}

// Verifies: REQ-CLI-003
#[test]
fn attest_default_writes_files_and_prints_id() {
    let data = xorshift_bytes(9000, 2);
    let f = write_temp("attest", &data);
    let out = run(&["attest", s(&f)]);
    assert!(out.status.success(), "attest must exit 0");

    let mut head = f.as_os_str().to_os_string();
    head.push(".terra.head");
    let mut blocks = f.as_os_str().to_os_string();
    blocks.push(".terra.blocks");
    let head = PathBuf::from(head);
    let blocks = PathBuf::from(blocks);
    assert!(head.exists(), "<file>.terra.head should exist");
    assert!(blocks.exists(), "<file>.terra.blocks should exist");
    assert_eq!(stdout_str(&out).trim(), terrapin::identifier(&data));

    let _ = std::fs::remove_file(&head);
    let _ = std::fs::remove_file(&blocks);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-004
#[test]
fn attest_out_base_name() {
    let data = xorshift_bytes(4000, 3);
    let f = write_temp("attest_out", &data);
    let base = unique_path("custombase");
    let out = run(&["attest", s(&f), "--out", s(&base)]);
    assert!(out.status.success(), "attest --out must exit 0");

    let mut head = base.as_os_str().to_os_string();
    head.push(".head");
    let mut blocks = base.as_os_str().to_os_string();
    blocks.push(".blocks");
    assert!(PathBuf::from(&head).exists(), "<base>.head should exist");
    assert!(PathBuf::from(&blocks).exists(), "<base>.blocks should exist");

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-005
#[test]
fn attest_identifier_equals_id() {
    let data = xorshift_bytes(7777, 4);
    let f = write_temp("eq", &data);
    let base = unique_path("eqbase");

    let id_out = run(&["id", s(&f)]);
    assert!(id_out.status.success());
    let attest_out = run(&["attest", s(&f), "--out", s(&base)]);
    assert!(attest_out.status.success());

    assert_eq!(stdout_str(&id_out).trim(), stdout_str(&attest_out).trim());

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-006
#[test]
fn validate_succeeds() {
    let data = xorshift_bytes(3 * BLOCK + 1000, 5);
    let f = write_temp("val", &data);
    let base = unique_path("valbase");
    attest_to(&f, &base);

    let out = run(&["validate", s(&f), "--tree", s(&base)]);
    assert!(out.status.success(), "validate should exit 0");
    assert!(
        stdout_str(&out).contains("successful"),
        "stdout should mention success: {}",
        stdout_str(&out)
    );

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-007
#[test]
fn validate_tamper_fails() {
    let mut data = xorshift_bytes(2 * BLOCK + 500, 6);
    let f = write_temp("tamper", &data);
    let base = unique_path("tamperbase");
    attest_to(&f, &base);

    // Flip one byte in the data file after attestation.
    data[1234] ^= 0xff;
    std::fs::write(&f, &data).unwrap();

    let out = run(&["validate", s(&f), "--tree", s(&base)]);
    assert!(!out.status.success(), "tampered validate must exit non-zero");
    assert!(
        stderr_str(&out).contains("failed"),
        "stderr should mention failure: {}",
        stderr_str(&out)
    );

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-008
#[test]
fn validate_range_and_out_of_bounds() {
    let len = 3 * BLOCK + 1000;
    let data = xorshift_bytes(len, 7);
    let f = write_temp("range", &data);
    let base = unique_path("rangebase");
    attest_to(&f, &base);

    // Valid block-straddling range.
    let start = (BLOCK - 500).to_string();
    let end = (BLOCK + 500).to_string();
    let ok = run(&[
        "validate", s(&f), "--tree", s(&base), "--start", &start, "--end", &end,
    ]);
    assert!(ok.status.success(), "valid range should exit 0: {}", stderr_str(&ok));

    // Out-of-bounds: end past length.
    let bad_end = (len + 100).to_string();
    let bad = run(&[
        "validate", s(&f), "--tree", s(&base), "--start", "0", "--end", &bad_end,
    ]);
    assert!(!bad.status.success(), "out-of-bounds range must exit non-zero");

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-009
#[test]
fn validate_missing_and_corrupt_tree() {
    let data = xorshift_bytes(6000, 8);
    let f = write_temp("missingtree", &data);

    // Missing tree base entirely.
    let missing_base = unique_path("nobase");
    let out = run(&["validate", s(&f), "--tree", s(&missing_base)]);
    assert!(!out.status.success(), "missing tree must exit non-zero");

    // Corrupt .head after a real attest.
    let base = unique_path("corruptbase");
    attest_to(&f, &base);
    let mut head = base.as_os_str().to_os_string();
    head.push(".head");
    let head = PathBuf::from(head);
    std::fs::write(&head, b"this is not a valid head\n").unwrap();
    let out2 = run(&["validate", s(&f), "--tree", s(&base)]);
    assert!(!out2.status.success(), "corrupt head must exit non-zero");

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-010
#[test]
fn cat_range_equals_slice() {
    let len = 3 * BLOCK + 1000;
    let data = xorshift_bytes(len, 9);
    let f = write_temp("cat", &data);
    let base = unique_path("catbase");
    attest_to(&f, &base);

    let start_n = BLOCK - 123;
    let end_n = 2 * BLOCK + 77;
    let start = start_n.to_string();
    let end = end_n.to_string();
    let out = run(&[
        "cat", s(&f), "--tree", s(&base), "--start", &start, "--end", &end,
    ]);
    assert!(out.status.success(), "cat should exit 0: {}", stderr_str(&out));
    assert_eq!(out.stdout, data[start_n..end_n].to_vec());

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-011
#[test]
fn cat_tamper_fails() {
    let mut data = xorshift_bytes(2 * BLOCK + 64, 10);
    let f = write_temp("cattamper", &data);
    let base = unique_path("cattamperbase");
    attest_to(&f, &base);

    data[42] ^= 0xff;
    std::fs::write(&f, &data).unwrap();

    let out = run(&["cat", s(&f), "--tree", s(&base)]);
    assert!(!out.status.success(), "cat on tamper must exit non-zero");

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-012
#[test]
fn bad_arguments_usage_error() {
    // Unknown subcommand.
    let unknown = run(&["frobnicate"]);
    assert!(!unknown.status.success(), "unknown subcommand must exit non-zero");

    // Missing required --tree for validate.
    let data = xorshift_bytes(100, 11);
    let f = write_temp("badargs", &data);
    let missing_tree = run(&["validate", s(&f)]);
    assert!(
        !missing_tree.status.success(),
        "missing required --tree must exit non-zero"
    );

    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-013
#[test]
fn help_renders_exit_zero() {
    let top = run(&["--help"]);
    assert!(top.status.success(), "--help must exit 0");
    assert!(
        stdout_str(&top).contains("terrapin") || stdout_str(&top).contains("USAGE"),
        "--help should print usage"
    );

    let sub = run(&["validate", "--help"]);
    assert!(sub.status.success(), "validate --help must exit 0");
}

// Verifies: REQ-CLI-014
#[test]
fn cross_process_attest_then_validate_and_cat() {
    let data = xorshift_bytes(2 * BLOCK + 999, 12);
    let f = write_temp("xproc", &data);
    let base = unique_path("xprocbase");

    // Process 1: attest.
    attest_to(&f, &base);

    // Process 2: validate (separate invocation, no shared memory).
    let v = run(&["validate", s(&f), "--tree", s(&base)]);
    assert!(v.status.success(), "cross-process validate should succeed: {}", stderr_str(&v));

    // Process 3: cat whole file.
    let c = run(&["cat", s(&f), "--tree", s(&base)]);
    assert!(c.status.success(), "cross-process cat should succeed: {}", stderr_str(&c));
    assert_eq!(c.stdout, data);

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}

// Verifies: REQ-CLI-015
#[test]
fn validate_enforces_trusted_identifier() {
    let data = xorshift_bytes(5 * 1000, 13);
    let f = write_temp("trusted", &data);
    let base = unique_path("trustedbase");
    attest_to(&f, &base);

    let correct = terrapin::identifier(&data);
    let ok = run(&[
        "validate", s(&f), "--tree", s(&base), "--identifier", &correct,
    ]);
    assert!(ok.status.success(), "correct identifier should validate: {}", stderr_str(&ok));

    let wrong = format!("terrapin-sha256:{}", "0".repeat(64));
    let bad = run(&[
        "validate", s(&f), "--tree", s(&base), "--identifier", &wrong,
    ]);
    assert!(!bad.status.success(), "wrong trusted identifier must exit non-zero");

    cleanup_base(&base);
    let _ = std::fs::remove_file(&f);
}
