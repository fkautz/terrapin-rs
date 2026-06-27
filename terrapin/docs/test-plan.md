---
title: Terrapin Test Plan
---

# Terrapin Test Plan

A comprehensive catalog of tests for the Terrapin v0.3 implementation (crate
`terrapin` + binary `terrapin-cli`), traced to `docs/spec.md`. This is a
planning document — none of the unchecked items are implemented yet.

Legend:
- `[x]` already implemented · `[ ]` to write
- Level: **U** unit · **I** integration (`terrapin/tests/`, `terrapin-cli/tests/`) · **P** property (proptest) · **S** slow / `#[ignore]` (huge data) · **C** conformance (golden vectors)
- Spec refs are section numbers in `docs/spec.md`.

Key invariants the suite must pin (callouts, expanded per-section below):
- The streaming + parallel path is **byte-identical** to the reference `tree_root`/`identifier` for every input.
- A **single leaf** (dataset ≤ BLOCK, incl. empty) yields the **bare leaf**, never `g(leaf)`.
- Exact **FANOUT-power** boundaries get a **single wrap**, never wrap-twice.
- The identifier is `G(manifest)` — commits algorithm, block size, **length**, tree root — never the bare tree root (§5.3, §8).
- Slice validation reads only the path data block(s) + one hash-file block per layer; it does **not** read the rest of the dataset (§2.3, §6).
- Non-canonical manifests are **rejected, not normalized** (§5.2).

---

## 0. Test infrastructure to add

- [ ] **INF-1** (U/I helper) `ZeroReader { remaining: u64 }: Read` that streams N zero bytes without allocating — enables multi-layer streaming tests against the algebraic zero-oracle.
- [ ] **INF-2** (U helper) Promote `Choppy` (short-read reader, in `stream.rs` tests) to a shared test util; add a `Flaky` reader that injects `ErrorKind::Interrupted` and a hard `io::Error`.
- [ ] **INF-3** (U helper) Deterministic data generator `fill(len, seed) -> Vec<u8>` so cases are reproducible without `/dev/urandom`.
- [ ] **INF-4** (helper) Synthetic-leaf builder: push N distinct 32-byte leaves into `TreeBuilder` directly (no data) to exercise **multi-layer** structure cheaply (a real ≥2-layer tree needs ≥128 GiB of data).
- [ ] **INF-5** dev-deps: `proptest` (property tests), `assert_cmd` + `predicates` (CLI), `tempfile` (robust temp dirs, replacing the manual pid-based scheme in `tree.rs` tests).
- [ ] **INF-6** (C) Add `terrapin/tests/vectors-terrapin.json` (the LLIFS conformance oracle referenced in code) + a loader that checks every vector; share the same file with `terrapin-go`.
- [ ] **INF-7** Shared `tmp_tree()` / `tmp_data()` fixtures with guaranteed cleanup (Drop guard), so failing tests don't leak files.

---

## 1. Existing coverage (inventory)

- [x] `manifest::g_empty_is_git_empty_blob` — `g("")` == git empty blob.
- [x] `manifest::explicit_vectors` — identifier of `""` and `"hello world"`.
- [x] `manifest::zero_data_vectors` — tree + id for 1, BLOCK-1, BLOCK, BLOCK+1 zero bytes.
- [x] `manifest::recursion_boundary_vectors` — 65536·BLOCK and +1 (algebraic).
- [x] `manifest::manifest_accept_reject` — 1 accept + 8 reject cases.
- [x] `builder::matches_reference_small_sizes` — builder vs `tree_root`, several sizes.
- [x] `builder::fanout_boundaries` — synthetic leaves at {1, 2, FANOUT-1, FANOUT, FANOUT+1}.
- [x] `stream::matches_in_memory_identifier` — reader vs in-memory, key sizes.
- [x] `stream::matches_under_short_reads` — Choppy reader, several chunk sizes.
- [x] `tree::roundtrip_validate_and_ranges` — write/read/validate/cat/tamper on a 3-block file.
- [x] `tree::empty_dataset` — empty file roundtrip + validate.
- [x] `tree::corrupt_head_rejected` — flipped root hex in head fails validate.

---

## 2. Primitive `g` (GitOID SHA-256) — §3.0

