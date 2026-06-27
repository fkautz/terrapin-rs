# Terrapin Requirements Catalog

Requirement IDs for the traceability gate (`trace-check`). Each requirement is a
behavior verified by exactly one tagged test (or a recorded waiver). Section refs
point at `terrapin/docs/spec.md`; see `terrapin/docs/test-plan.md` for the
expanded rationale and the plan-ID this consolidates.

## Primitive g (gitoid-sha256) — §3.0

### REQ-G-001 — g("") is the git empty blob
- Section: §3.0
- Keyword: MUST

### REQ-G-002 — g equals sha256("blob <len>\0" + data)
- Section: §3.0
- Keyword: MUST

### REQ-G-003 — g binds length (differs from raw sha256)
- Section: §3.0
- Keyword: MUST

### REQ-G-004 — g is 32 bytes and deterministic
- Section: §3.0
- Keyword: MUST

### REQ-G-005 — g correct across BLOCK-boundary sizes
- Section: §3.0
- Keyword: MUST

### REQ-G-006 — g avalanche on single-bit change
- Section: §3.0
- Keyword: SHOULD

## Hex helpers

### REQ-HEX-001 — to_hex lowercase, zero-padded, empty
- Section: §3.0
- Keyword: MUST

### REQ-HEX-002 — hex_to_32 roundtrips to_hex
- Section: §5.3
- Keyword: MUST

### REQ-HEX-003 — hex_to_32 rejects bad length and non-hex
- Section: §5.3
- Keyword: MUST

### REQ-HEX-004 — head tree-hex case policy is defined
- Section: §5.2
- Keyword: SHOULD

## Manifest encoding & parse_manifest — §5.1, §5.2

### REQ-MAN-001 — manifest_bytes exact shape, 4 lines, final LF
- Section: §5.1
- Keyword: MUST

### REQ-MAN-002 — manifest field value "sha256" vs digest prefix "terrapin-sha256:"
- Section: §5.1
- Keyword: MUST

### REQ-MAN-003 — length is byte length; block_size literal 2097152
- Section: §5.1
- Keyword: MUST

### REQ-MAN-004 — parse_manifest accepts canonical, length 0 and u64::MAX, roundtrips
- Section: §5.2
- Keyword: MUST

### REQ-MAN-005 — parse_manifest rejects structural defects (ENC-2,4,5,9)
- Section: §5.2
- Keyword: MUST

### REQ-MAN-006 — parse_manifest rejects spacing defects (ENC-3)
- Section: §5.2
- Keyword: MUST

### REQ-MAN-007 — parse_manifest rejects value defects (ENC-1,6,7,8)
- Section: §5.2
- Keyword: MUST

### REQ-MAN-008 — parse_manifest rejects, never normalizes
- Section: §5.2
- Keyword: MUST

### REQ-MAN-009 — random mutation of a valid manifest is rejected unless identical
- Section: §5.2
- Keyword: SHOULD

## Reference tree_root — §4

### REQ-TR-001 — zero-data boundary tree vectors
- Section: §4.3
- Keyword: MUST

### REQ-TR-002 — algebraic recursion-boundary vectors
- Section: §4.3
- Keyword: MUST

### REQ-TR-003 — empty dataset root is g("")
- Section: §4.3
- Keyword: MUST

### REQ-TR-004 — non-zero multi-block sizes match
- Section: §4.3
- Keyword: MUST

### REQ-TR-005 — single leaf is the bare leaf, not g(leaf)
- Section: §4.3
- Keyword: MUST

### REQ-TR-006 — block order is significant
- Section: §4.3
- Keyword: MUST

### REQ-TR-007 — tree_root avalanche on single-byte change
- Section: §4.3
- Keyword: SHOULD

## Identifier — §5.3, §8

### REQ-ID-001 — explicit golden identifiers
- Section: §5.3
- Keyword: MUST

### REQ-ID-002 — identifier zero-data vectors
- Section: §5.3
- Keyword: MUST

### REQ-ID-003 — identifier == identifier_from_parts(len, tree_root)
- Section: §5.3
- Keyword: MUST

### REQ-ID-004 — prefix terrapin-sha256: and 64 lowercase hex
- Section: §5.3
- Keyword: MUST

### REQ-ID-005 — identifier is G(manifest), not bare root nor gitoid form
- Section: §8
- Keyword: MUST

### REQ-ID-006 — length is committed (same root, different length, different id)
- Section: §7
- Keyword: MUST

### REQ-ID-007 — distinct inputs yield distinct identifiers
- Section: §5.3
- Keyword: MUST

### REQ-ID-008 — identifier regression snapshot
- Section: §5.3
- Keyword: SHOULD

