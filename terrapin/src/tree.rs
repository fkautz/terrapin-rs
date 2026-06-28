//! Publishable tree artifact and slice validation.
//!
//! The tree is written as two files so a validator can range-fetch only the
//! hash-file blocks it needs (spec section 6 path note):
//!
//! * `<name>.head`  — a small text header (algorithm, block size, length, tree
//!   root, identifier, per-layer hash counts).
//! * `<name>.blocks` — every layer's hash file concatenated, raw 32-byte hashes,
//!   32-byte aligned. Layer `L` starts at a known byte offset; hash `j` of layer
//!   `L` is at `offset[L] + j*32`.
//!
//! Validation (spec section 6) starts from the identifier, derives the exact
//! tree shape from `length`, and recomputes `G` upward along the path for each
//! requested data block, fetching one hash-file block per layer (cached across
//! the range), never reading the whole leaf layer for a small slice.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::builder::BuiltTree;
use crate::manifest::{g, identifier_from_parts, manifest_bytes, BLOCK, FANOUT};

const HEAD_VERSION: &str = "1";

/// A cached hash-file group along the validation path:
/// `(group_start_index, group_bytes, node = g(group))`.
type GroupCache = Option<(u64, Vec<u8>, [u8; 32])>;

/// Number of hashes at each layer, derived solely from `length` and the fixed
/// block size (spec section 6 step 3). `layers[0]` is the leaf count.
pub fn derive_counts(length: u64) -> Vec<u64> {
    let nblocks = if length == 0 {
        1 // empty dataset is one empty leaf
    } else {
        length.div_ceil(BLOCK as u64)
    };
    let mut counts = vec![nblocks];
    while *counts.last().unwrap() > FANOUT as u64 {
        let prev = *counts.last().unwrap();
        counts.push(prev.div_ceil(FANOUT as u64));
    }
    counts
}

fn offsets_from_counts(counts: &[u64]) -> Vec<u64> {
    let mut offs = Vec::with_capacity(counts.len());
    let mut acc = 0u64;
    for &c in counts {
        offs.push(acc);
        acc += c * 32;
    }
    offs
}

/// Start hash-index of the FANOUT-sized group containing hash index `idx`.
/// Shared by `validate`'s climb and `path_blocks` so the two never drift.
fn group_start(idx: u64) -> u64 {
    (idx / FANOUT as u64) * FANOUT as u64
}

/// A read handle for a persisted tree.
pub struct PersistedTree {
    pub length: u64,
    pub tree_hex: String,
    pub identifier: String,
    pub counts: Vec<u64>,
    offsets: Vec<u64>,
    blocks_path: PathBuf,
}

impl PersistedTree {
    /// Write the two-file artifact `<name>.head` / `<name>.blocks`.
    pub fn write(name: &Path, tree: &BuiltTree) -> io::Result<()> {
        let blocks_path = with_ext(name, "blocks");
        let head_path = with_ext(name, "head");

        let mut bf = File::create(&blocks_path)?;
        for layer in &tree.layers {
            bf.write_all(layer)?;
        }
        bf.flush()?;

        let counts: Vec<u64> = tree.layers.iter().map(|l| (l.len() / 32) as u64).collect();
        let counts_str = counts
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let head = format!(
            "terrapin-tree: {}\nalgorithm: terrapin-sha256\nblock_size: {}\nlength: {}\ntree: {}\nidentifier: {}\nlayer_counts: {}\n",
            HEAD_VERSION,
            BLOCK,
            tree.length,
            tree.tree_hex(),
            tree.identifier(),
            counts_str,
        );
        std::fs::write(&head_path, head)?;
        Ok(())
    }

    /// Open a persisted tree by base name.
    pub fn read(name: &Path) -> Result<PersistedTree, String> {
        let head_path = with_ext(name, "head");
        let blocks_path = with_ext(name, "blocks");
        let text = std::fs::read_to_string(&head_path)
            .map_err(|e| format!("cannot read {}: {}", head_path.display(), e))?;

        let mut version = None;
        let mut block_size = None;
        let mut length = None;
        let mut tree_hex = None;
        let mut identifier = None;
        let mut counts: Option<Vec<u64>> = None;

        for line in text.lines() {
            let (key, val) = line
                .split_once(": ")
                .ok_or_else(|| format!("head: bad line {:?}", line))?;
            match key {
                "terrapin-tree" => version = Some(val.to_string()),
                "algorithm" => {
                    if val != "terrapin-sha256" {
                        return Err(format!("head: unsupported algorithm {}", val));
                    }
                }
                "block_size" => block_size = Some(val.to_string()),
                "length" => {
                    length = Some(val.parse::<u64>().map_err(|_| "head: bad length".to_string())?)
                }
                "tree" => tree_hex = Some(val.to_string()),
                "identifier" => identifier = Some(val.to_string()),
                "layer_counts" => {
                    let cs = val
                        .split_whitespace()
                        .map(|s| s.parse::<u64>())
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| "head: bad layer_counts".to_string())?;
                    counts = Some(cs);
                }
                _ => return Err(format!("head: unknown key {}", key)),
            }
        }