- [x] **G-1** (U) `g("")` == `473a0f4c…721813`.
- [ ] **G-2** (U) `g("hello world")` == independently computed `sha256("blob 11\0hello world")` (assemble the framed bytes and hash with a reference `sha2`/known constant).
- [ ] **G-3** (U) Length framing: `g(data) != sha256(data)` (raw) for a few inputs — proves the `blob <len>\0` prefix is applied.
- [ ] **G-4** (U) Output is always 32 bytes; determinism `g(x)==g(x)`.
- [ ] **G-5** (U) `g` over exactly `BLOCK` bytes and over `BLOCK-1`/`BLOCK+1` (no off-by-one in framing/length decimal).
- [ ] **G-6** (U) Decimal length has **no leading zeros** and uses base-10 (e.g. `g` of a 10-byte and 100-byte input differ as expected; verify the framed preimage for a 0- and a 1000000-byte length).
- [ ] **G-7** (P) Avalanche: flipping any single bit of the input changes `g` (random inputs).

---

## 3. Hex helpers — `to_hex`, `hex_to_32`

- [ ] **HEX-1** (U) `to_hex` is lowercase, 2 chars/byte, preserves leading-zero nibble (`0x05 -> "05"`, all-zero 32 bytes -> 64 `0`s).
- [ ] **HEX-2** (U) `to_hex` of empty slice -> `""`.
- [ ] **HEX-3** (U) `hex_to_32` roundtrips `to_hex` for random 32-byte arrays.
- [ ] **HEX-4** (U) `hex_to_32` rejects length ≠ 64 (63, 65, 0).
- [ ] **HEX-5** (U) `hex_to_32` rejects non-hex chars (`g`, `z`, space, `:`).
- [ ] **HEX-6** (U) **Decision test**: `hex_to_32` currently accepts **uppercase** (`from_str_radix`). Spec ENC-7 mandates lowercase for the manifest. Decide & assert: either reject uppercase in the head `tree` field at `read()`, or document it as a cosmetic container field. (See **PT-READ-9**.)

---

## 4. Manifest encoding & `parse_manifest` — §5.1, §5.2 (ENC-1..9)

### 4.1 `manifest_bytes` shape
- [ ] **MAN-1** (U) Output exactly `"terrapin: sha256\nblock_size: 2097152\nlength: {n}\ntree: {hex}\n"`; ends in LF; 4 lines.
- [ ] **MAN-2** (U) The manifest **field value** is `sha256` (ENC-8), distinct from the digest **prefix** `terrapin-sha256:` (§5.3) — assert both in one test so they never get conflated.
- [ ] **MAN-3** (U) `length` field equals the **byte length**, not the block count (e.g. a 3-block file shows the real byte length).
- [ ] **MAN-4** (U) `block_size` is the literal `2097152` (not `2*1024*1024` rendering, not `2000000`).

### 4.2 `parse_manifest` accept
- [x] **MAN-5** (U) Canonical 4-field manifest parses to `(length, tree_hex)`.
- [ ] **MAN-6** (U) `length: 0` accepted (the canonical zero).
- [ ] **MAN-7** (U) `u64::MAX` length accepted and parsed exactly.
- [ ] **MAN-8** (U) Roundtrip `parse_manifest(manifest_bytes(n, tree)) == (n, tree)` for many `n`.

