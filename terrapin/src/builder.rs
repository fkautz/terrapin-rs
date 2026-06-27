//! Streaming tree builder.
//!
//! Leaf hashes (one `g(block)` per 2 MiB data block) are pushed in order; the
//! builder retains every layer's hash file so the full tree can be persisted
//! (spec section 4.2). The root is computed by applying the spec `T` reduction
//! (section 4.3) to the retained leaf layer, so the result is identical to the
//! reference [`crate::tree_root`] by construction — including the FANOUT-power
//! boundaries (e.g. exactly 65536 leaves produce a single wrap, not two).
//!
//! Memory is `O(dataset_len / FANOUT)` (the size of the leaf hash file), never
//! the dataset itself.

use crate::manifest::{g, identifier_from_parts, to_hex, BLOCK};

/// Accumulates leaf hashes and builds the recursive tree.
#[derive(Default)]
pub struct TreeBuilder {
    /// Concatenated 32-byte leaf hashes (the layer-0 hash file).
    leaves: Vec<u8>,
}

/// A fully built tree: every layer's hash file plus the derived root.
pub struct BuiltTree {
    /// Total dataset length in bytes.
    pub length: u64,
    /// `layers[0]` is the leaf hash file; `layers[k]` is the topmost hash file.
    /// Each entry is a concatenation of raw 32-byte hashes.
    pub layers: Vec<Vec<u8>>,
    /// The recursive tree root `T(dataset)`.
    pub root: [u8; 32],
}

impl TreeBuilder {
    pub fn new() -> Self {
        TreeBuilder { leaves: Vec::new() }
    }

    /// Append one leaf hash (`g` of a data block), in block order.
    pub fn push_leaf(&mut self, h: &[u8; 32]) {
        self.leaves.extend_from_slice(h);
    }

    /// Number of leaf hashes pushed so far.
    pub fn leaf_count(&self) -> u64 {
        (self.leaves.len() / 32) as u64
    }

    /// Finish the tree for a dataset of `length` bytes.
    ///
    /// Requires at least one leaf (an empty dataset is one empty leaf, `g("")`).
    pub fn build(self, length: u64) -> BuiltTree {
        let mut layers: Vec<Vec<u8>> = vec![self.leaves];
        debug_assert!(!layers[0].is_empty(), "at least one leaf is required");

        let root: [u8; 32];
        if layers[0].len() == 32 {
            // Single leaf (dataset <= BLOCK, including the empty case): the root
            // is the bare leaf, never g(leaf). Spec section 4.3 base case.
            root = layers[0][..32].try_into().unwrap();
        } else {
            let mut cur = 0;
            loop {
                if layers[cur].len() <= BLOCK {
                    // <= FANOUT hashes left: one final wrap is the root.
                    root = g(&layers[cur]);
                    break;
                }
                // Split into FANOUT-hash (BLOCK-byte) groups, last may be short,
                // and g each group to form the next layer.
                let next: Vec<u8> = layers[cur].chunks(BLOCK).flat_map(g).collect();
                layers.push(next);
                cur += 1;
            }
        }

        BuiltTree {
            length,
            layers,
            root,
        }
    }
}

impl BuiltTree {
    /// The `terrapin-sha256:<hex>` identifier (spec section 5.3).
    pub fn identifier(&self) -> String {
        identifier_from_parts(self.length, &self.root)
    }

    /// The tree root as 64 lowercase hex.
    pub fn tree_hex(&self) -> String {
        to_hex(&self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{tree_root, FANOUT};

    /// Build a tree from raw bytes (in-memory), mirroring how the streaming
    /// path feeds leaves, and return the root.
    fn build_root(data: &[u8]) -> [u8; 32] {
        let mut b = TreeBuilder::new();
        if data.len() <= BLOCK {
            b.push_leaf(&g(data));
        } else {
            let mut i = 0;
            while i < data.len() {
                let end = std::cmp::min(i + BLOCK, data.len());
                b.push_leaf(&g(&data[i..end]));
                i = end;
            }
        }
        b.build(data.len() as u64).root
    }

    // Verifies: REQ-TB-001
    #[test]
    fn matches_reference_small_sizes() {
        for len in [0usize, 1, 31, 32, 33, 1000, BLOCK - 1, BLOCK, BLOCK + 1, 2 * BLOCK, 3 * BLOCK + 7] {
            let data = vec![0xabu8; len];
            assert_eq!(build_root(&data), tree_root(&data), "len {}", len);
        }
    }

    /// Feed `n` leaf hashes directly (each a distinct value) and compare the
    /// builder root against a straight reduction over the same leaf bytes.
    fn reduce_reference(leaves: &[u8]) -> [u8; 32] {
        // Mirror BuiltTree::build's reduction independently.
        if leaves.len() == 32 {
            return leaves[..32].try_into().unwrap();
        }
        let mut cur = leaves.to_vec();
        loop {
            if cur.len() <= BLOCK {
                return g(&cur);
            }
            cur = cur.chunks(BLOCK).flat_map(g).collect();
        }
    }

    // Verifies: REQ-TB-002
    #[test]
    fn fanout_boundaries() {
        for &n in &[1usize, 2, FANOUT - 1, FANOUT, FANOUT + 1] {
            let mut b = TreeBuilder::new();
            let mut leaves = Vec::with_capacity(n * 32);
            for i in 0..n {
                // distinct leaf values
                let h = g(&(i as u64).to_le_bytes());
                b.push_leaf(&h);
                leaves.extend_from_slice(&h);
            }
            let got = b.build((n as u64) * BLOCK as u64).root;
            assert_eq!(got, reduce_reference(&leaves), "n {}", n);
        }
    }

    // Verifies: REQ-TB-011
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn zero_leaf_build_panics_in_debug() {
        // A build with no leaves is a caller error: every dataset (incl. empty)
        // contributes at least one leaf. Guarded by debug_assert.
        let b = TreeBuilder::new();
        let _ = b.build(0);
    }
}
