# Terrapin Requirement Waivers

Requirements deliberately not covered by an automated test, with an allowed
reason and rationale. A requirement is either tested or waived, never both.

### REQ-CF-003
- Reason: not-implemented
- Rationale: Cross-implementation parity with terrapin-go is verified in a separate repository's CI against the shared vectors-terrapin.json; not reproducible inside this crate's test run.

### REQ-SEC-006
- Reason: foundational
- Rationale: Second-preimage resistance is inherited from SHA-256 and the length-binding manifest (covered indirectly by REQ-ID-006 and REQ-SEC-001); there is no in-suite way to demonstrate collision resistance directly.

### REQ-WE-004
- Reason: not-implemented
- Rationale: Asserting the exact transfer-set (one hash-file block per layer) requires instrumenting the private read_group call path; deferred until a validation hook exists. The path arithmetic itself is covered by REQ-WE-003.

### REQ-RT-004
- Reason: covered-by
- Rationale: The Read + Send + 'static bound is enforced at compile time by REQ-SB-010 (drives a std::fs::File) and REQ-SB-001 (drives a Cursor); a dedicated test would add nothing the type system does not already guarantee.

### REQ-SB-012
- Reason: not-implemented
- Rationale: A hard O(dataset) memory bound needs a process-global counting allocator, which would perturb every other test; tracked as a future dedicated harness. The streaming design is exercised by REQ-SB-011 (ZeroReader, no dataset allocation).

### REQ-PERF-001
- Reason: deployment-guidance
- Rationale: Throughput is observed by running `cargo run --example bench --release`; not asserted in the test suite to avoid hardware-dependent flakiness.

### REQ-PERF-002
- Reason: deployment-guidance
- Rationale: The parallel-vs-single-thread speedup is environment-dependent (core count); measured via the bench example rather than gated in CI.