### 4.3 `parse_manifest` reject — exhaustive ENC matrix (each its own case)
- [x] partial: uppercase tree, missing final LF, wrong order, leading-zero length, double space, wrong block_size, short tree, extra key.
- [ ] **MAN-R1** (ENC-1) non-UTF8 / non-ASCII byte in any field.
- [ ] **MAN-R2** (ENC-2) CRLF line endings (`\r\n`).
- [ ] **MAN-R3** (ENC-2) extra trailing blank line (`…tree: …\n\n`).
- [ ] **MAN-R4** (ENC-3) no space after a colon (`terrapin:sha256`).
- [ ] **MAN-R5** (ENC-3) tab instead of space after colon.
- [ ] **MAN-R6** (ENC-3) leading whitespace on a line.
- [ ] **MAN-R7** (ENC-3) trailing whitespace before the LF on any field.
- [ ] **MAN-R8** (ENC-5) duplicate key (e.g. two `length`).
- [ ] **MAN-R9** (ENC-5) missing a required key (only 3 lines).
- [ ] **MAN-R10** (ENC-5) comment line / blank line inserted.
- [ ] **MAN-R11** (ENC-6) length with sign (`-1`, `+1`), separators (`1,000`), spaces, hex, or empty value.
- [ ] **MAN-R12** (ENC-6) `block_size` with leading zero or non-canonical decimal.
- [ ] **MAN-R13** (ENC-7) tree of length 63/65, with uppercase, or with a non-hex char.
- [ ] **MAN-R14** (ENC-8) `terrapin:` value other than `sha256` (`sha512`, `SHA256`, empty).
- [ ] **MAN-R15** (ENC-9) a 5th field after `tree` (already partly covered — keep as explicit ENC-9).
- [ ] **MAN-R16** (U) **Non-normalization**: a manifest differing only by a normalizable defect (extra space, trailing zero) is rejected — `parse_manifest` never "fixes" input.
- [ ] **MAN-R17** (P) Random byte mutations of a valid manifest are rejected unless they reproduce the exact canonical bytes.

---

## 5. Reference `tree_root` — §4.1–4.3

- [x] **TR-1** (U) Zero-data vectors {1, BLOCK-1, BLOCK, BLOCK+1}.
- [x] **TR-2** (U) Algebraic boundary {65536·BLOCK, +1}.
- [ ] **TR-3** (U) Empty -> `g("")` explicitly (base case, §4.3 empty note).
- [ ] **TR-4** (U) `2·BLOCK` and `2·BLOCK+1`, `3·BLOCK`, `3·BLOCK+7` with **non-zero** data.
- [ ] **TR-5** (U) `BLOCK·FANOUT - 1` (one short final block under a full layer) via zero-oracle.
- [ ] **TR-6** (U) Single-leaf invariant: for `len ≤ BLOCK`, `tree_root(data) == g(data)` (bare leaf), and is **not** `g(g(data))`.
- [ ] **TR-7** (U) Order sensitivity: `tree_root(a‖b) != tree_root(b‖a)` for two distinct blocks.
- [ ] **TR-8** (P) Avalanche: flipping any single byte of the dataset changes `tree_root`.

---

## 6. `identifier` / `identifier_from_parts` — §5.3, §8

- [x] **ID-1** (U) Golden `""` and `"hello world"`.
- [x] **ID-2** (U) Golden zero vectors.
- [ ] **ID-3** (U) `identifier(data) == identifier_from_parts(len(data), tree_root(data))` for many sizes.
- [ ] **ID-4** (U) Prefix is exactly `terrapin-sha256:` and the hex part is 64 lowercase hex.
- [ ] **ID-5** (U) Identifier ≠ `to_hex(tree_root(data))` and ≠ OmniBOR-style `gitoid:blob:sha256:…` — i.e. it is `G(manifest)`, not the bare root (§8 migration).
- [ ] **ID-6** (U) **Length is committed**: same tree root hex but different `length` in `identifier_from_parts` yields a different identifier (§7 reinterpretation defense).
- [ ] **ID-7** (U) Distinct inputs of different length have distinct identifiers (empty vs 1-byte vs BLOCK).
- [ ] **ID-8** (regression) Snapshot a fixed table of `(input, identifier)` to catch accidental algorithm drift.

---

## 7. `derive_counts` / `offsets_from_counts` — §4.3, §6

- [ ] **DC-1** (U) `derive_counts(0) == [1]`, `derive_counts(1) == [1]`, `derive_counts(BLOCK) == [1]`.
- [ ] **DC-2** (U) `derive_counts(BLOCK+1) == [2]`.
- [ ] **DC-3** (U) `derive_counts(FANOUT·BLOCK) == [65536]` (1 layer — exact-fit boundary).
- [ ] **DC-4** (U) `derive_counts(FANOUT·BLOCK + 1) == [65537, 2]` (2 layers).
- [ ] **DC-5** (U) Spec §4.3 worked example: `derive_counts(1 PiB) == [536870912, 8192]`.
- [ ] **DC-6** (U) `derive_counts(FANOUT²·BLOCK) == [FANOUT², FANOUT]` (still 2 layers — exact boundary).
- [ ] **DC-7** (U) `derive_counts(FANOUT²·BLOCK + 1) == [FANOUT²+1, FANOUT+1, 2]` (**3 layers** — the smallest 3-layer arithmetic case).
- [ ] **DC-8** (U) `derive_counts(u64::MAX)` terminates, no overflow, monotonically shrinking, last ≤ FANOUT.
- [ ] **DC-9** (U) Cross-check: for every test size, `derive_counts(len)` equals the per-layer hash counts produced by `TreeBuilder::build` (synthetic where needed).
- [ ] **OFF-1** (U) `offsets_from_counts`: `off[0]==0`, `off[L]==Σ_{k<L} count[k]·32`, single- and multi-layer.
- [ ] **OFF-2** (U) Offsets land on 32-byte boundaries and the total equals `.blocks` size.