## derive_counts / offsets — §4.3, §6

### REQ-DC-001 — derive_counts small sizes
- Section: §6
- Keyword: MUST

### REQ-DC-002 — derive_counts exact-fit boundaries (FANOUT, FANOUT^2 blocks)
- Section: §6
- Keyword: MUST

### REQ-DC-003 — derive_counts multi-layer (2- and 3-layer)
- Section: §6
- Keyword: MUST

### REQ-DC-004 — derive_counts 1 PiB == [536870912, 8192]
- Section: §4.3
- Keyword: MUST

### REQ-DC-005 — derive_counts u64::MAX terminates without overflow
- Section: §6
- Keyword: MUST

### REQ-DC-006 — derive_counts matches builder layer counts
- Section: §6
- Keyword: MUST

### REQ-OFF-001 — offsets_from_counts correctness and 32-byte alignment
- Section: §6
- Keyword: MUST

## TreeBuilder / BuiltTree

### REQ-TB-001 — builder matches reference tree_root (small)
- Section: §4.3
- Keyword: MUST

### REQ-TB-002 — builder FANOUT boundaries (synthetic leaves)
- Section: §4.3
- Keyword: MUST

### REQ-TB-003 — single leaf yields bare-leaf root and matching identifier
- Section: §4.3
- Keyword: MUST

### REQ-TB-004 — empty path yields g("") root
- Section: §4.3
- Keyword: MUST

### REQ-TB-005 — multi-layer structure for FANOUT+1 leaves
- Section: §4.3
- Keyword: MUST

### REQ-TB-006 — internal consistency: layer[L+1]=g(group), root=g(top)
- Section: §4.3
- Keyword: MUST

### REQ-TB-007 — leaf_count accurate
- Section: §4.2
- Keyword: MUST

### REQ-TB-008 — leaf order significant
- Section: §4.3
- Keyword: MUST

### REQ-TB-009 — length independent of leaf count, flows to identifier
- Section: §5.1
- Keyword: MUST

### REQ-TB-010 — tree_hex == to_hex(tree_root)
- Section: §4.3
- Keyword: MUST

### REQ-TB-011 — zero-leaf build has defined behavior
- Section: §4.3
- Keyword: SHOULD

## Streaming reader — BlockReader

### REQ-BR-001 — k*BLOCK yields exactly k leaves (no spurious empty)
- Section: §4.1
- Keyword: MUST

### REQ-BR-002 — k*BLOCK+r yields k full + 1 short leaf
- Section: §4.1
- Keyword: MUST

### REQ-BR-003 — empty reader yields exactly one empty leaf
- Section: §4.1
- Keyword: MUST

### REQ-BR-004 — short reads reassembled to exact boundaries
- Section: §4.1
- Keyword: MUST

### REQ-BR-005 — Interrupted is retried
- Section: §4.1
- Keyword: MUST

### REQ-BR-006 — hard read error surfaced, never a wrong success
- Section: §4.1
- Keyword: MUST

### REQ-BR-007 — premature Ok(0) treated as EOF
- Section: §4.1
- Keyword: SHOULD

## Streaming + parallel build

### REQ-SB-001 — reader identifier == in-memory (key sizes)
- Section: §2.1
- Keyword: MUST

### REQ-SB-002 — identifier stable under short-read chunking
- Section: §2.1
- Keyword: MUST

### REQ-SB-003 — non-zero multi-block reader matches in-memory and BuiltTree
- Section: §2.1
- Keyword: MUST

### REQ-SB-004 — length from reader equals byte length
- Section: §5.1
- Keyword: MUST

### REQ-SB-005 — deterministic across runs and runtime flavors
- Section: §2.1
- Keyword: MUST

### REQ-SB-006 — block order preserved (distinct blocks)
- Section: §2.1
- Keyword: MUST

### REQ-SB-007 — identifier_from_reader == build_from_reader().identifier()
- Section: §5.3
- Keyword: MUST

### REQ-SB-008 — mid-stream reader error returns Err
- Section: §2.1
- Keyword: MUST

### REQ-SB-009 — works on single-threaded runtime without deadlock
- Section: §2.1
- Keyword: MUST

### REQ-SB-010 — 64 MiB file from disk matches in-memory
- Section: §2.1
- Keyword: SHOULD

### REQ-SB-011 — ZeroReader 2-layer matches algebraic oracle (slow)
- Section: §4.3
- Keyword: SHOULD

### REQ-SB-012 — streaming holds no O(dataset) memory
- Section: §2.1
- Keyword: SHOULD

## PersistedTree write/read

### REQ-PT-001 — .blocks size and content
- Section: §6
- Keyword: MUST