        if version.as_deref() != Some(HEAD_VERSION) {
            return Err("head: unsupported terrapin-tree version".into());
        }
        if block_size.as_deref() != Some(&BLOCK.to_string()) {
            return Err("head: block_size must be 2097152".into());
        }
        let length = length.ok_or("head: missing length")?;
        let tree_hex = tree_hex.ok_or("head: missing tree")?;
        let identifier = identifier.ok_or("head: missing identifier")?;
        let counts = counts.ok_or("head: missing layer_counts")?;

        // The tree shape is a total function of length; reject a header whose
        // declared counts disagree with it.
        if counts != derive_counts(length) {
            return Err("head: layer_counts inconsistent with length".into());
        }
        let offsets = offsets_from_counts(&counts);

        Ok(PersistedTree {
            length,
            tree_hex,
            identifier,
            counts,
            offsets,
            blocks_path,
        })
    }

    /// Assert this tree's identifier equals a trusted one obtained out-of-band
    /// (spec section 6 step 1). A tree forged for different data has a different
    /// identifier and is rejected here, closing the gap that `validate` alone —
    /// which only checks the tree's *own* self-consistency — leaves open.
    pub fn check_against(&self, trusted_identifier: &str) -> Result<(), String> {
        if self.identifier != trusted_identifier {
            return Err(format!(
                "identifier mismatch: tree is {}, expected {}",
                self.identifier, trusted_identifier
            ));
        }
        Ok(())
    }

    /// The hash-file blocks `validate` reads to authenticate the byte range
    /// `[start, end)` — one per layer along each touched leaf's path (spec
    /// section 6 path note). Returns `(layer, group_start_index, group_len_hashes)`
    /// entries, deduplicated. This is structural (no data, no `.blocks` I/O), and
    /// uses the same `group_start` arithmetic as `validate`'s climb, so it is an
    /// exact account of `validate`'s `.blocks` access pattern. A single-leaf tree
    /// and empty/empty-range inputs read no hash-file blocks.
    pub fn path_blocks(
        &self,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<Vec<(usize, u64, usize)>, String> {
        let start = start.unwrap_or(0);
        let end = end.unwrap_or(self.length);
        if start > end || end > self.length {
            return Err(format!(
                "range {}..{} out of bounds for length {}",
                start, end, self.length
            ));
        }
        if self.length == 0 || end == start || self.counts[0] == 1 {
            return Ok(Vec::new());
        }
        let nlayers = self.counts.len();
        let b_lo = start / BLOCK as u64;
        let b_hi = (end - 1) / BLOCK as u64;
        let mut blocks: Vec<(usize, u64, usize)> = Vec::new();
        for i in b_lo..=b_hi {
            let mut idx = i;
            for (l, count) in self.counts.iter().enumerate().take(nlayers) {
                let gstart = group_start(idx);
                let len = (count - gstart).min(FANOUT as u64) as usize;
                let entry = (l, gstart, len);
                if !blocks.contains(&entry) {
                    blocks.push(entry);
                }
                idx /= FANOUT as u64;
            }
        }
        Ok(blocks)
    }

    fn root(&self) -> Result<[u8; 32], String> {
        let raw = hex_to_32(&self.tree_hex).ok_or("head: tree not 64 hex")?;
        Ok(raw)
    }

    /// Verify the header binds to its identifier: `G(manifest) == identifier`
    /// (spec section 6 step 2). This anchors trust in the tree root.
    fn check_identifier(&self) -> Result<[u8; 32], String> {
        let root = self.root()?;
        let recomputed = identifier_from_parts(self.length, &root);
        if recomputed != self.identifier {
            return Err("tree: identifier does not match manifest".into());
        }
        // Also confirm the manifest is itself canonical/parseable.
        let _ = manifest_bytes(self.length, &self.tree_hex);
        Ok(root)
    }

    fn read_blocks_slice(&self, byte_off: u64, len: usize) -> Result<Vec<u8>, String> {
        let mut f = File::open(&self.blocks_path)
            .map_err(|e| format!("cannot open {}: {}", self.blocks_path.display(), e))?;
        f.seek(SeekFrom::Start(byte_off))
            .map_err(|e| format!("blocks seek: {}", e))?;
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf)
            .map_err(|e| format!("blocks truncated: {}", e))?;
        Ok(buf)
    }

    /// Read the hash-file group at `layer` starting at hash index `group_start`.
    fn read_group(&self, layer: usize, group_start: u64) -> Result<Vec<u8>, String> {
        let remaining = self.counts[layer] - group_start;
        let len_hashes = remaining.min(FANOUT as u64) as usize;
        let byte_off = self.offsets[layer] + group_start * 32;
        self.read_blocks_slice(byte_off, len_hashes * 32)
    }

    /// Validate the byte range `[start, end)` of `data_path` against the tree,
    /// optionally streaming the verified bytes to `writer`. With `start`/`end`
    /// `None`, validates/streams the whole dataset.
    pub fn validate(
        &self,
        data_path: &Path,
        start: Option<u64>,
        end: Option<u64>,
        mut writer: Option<&mut dyn Write>,
    ) -> Result<(), String> {
        let root = self.check_identifier()?;

        let start = start.unwrap_or(0);
        let end = end.unwrap_or(self.length);
        if start > end || end > self.length {
            return Err(format!(
                "range {}..{} out of bounds for length {}",
                start, end, self.length
            ));
        }

        let mut data = File::open(data_path)
            .map_err(|e| format!("cannot open {}: {}", data_path.display(), e))?;
        let data_len = data
            .metadata()
            .map_err(|e| format!("stat data: {}", e))?
            .len();
        if data_len != self.length {
            return Err(format!(
                "data length {} != tree length {}",
                data_len, self.length
            ));
        }

        // Empty dataset: a single empty leaf; nothing to stream.
        if self.length == 0 {
            if g(b"") != root {
                return Err("validation failed: empty dataset root mismatch".into());
            }
            return Ok(());
        }

        // Empty range: header already verified, no blocks to walk.
        if end == start {
            return Ok(());
        }

        let single_leaf = self.counts[0] == 1;
        let nlayers = self.counts.len();
        // Per-layer cache: (group_start_index, group_bytes, node = g(group)).
        let mut cache: Vec<GroupCache> = vec![None; nlayers];

        let b_lo = start / BLOCK as u64;
        let b_hi = (end - 1) / BLOCK as u64;

        for i in b_lo..=b_hi {
            let block_off = i * BLOCK as u64;
            let block_len = (self.length - block_off).min(BLOCK as u64) as usize;
            let mut buf = vec![0u8; block_len];
            data.seek(SeekFrom::Start(block_off))
                .map_err(|e| format!("data seek: {}", e))?;
            data.read_exact(&mut buf)
                .map_err(|e| format!("data read: {}", e))?;

            let mut h = g(&buf);

            if single_leaf {
                if h != root {
                    return Err(format!("validation failed at block {}", i));
                }
            } else {
                let mut idx = i;
                for (l, slot) in cache.iter_mut().enumerate().take(nlayers) {
                    let gstart = group_start(idx);
                    let posn = (idx - gstart) as usize;

                    let need_reload = match slot {
                        Some((gs, _, _)) => *gs != gstart,
                        None => true,
                    };
                    if need_reload {
                        let bytes = self.read_group(l, gstart)?;
                        let node = g(&bytes);
                        *slot = Some((gstart, bytes, node));
                    }
                    let (_, bytes, node) = slot.as_ref().unwrap();
                    if bytes[posn * 32..posn * 32 + 32] != h[..] {
                        return Err(format!(
                            "validation failed at block {} (layer {})",
                            i, l
                        ));
                    }
                    h = *node;
                    idx /= FANOUT as u64;
                }
                if h != root {
                    return Err(format!("validation failed at block {} (root)", i));
                }
            }

            if let Some(w) = writer.as_mut() {
                let s = start.max(block_off);
                let e = end.min(block_off + block_len as u64);
                if e > s {
                    let lo = (s - block_off) as usize;
                    let hi = (e - block_off) as usize;
                    w.write_all(&buf[lo..hi])
                        .map_err(|e| format!("write output: {}", e))?;
                }
            }
        }
        Ok(())
    }
}