---

## 8. `TreeBuilder` / `BuiltTree` — builder.rs

- [x] **TB-1** (U) `build_root` vs `tree_root` for several sizes.
- [x] **TB-2** (U) FANOUT boundaries via synthetic leaves.
- [ ] **TB-3** (U) Single-leaf: 1 pushed leaf -> root is the bare leaf; `layers == [[leaf]]`; `BuiltTree.identifier()==identifier(data)`.
- [ ] **TB-4** (U) Empty dataset path: push `g("")` once -> root `g("")`, identifier matches empty.
- [ ] **TB-5** (U) Multi-layer structure (synthetic, **INF-4**): push `FANOUT+1` distinct leaves ->
  - `layers.len()==2`, counts `[FANOUT+1, 2]`;
  - `layers[1][0..32]==g(layers[0][0 .. FANOUT·32])`, `layers[1][32..64]==g(layers[0][FANOUT·32..])`;
  - `root==g(layers[1])`.
- [ ] **TB-6** (U) Internal consistency for any built tree: for each `L`, `layers[L+1][j]==g(layers[L][group j])`, and `root==g(layers[last])` (or bare leaf).
- [ ] **TB-7** (U) `leaf_count()` accurate after N pushes.
- [ ] **TB-8** (U) Order sensitivity: swapping two pushed leaves changes the root.
- [ ] **TB-9** (U) `BuiltTree.length` is independent of leaf count (set explicitly) and flows into the identifier.
- [ ] **TB-10** (U) `BuiltTree.tree_hex() == to_hex(tree_root(data))`.
- [ ] **TB-11** (U, debug) `build()` with **zero** leaves: assert the documented behavior (debug_assert fires / defined result) so callers can't silently get `g("")` for a non-empty length.

---

## 9. Streaming reader — `BlockReader` (stream.rs)

- [ ] **BR-1** (U) Contiguous reader of `k·BLOCK` bytes -> exactly `k` full blocks, **no** trailing empty block.
- [ ] **BR-2** (U) `k·BLOCK + r` bytes -> `k` full + 1 short (`r`) block.
- [ ] **BR-3** (U) Empty reader -> exactly **one** empty block (then `None`).
- [ ] **BR-4** (U) `BLOCK`-exact then EOF -> one block, no spurious empty.
- [x] **BR-5** (U) Short reads reassembled to exact boundaries (Choppy) — exists in `matches_under_short_reads` (extract a direct BlockReader unit).
- [ ] **BR-6** (U) `ErrorKind::Interrupted` mid-fill is retried (Flaky reader), block still assembled correctly.
- [ ] **BR-7** (U) Hard `io::Error` is surfaced as `Some(Err(..))`, and **subsequent** `next()` returns `None` (the `finished` latch — no infinite loop, no further reads).
- [ ] **BR-8** (U) A reader that returns `Ok(0)` before true EOF terminates the block early (documents the `Read` EOF contract).

---

## 10. Streaming + parallel build — `build_from_reader` / `identifier_from_reader`