### REQ-PT-002 — .head exact text format
- Section: §6
- Keyword: MUST

### REQ-PT-003 — artifact is byte-reproducible
- Section: §6
- Keyword: MUST

### REQ-PT-004 — with_ext naming (incl dotted names and directories)
- Section: §6
- Keyword: MUST

### REQ-PT-005 — read roundtrip preserves length/tree/identifier/counts
- Section: §6
- Keyword: MUST

### REQ-PT-006 — read rejects missing head
- Section: §6
- Keyword: MUST

### REQ-PT-007 — read rejects malformed/unknown/missing key
- Section: §6
- Keyword: MUST

### REQ-PT-008 — read rejects bad version/block_size/algorithm
- Section: §6
- Keyword: MUST

### REQ-PT-009 — read rejects layer_counts inconsistent with length
- Section: §6
- Keyword: MUST

### REQ-PT-010 — read rejects non-numeric layer_counts
- Section: §6
- Keyword: MUST

### REQ-PT-011 — head whitespace/CRLF policy is defined
- Section: §6
- Keyword: SHOULD

## Validation — success — §6

### REQ-VAL-001 — whole multi-block file validates
- Section: §6
- Keyword: MUST

### REQ-VAL-002 — multi-block ranges validate (roundtrip)
- Section: §6
- Keyword: MUST

### REQ-VAL-003 — empty dataset validates
- Section: §6
- Keyword: MUST

### REQ-VAL-004 — single-block file validates
- Section: §6
- Keyword: MUST

### REQ-VAL-005 — default and one-sided ranges
- Section: §6
- Keyword: MUST

### REQ-VAL-006 — empty ranges succeed
- Section: §6
- Keyword: MUST

### REQ-VAL-007 — block-aligned and boundary-straddling ranges
- Section: §6
- Keyword: MUST

### REQ-VAL-008 — single-byte ranges
- Section: §6
- Keyword: MUST

### REQ-VAL-009 — last partial-block range
- Section: §6
- Keyword: MUST

### REQ-VAL-010 — content-addressed copy validates
- Section: §6
- Keyword: MUST

### REQ-VAL-011 — validation is idempotent
- Section: §6
- Keyword: MUST

### REQ-VAL-012 — 2-layer sparse-file range validates (slow)
- Section: §6
- Keyword: SHOULD

## Validation — failure — §6, §7

### REQ-VF-001 — tampered data inside range fails
- Section: §6
- Keyword: MUST

### REQ-VF-002 — corrupt head root fails (identifier binding)
- Section: §6
- Keyword: MUST

### REQ-VF-003 — data length mismatch fails
- Section: §6
- Keyword: MUST

### REQ-VF-004 — out-of-bounds range errors, not panics
- Section: §6
- Keyword: MUST

### REQ-VF-005 — corrupt leaf hash in .blocks fails any range
- Section: §6
- Keyword: MUST

### REQ-VF-006 — data tamper outside range still validates (slice independence)
- Section: §6
- Keyword: MUST

### REQ-VF-007 — truncated .blocks errors, not panics
- Section: §6
- Keyword: MUST

### REQ-VF-008 — missing .blocks file errors
- Section: §6
- Keyword: MUST

### REQ-VF-009 — missing data file errors
- Section: §6
- Keyword: MUST

### REQ-VF-010 — different data of same length fails
- Section: §7
- Keyword: MUST

### REQ-VF-011 — swapped blocks fail (position committed)
- Section: §7
- Keyword: MUST

### REQ-VF-012 — corrupt head identifier fails
- Section: §6
- Keyword: MUST

### REQ-VF-013 — single-leaf tree tamper fails
- Section: §6
- Keyword: MUST

### REQ-VF-014 — empty-dataset tree vs non-empty file fails
- Section: §6
- Keyword: MUST

### REQ-VF-015 — 2-layer upper-node tamper fails (slow)
- Section: §6
- Keyword: SHOULD

## cat (validate + stream) — §6

### REQ-CAT-001 — cat whole file equals bytes
- Section: §6
- Keyword: MUST

### REQ-CAT-002 — cat range equals data slice
- Section: §6
- Keyword: MUST

### REQ-CAT-003 — cat range variants incl empty
- Section: §6
- Keyword: MUST

### REQ-CAT-004 — cat slice math has no off-by-one
- Section: §6
- Keyword: MUST

### REQ-CAT-005 — cat output is binary-safe
- Section: §6
- Keyword: MUST

### REQ-CAT-006 — cat emits no bytes past first failure
- Section: §6
- Keyword: SHOULD

### REQ-CAT-007 — cat surfaces writer errors
- Section: §6
- Keyword: SHOULD

