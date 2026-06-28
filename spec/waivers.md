# Terrapin Requirement Waivers

Requirements deliberately not covered by an automated test, with an allowed
reason and rationale. A requirement is either tested or waived, never both.

### REQ-CF-003
- Reason: not-implemented
- Rationale: Cross-implementation parity with terrapin-go is verified in a separate repository's CI against the shared vectors-terrapin.json; not reproducible inside this crate's test run.

### REQ-SEC-006
- Reason: foundational
- Rationale: Second-preimage resistance is inherited from SHA-256 and the length-binding manifest (covered indirectly by REQ-ID-006 and REQ-SEC-001); there is no in-suite way to demonstrate collision resistance directly.

### REQ-RT-004
- Reason: covered-by
- Rationale: The Read + Send + 'static bound is enforced at compile time by REQ-SB-010 (drives a std::fs::File) and REQ-SB-001 (drives a Cursor); a dedicated test would add nothing the type system does not already guarantee.

### REQ-PERF-002
- Reason: deployment-guidance
- Rationale: The parallel-vs-single-thread speedup is environment-dependent (core count); measured via the bench example rather than gated in CI.