- [x] **SB-1** (U) Reader == in-memory identifier for {0,1,BLOCK-1,BLOCK,BLOCK+1,2·BLOCK,3·BLOCK+7}.
- [x] **SB-2** (U) Stable under varied short-read chunk sizes.
- [ ] **SB-3** (U) Non-zero / pseudo-random data of several multi-block sizes -> equals `identifier(data)` and `BuiltTree.identifier()`.
- [ ] **SB-4** (U) `BuiltTree.length` from the reader equals the true byte length (sum of block lengths), incl. short final block.
- [ ] **SB-5** (U) **Determinism across runs/threads**: same input hashed many times (and under `flavor="current_thread"` vs `"multi_thread"`) gives identical identifier — proves `buffered` order preservation, not `buffer_unordered`.
- [ ] **SB-6** (U) Order proof: 4+ distinct blocks; result equals the in-memory root (a reordering bug would diverge).
- [ ] **SB-7** (U) `identifier_from_reader == build_from_reader(..).identifier()`.
- [ ] **SB-8** (U) Mid-stream reader error -> `build_from_reader` returns `Err`, never a wrong-but-successful hash.
- [ ] **SB-9** (U) Runs on a single-core runtime (`available_parallelism()==1` path) without deadlock (force via `current_thread`).
- [ ] **SB-10** (I, moderately large) 64 MiB pseudo-random file from disk (`File`, not Cursor) matches in-memory.
- [ ] **SB-11** (S, **INF-1**) `ZeroReader` of `FANOUT·BLOCK` and `FANOUT·BLOCK+1` -> root matches the algebraic `tree_root_zero` oracle (streaming **2-layer** correctness without RAM blowup).
- [ ] **SB-12** (U) Memory-shape sanity: building a large `ZeroReader` does not allocate O(dataset) (observe stable peak via a counting allocator, or at least assert it completes within a leaf-hash-sized budget) — guards the "never hold the dataset" claim.

---

## 11. `PersistedTree::write` / `read` (artifact format)

- [ ] **PT-W1** (U) `.blocks` size == `Σ counts · 32`; bytes == concatenation of `layers` in order.
- [ ] **PT-W2** (U) `.head` text matches the exact expected lines (version, algorithm, block_size, length, tree, identifier, layer_counts).
- [ ] **PT-W3** (U) **Reproducible**: writing the same `BuiltTree` twice yields byte-identical `.head` and `.blocks` (publishability / dedup).
- [ ] **PT-W4** (U) `with_ext` builds `name.head`/`name.blocks`, including names that already contain dots (`foo.bin` -> `foo.bin.head`) and names with directories.
- [x] **PT-R1** (U) Roundtrip read preserves length/tree/identifier/counts (in `roundtrip_validate_and_ranges`).
- [ ] **PT-R2** (U) `read` rejects: missing `.head` file (clear error).
- [ ] **PT-R3** (U) `read` rejects: malformed line (no `": "`), unknown key, missing required key.
- [ ] **PT-R4** (U) `read` rejects: wrong `terrapin-tree` version.
- [ ] **PT-R5** (U) `read` rejects: `block_size` ≠ 2097152.
- [ ] **PT-R6** (U) `read` rejects: `algorithm` ≠ `terrapin-sha256`.
- [ ] **PT-R7** (U) `read` rejects: `layer_counts` inconsistent with `length` (the `derive_counts` cross-check).
- [ ] **PT-R8** (U) `read` rejects: non-numeric / empty `layer_counts`.
- [ ] **PT-R9** (U) **Decision**: `read` accepts an uppercase or wrong-length `tree` hex? Currently length-wrong fails later via `hex_to_32`; uppercase passes and still binds. Add a test asserting the chosen policy (reject at read vs accept-as-cosmetic). Pairs with **HEX-6**.
- [ ] **PT-R10** (U) `read` tolerates a trailing newline / final empty line in `.head` (or rejects — assert the choice; `lines()` drops it).
- [ ] **PT-R11** (U) `.head` with CRLF -> defined behavior (the `\r` would land in values) — assert reject or document.

---

## 12. `PersistedTree::validate` — success paths (§6)

(1-layer trees, i.e. ≤ FANOUT blocks; build from real multi-block files.)