## CLI (black-box)

### REQ-CLI-001 — id prints identifier equal to the library
- Section: §5.3
- Keyword: MUST

### REQ-CLI-002 — id on missing file exits non-zero
- Section: §6
- Keyword: MUST

### REQ-CLI-003 — attest writes tree files and prints identifier
- Section: §6
- Keyword: MUST

### REQ-CLI-004 — attest --out chooses the base name
- Section: §6
- Keyword: MUST

### REQ-CLI-005 — attest identifier equals id
- Section: §5.3
- Keyword: MUST

### REQ-CLI-006 — validate succeeds with exit 0
- Section: §6
- Keyword: MUST

### REQ-CLI-007 — validate on tamper exits non-zero
- Section: §6
- Keyword: MUST

### REQ-CLI-008 — validate range and out-of-bounds handling
- Section: §6
- Keyword: MUST

### REQ-CLI-009 — validate on missing/corrupt tree exits non-zero
- Section: §6
- Keyword: MUST

### REQ-CLI-010 — cat emits bytes; range equals dd
- Section: §6
- Keyword: MUST

### REQ-CLI-011 — cat on tamper exits non-zero
- Section: §6
- Keyword: MUST

### REQ-CLI-012 — bad arguments produce a usage error
- Section: §6
- Keyword: MUST

### REQ-CLI-013 — help renders and exits 0
- Section: §6
- Keyword: SHOULD

### REQ-CLI-014 — attest and validate work across process boundaries
- Section: §6
- Keyword: MUST

### REQ-CLI-015 — validate enforces a supplied trusted identifier
- Section: §6
- Keyword: MUST

## Property-based

### REQ-PR-001 — random data: streaming id == in-memory id
- Section: §2.1
- Keyword: SHOULD

### REQ-PR-002 — random chunking does not change the identifier
- Section: §2.1
- Keyword: SHOULD

### REQ-PR-003 — random valid range validates and cat equals slice
- Section: §6
- Keyword: SHOULD

### REQ-PR-004 — single-byte flip changes the identifier
- Section: §7
- Keyword: SHOULD

### REQ-PR-005 — random data write/read/validate succeeds
- Section: §6
- Keyword: SHOULD

### REQ-PR-006 — random leaf streams satisfy the builder layer relation
- Section: §4.3
- Keyword: SHOULD

## Conformance

### REQ-CF-001 — load vectors-terrapin.json and verify all
- Section: §3
- Keyword: MUST

### REQ-CF-002 — boundary and §5.4 example vectors
- Section: §5.4
- Keyword: MUST

### REQ-CF-003 — cross-implementation parity (terrapin-go)
- Section: §3
- Keyword: SHOULD

### REQ-CF-004 — frozen identifier corpus snapshot
- Section: §5.3
- Keyword: SHOULD

## Security / adversarial — §7

### REQ-SEC-001 — length reinterpretation is prevented
- Section: §7
- Keyword: MUST

### REQ-SEC-002 — forged tree fails against a trusted identifier
- Section: §7
- Keyword: MUST

### REQ-SEC-003 — bare tree root is not accepted as the identifier
- Section: §7
- Keyword: MUST

### REQ-SEC-004 — truncation at the hasher is detected
- Section: §7
- Keyword: MUST

### REQ-SEC-005 — swapped/duplicated blocks are detected
- Section: §7
- Keyword: MUST

### REQ-SEC-006 — second-preimage resistance (foundational)
- Section: §7
- Keyword: SHOULD

### REQ-SEC-007 — non-canonical manifest rejected at the validation boundary
- Section: §7
- Keyword: MUST

## Spec worked examples

### REQ-WE-002 — §5.4 single-block example (tree == g(dataset))
- Section: §5.4
- Keyword: MUST

### REQ-WE-003 — §6.1 path index arithmetic
- Section: §6
- Keyword: MUST

### REQ-WE-004 — §6.1 transfer-set is one hash-file block per layer
- Section: §6
- Keyword: SHOULD

## Concurrency

### REQ-RT-001 — correct under multi-thread runtimes (worker counts)
- Section: §2.1
- Keyword: MUST

### REQ-RT-003 — concurrent validation of the same tree succeeds
- Section: §6
- Keyword: SHOULD

### REQ-RT-004 — reader bound Read + Send + 'static holds
- Section: §2.1
- Keyword: SHOULD

## Performance (non-asserting)

### REQ-PERF-001 — bench example runs (smoke)
- Section: §10
- Keyword: MAY

### REQ-PERF-002 — parallel build outpaces single-thread (loose)
- Section: §10
- Keyword: MAY
