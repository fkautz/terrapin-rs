//! Terrapin: parallel, streaming content addressing for very large datasets.
//!
//! Terrapin hashes a dataset by splitting it into fixed 2 MiB blocks, hashing
//! each with GitOID SHA-256, and recursively hashing the resulting hash files
//! into a single tree root. The identifier is the GitOID of a canonical manifest
//! committing the algorithm, block size, length, and tree root:
//! `terrapin-sha256:<64 hex>`. See `docs/spec.md` for the normative definition.
//!
//! * [`identifier`] / [`tree_root`] — in-memory reference over a full slice.
//! * [`identifier_from_reader`] / [`build_from_reader`] — streaming + parallel
//!   construction that never holds the dataset in memory.
//! * [`PersistedTree`] — write a publishable two-file tree and validate (or
//!   stream) arbitrary byte ranges without reading the whole dataset.

mod builder;
mod manifest;
mod stream;
mod tree;

pub use builder::{BuiltTree, TreeBuilder};
pub use manifest::{
    g, identifier, identifier_from_parts, manifest_bytes, parse_manifest, to_hex, tree_root, BLOCK,
    FANOUT,
};
pub use stream::{build_from_reader, identifier_from_reader};
pub use tree::{derive_counts, PersistedTree};