- [x] **V-1** (U) Whole multi-block file validates.
- [x] **V-2** (U) Sub-block range; range spanning two blocks; last partial block.
- [ ] **V-3** (U) Single-block (≤BLOCK) file: whole-file validate (single-leaf path).
- [ ] **V-4** (U) `None/None` defaults to whole file; `start=Some,end=None`; `start=None,end=Some`.
- [ ] **V-5** (U) Empty range `[k,k)` -> success (header verified, no blocks walked), for `k` at 0, mid, and `length`.
- [ ] **V-6** (U) Range exactly one full block `[BLOCK, 2·BLOCK)`.
- [ ] **V-7** (U) Range straddling a block boundary `[BLOCK-1, BLOCK+1)`.
- [ ] **V-8** (U) Single-byte ranges at offsets 0, BLOCK, length-1.
- [ ] **V-9** (U) Range covering only the short final block.
- [ ] **V-10** (U) Content-addressed: validate a **copy** of the data (different path, identical bytes) -> success.
- [ ] **V-11** (U) Idempotent: validating twice in a row both succeed (re-opens `.blocks`, no state corruption).
- [ ] **V-12** (S, **INF-1** sparse file) 2-layer tree: build a ≥128 GiB sparse zero file once (`#[ignore]`), validate a **1-block range** cheaply, and validate a range whose path crosses a layer-1 group boundary (mirrors §6.1).

---

## 13. `PersistedTree::validate` — failure & rejection paths

- [x] **VF-1** (U) Tampered data byte inside the range -> error (in roundtrip test).
- [x] **VF-2** (U) Corrupt head root hex -> identifier binding fails.
- [ ] **VF-3** (U) Data length ≠ tree length (truncated and extended file) -> explicit length-mismatch error.
- [ ] **VF-4** (U) Bounds: `start>end`, `end>length`, `start>length` -> error (not a panic).
- [ ] **VF-5** (U) Corrupt a **leaf hash** in `.blocks` -> validate of **any** range fails (1-layer recompute uses the whole leaf group). Distinguish from VF-6.
- [ ] **VF-6** (U) **Slice independence (data)**: tamper a data byte **outside** the validated range -> the slice still validates (we never read that block). Mirrors the CLI e2e finding; document the cost/trust model.
- [ ] **VF-7** (U) Truncated `.blocks` (a group read runs past EOF) -> error, not panic.
- [ ] **VF-8** (U) Missing `.blocks` file at validate time -> error.
- [ ] **VF-9** (U) Missing data file -> error.
- [ ] **VF-10** (U) Tree built from data A, validated against different data B of the **same length** -> fail.
- [ ] **VF-11** (U) Two data blocks swapped (same multiset, different order) -> fail (position committed).
- [ ] **VF-12** (U) Corrupt head `identifier` field (but root intact) -> `check_identifier` fails.
- [ ] **VF-13** (U) Single-leaf tree, tampered single block -> fail.
- [ ] **VF-14** (U) Empty-dataset tree validated against a **non-empty** file -> length-mismatch fail.
- [ ] **VF-15** (S) Multi-layer: corrupt an **upper-layer** hash in `.blocks` on the validated path -> fail; corrupt an upper-layer hash on a **different** path -> slice still validates (layer-local trust). (`#[ignore]`, needs ≥2-layer artifact.)

---

## 14. `cat` (validate + stream) — §6

- [ ] **CAT-1** (U) `cat` whole file -> output equals original bytes.
- [x] **CAT-2** (U) `cat` a multi-block range equals `data[start..end]` (roundtrip test).
- [ ] **CAT-3** (U) `cat` ranges: within a block, straddling a boundary, only the short final block, single byte, `[0,0)` (empty -> no output).
- [ ] **CAT-4** (U) Slice math: the emitted bytes are exactly `[max(start,bs)…min(end,be))` per block — no off-by-one at boundaries, no extra bytes.
- [ ] **CAT-5** (U) **Binary-safe**: output of arbitrary bytes (incl. `\n`, `\r`, NUL, 0xFF) is byte-exact, no newline added/translated.
- [ ] **CAT-6** (U) **Partial-output on failure**: a range where an early block is valid but a later block is tampered emits the early (verified) blocks then errors — document this is per-block post-verification (no unverified bytes emitted, but earlier verified bytes are). Consider a test asserting no bytes are emitted past the first failure.
- [ ] **CAT-7** (U) `cat` to a writer that errors (broken pipe) -> error surfaced cleanly.

---

## 15. CLI integration (`terrapin-cli/tests/`, assert_cmd)