fn with_ext(name: &Path, ext: &str) -> PathBuf {
    let mut s = name.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

fn hex_to_32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::TreeBuilder;
    use crate::manifest::identifier;

    fn build(data: &[u8]) -> BuiltTree {
        let mut b = TreeBuilder::new();
        if data.len() <= BLOCK {
            b.push_leaf(&g(data));
        } else {
            let mut i = 0;
            while i < data.len() {
                let end = (i + BLOCK).min(data.len());
                b.push_leaf(&g(&data[i..end]));
                i = end;
            }
        }
        b.build(data.len() as u64)
    }

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("terrapin-test-{}-{}", std::process::id(), name));
        p
    }

    // Verifies: REQ-OFF-001
    #[test]
    fn offsets_from_counts_alignment() {
        assert_eq!(offsets_from_counts(&[3]), vec![0]);
        let counts = vec![65537u64, 2];
        let offs = offsets_from_counts(&counts);
        assert_eq!(offs, vec![0, 65537 * 32]);
        for o in &offs {
            assert_eq!(o % 32, 0, "offset must be 32-byte aligned");
        }
        let total = offs.last().unwrap() + counts.last().unwrap() * 32;
        assert_eq!(total, (65537 + 2) * 32);
    }

    // Verifies: REQ-HEX-002
    #[test]
    fn hex_to_32_roundtrips_to_hex() {
        let mut raw = [0u8; 32];
        for (i, b) in raw.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        let h = crate::manifest::to_hex(&raw);
        assert_eq!(hex_to_32(&h), Some(raw));
    }

    // Verifies: REQ-HEX-003
    #[test]
    fn hex_to_32_rejects_bad_input() {
        assert_eq!(hex_to_32(&"0".repeat(63)), None);
        assert_eq!(hex_to_32(&"0".repeat(65)), None);
        assert_eq!(hex_to_32(""), None);
        let mut s = "0".repeat(64);
        s.replace_range(0..1, "g");
        assert_eq!(hex_to_32(&s), None);
    }

    // Verifies: REQ-VAL-002
    #[test]
    fn roundtrip_validate_and_ranges() {
        let len = 3 * BLOCK + 4242;
        let data: Vec<u8> = (0..len).map(|i| (i * 11 + 5) as u8).collect();
        let data_path = tmp("data.bin");
        std::fs::write(&data_path, &data).unwrap();

        let tree = build(&data);
        let base = tmp("tree");
        PersistedTree::write(&base, &tree).unwrap();

        let pt = PersistedTree::read(&base).unwrap();
        assert_eq!(pt.identifier, identifier(&data));

        // whole file
        pt.validate(&data_path, None, None, None).unwrap();
        // sub-block range
        pt.validate(&data_path, Some(10), Some(100), None).unwrap();
        // spanning multiple blocks
        pt.validate(&data_path, Some(BLOCK as u64 - 5), Some(2 * BLOCK as u64 + 5), None)
            .unwrap();
        // last partial block
        pt.validate(&data_path, Some(3 * BLOCK as u64), Some(len as u64), None)
            .unwrap();

        // cat a range and compare bytes
        let (s, e) = (BLOCK as u64 + 7, 2 * BLOCK as u64 + 9);
        let mut out = Vec::new();
        pt.validate(&data_path, Some(s), Some(e), Some(&mut out)).unwrap();
        assert_eq!(out, &data[s as usize..e as usize]);

        // tamper: flip one byte -> validation fails
        let mut bad = data.clone();
        bad[2 * BLOCK + 1] ^= 0xff;
        let bad_path = tmp("bad.bin");
        std::fs::write(&bad_path, &bad).unwrap();
        assert!(pt.validate(&bad_path, None, None, None).is_err());

        let _ = std::fs::remove_file(&data_path);
        let _ = std::fs::remove_file(&bad_path);
        let _ = std::fs::remove_file(with_ext(&base, "head"));
        let _ = std::fs::remove_file(with_ext(&base, "blocks"));
    }

    // Verifies: REQ-VAL-003
    #[test]
    fn empty_dataset() {
        let data_path = tmp("empty.bin");
        std::fs::write(&data_path, b"").unwrap();
        let tree = build(b"");
        let base = tmp("empty-tree");
        PersistedTree::write(&base, &tree).unwrap();
        let pt = PersistedTree::read(&base).unwrap();
        assert_eq!(pt.identifier, identifier(b""));
        pt.validate(&data_path, None, None, None).unwrap();

        let _ = std::fs::remove_file(&data_path);
        let _ = std::fs::remove_file(with_ext(&base, "head"));
        let _ = std::fs::remove_file(with_ext(&base, "blocks"));
    }

    // Verifies: REQ-VF-002
    #[test]
    fn corrupt_head_rejected() {
        let data: Vec<u8> = vec![7u8; BLOCK + 10];
        let tree = build(&data);
        let base = tmp("corrupt-tree");
        PersistedTree::write(&base, &tree).unwrap();

        // Corrupt the tree root hex in the head -> identifier binding fails.
        let head_path = with_ext(&base, "head");
        let text = std::fs::read_to_string(&head_path).unwrap();
        let corrupted = text.replace(&tree.tree_hex(), &"0".repeat(64));
        std::fs::write(&head_path, corrupted).unwrap();

        let data_path = tmp("corrupt-data.bin");
        std::fs::write(&data_path, &data).unwrap();
        let pt = PersistedTree::read(&base).unwrap();
        assert!(pt.validate(&data_path, None, None, None).is_err());

        let _ = std::fs::remove_file(&data_path);
        let _ = std::fs::remove_file(&head_path);
        let _ = std::fs::remove_file(with_ext(&base, "blocks"));
    }
}