- [ ] **CLI-1** (I) `id <file>` prints `terrapin-sha256:<hex>\n`, exit 0, equals library `identifier`.
- [ ] **CLI-2** (I) `id` of a missing file -> stderr message, exit 1.
- [ ] **CLI-3** (I) `attest <file>` writes `<file>.terra.head`/`.blocks`, prints identifier, exit 0.
- [ ] **CLI-4** (I) `attest --out NAME` writes `NAME.head`/`.blocks`.
- [ ] **CLI-5** (I) `attest` identifier == `id` identifier (same file).
- [ ] **CLI-6** (I) `validate <file> --tree NAME` -> success message, exit 0.
- [ ] **CLI-7** (I) `validate` on tampered file -> stderr "Validation failed…", exit 1.
- [ ] **CLI-8** (I) `validate --start --end` valid range -> exit 0; out-of-bounds range -> exit 1.
- [ ] **CLI-9** (I) `validate --tree` pointing at missing/corrupt head -> exit 1 with message.
- [ ] **CLI-10** (I) `cat <file> --tree NAME` -> bytes to stdout, exit 0; `--start/--end` slice equals `dd` output.
- [ ] **CLI-11** (I) `cat` on tampered file -> exit 1.
- [ ] **CLI-12** (I) Unknown subcommand / missing required arg / negative `--start` -> structopt usage error, exit ≠ 0.
- [ ] **CLI-13** (I) `--help` / subcommand `--help` render and exit 0.
- [ ] **CLI-14** (I) Round-trip across an independent process boundary: `attest` in one invocation, `validate`/`cat` in another (no shared in-memory state).
- [ ] **CLI-15** (I) **Trusted-identifier gap**: spec §6 step 1 starts from a *trusted* identifier. Today `validate` trusts the head's own identifier. Add/cover a `validate --identifier terrapin-sha256:…` that asserts `head.identifier == provided` and fails on mismatch (forged self-consistent tree for different data must not pass when a trusted id is supplied). May require an implementation change — note it.

---

## 16. Property-based (proptest) — §all

- [ ] **PR-1** Random `data` (len 0..~10·BLOCK): `identifier_from_reader == identifier == BuiltTree.identifier`.
- [ ] **PR-2** Random `data` + random chunk schedule (Choppy with random sizes): identifier invariant.
- [ ] **PR-3** Random `data` + random valid `[start,end)`: `validate` succeeds and `cat == data[start..end]`.
- [ ] **PR-4** Flip a random single byte of `data` -> identifier changes (avalanche).
- [ ] **PR-5** Random `data`: write -> read -> validate(whole) always succeeds.
- [ ] **PR-6** Random valid manifest fields -> `parse_manifest(manifest_bytes(..))` roundtrips; random single-byte mutation -> rejected unless identical.
- [ ] **PR-7** Random leaf streams into `TreeBuilder` -> `layers` satisfy the internal `g`-of-group relation and `derive_counts(len)` matches counts.

---

## 17. Conformance / cross-implementation — §3, §C

- [ ] **CF-1** (C, **INF-6**) Load `vectors-terrapin.json` and assert every `(input|length, tree, id)` triple.
- [ ] **CF-2** (C) Include the boundary vectors (empty, 1, BLOCK±1, FANOUT·BLOCK±1) and the §5.4 example (1,203,942 bytes).
- [ ] **CF-3** (C) Same vectors file is consumed by `terrapin-go` (shared fixture) — parity guard (out-of-repo CI note).
- [ ] **CF-4** (regression) A frozen snapshot of identifiers for a small fixed corpus; breaking it requires an intentional spec bump.

---

## 18. Security / adversarial — §7

- [ ] **SEC-1** (U) **Length reinterpretation**: a tree/identifier for a small dataset cannot validate a larger dataset, and vice versa (length committed; data-length check + manifest binding).
- [ ] **SEC-2** (U) Forged tree with a self-consistent head for **different** data passes `validate` **without** a trusted id but **fails** when the true identifier is supplied (ties to **CLI-15**).
- [ ] **SEC-3** (U) Bare-tree-root confusion: feeding the tree root hex as if it were the identifier is rejected (the digest is `G(manifest)`).
- [ ] **SEC-4** (U) Truncation at the hasher: a data file truncated mid-final-block -> length mismatch / leaf mismatch fail (the `blob <len>\0` framing + length check).
- [ ] **SEC-5** (U) Swapped/duplicated blocks (same bytes, wrong positions) -> fail.
- [ ] **SEC-6** (doc/test) Second-preimage resistance is inherited from SHA-256 — assert the construction adds no obvious collision (e.g. a 1-block dataset whose bytes equal an upper hash-file block cannot share an identifier, because lengths/manifests differ — §7 bullet 1).
- [ ] **SEC-7** (U) Non-canonical manifest supplied out-of-band is rejected (re-assert §5.2 at the validation boundary, not just in `parse_manifest`).

---

## 19. Spec worked-example tests

- [ ] **WE-1** (U) §4.3 layering: `derive_counts(1 PiB) == [536870912, 8192]` (also **DC-5**).
- [ ] **WE-2** (U) §5.4: a 1,203,942-byte dataset is a single block, `tree == g(dataset)`, and `derive_counts == [1]`.
- [ ] **WE-3** (U) §6.1 path index math: for leaf index 250,000,000, `idx/FANOUT == 3814` and `idx - 3814·FANOUT == 45696` — assert the exact arithmetic `validate` uses to locate path blocks (pure, no huge data).
- [ ] **WE-4** (U) §6.1 transfer-set reasoning: for a synthetic ≥2-layer tree, the set of `.blocks` byte ranges touched by a single-block validation equals {one layer-0 group, one layer-1 group} (assert via instrumented `read_group` calls or an offset calculation), proving "fetch one hash-file block per layer."

---

## 20. Concurrency / runtime

- [ ] **RT-1** (U) `build_from_reader` correct under `#[tokio::test(flavor="multi_thread", worker_threads=N)]` for N in {1,2,8}.
- [ ] **RT-2** (U) Works under `current_thread` runtime (spawn_blocking still offloads).
- [ ] **RT-3** (U) Concurrent `validate` of the same tree from multiple tasks (read-only `.blocks`) all succeed.
- [ ] **RT-4** (U) `R: Read + Send + 'static` bound holds for `File` and `Cursor` (compile-time coverage via the tests above).

---

## 21. Determinism / reproducibility

- [ ] **DET-1** (U) Identical input -> identical identifier, identical `.head`, identical `.blocks` (also **PT-W3**, **SB-5**).
- [ ] **DET-2** (U) Artifact is independent of read chunking and of thread count.

---

## 22. Performance smoke (non-asserting)

- [ ] **PERF-1** (S) `cargo run --example bench --release` completes and prints three lines (smoke, no throughput assertion to avoid flakiness).
- [ ] **PERF-2** (S, optional) On multi-core CI, assert parallel build ≥ 1.5× single-thread `tree_root` as a loose regression guard (gate behind an env flag).

---

## 23. Known coverage limitations (document, don't pretend)

- **Real ≥2-layer trees need ≥128 GiB of data.** Covered indirectly by: `derive_counts` arithmetic (3-layer), synthetic-leaf `TreeBuilder` structure (**TB-5**), the algebraic zero-oracle for 2 layers (**SB-11**, **TR-2**), and `#[ignore]`d sparse-file integration (**V-12**, **VF-15**). True ≥3-layer end-to-end validation is not feasible in a normal test run.
- **`spawn_blocking` JoinError (task panic)** is hard to trigger since `g` doesn't panic; the error path is covered structurally, not by a real panic.
- **Allocation bound** (**SB-12**) is best-effort (counting allocator) rather than a hard guarantee.
- **Multi-petabyte streaming-to-disk of layer 0** is out of current scope (builder retains the leaf layer in memory ≈ dataset/65536); no test exercises the not-yet-built disk-streamed path.

---

## 24. Suggested priority order

1. **Correctness core** (must-have before trusting the rewrite): MAN-R*, TR-3/6/7, ID-3/5/6, DC-1..8, TB-3/5/6, BR-1..4/7, SB-3/5/6/8, PT-W1..3, V-3..9, VF-3..14, CAT-1/3/4/6.
2. **CLI surface**: CLI-1..14 (+ CLI-15 decision).
3. **Adversarial + conformance**: SEC-1..7, CF-1..4, WE-1..4.
4. **Property tests**: PR-1..7.
5. **Slow/large + perf**: SB-11, V-12, VF-15, PERF-*.
6. **Open decisions to resolve via tests**: HEX-6 / PT-R9 (uppercase hex policy), CLI-15 (trusted-identifier input), CAT-6 (partial-output semantics), PT-R10/R11 (head whitespace/CRLF policy).
