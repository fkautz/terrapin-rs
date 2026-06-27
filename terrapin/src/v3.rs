//! Terrapin v0.3 (profile terrapin-sha256).
//!
//! The identifier is `G(canonical root manifest)`, NOT the bare recursive tree
//! root (TERRAPIN-3). Block size is pinned to exactly 2,097,152 bytes and
//! layering is a total function of length (no optional layer-skipping). This is
//! a breaking change from the v0.2 streaming API in `lib.rs`, which is retained
//! for migration only.

use gitoid::boringssl::Sha256;
use gitoid::{Blob, GitOid};

/// Exact Terrapin block size (2 MiB, not 2,000,000).
pub const BLOCK: usize = 2097152;

/// GitOID SHA-256: sha256("blob " + decimal(len) + "\0" + data).
pub fn g(data: &[u8]) -> [u8; 32] {
    let gid = GitOid::<Sha256, Blob>::id_bytes(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(gid.as_bytes());
    out
}

/// Recursive tree root T(data). Total recursion; no skipped layers. Intermediate
/// value, not an identifier.
pub fn tree_root(data: &[u8]) -> [u8; 32] {
    if data.len() <= BLOCK {
        return g(data);
    }
    let mut hash_file = Vec::with_capacity(data.len().div_ceil(BLOCK) * 32);
    let mut i = 0;
    while i < data.len() {
        let end = std::cmp::min(i + BLOCK, data.len());
        hash_file.extend_from_slice(&g(&data[i..end]));
        i = end;
    }
    tree_root(&hash_file)
}

fn to_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}

/// Canonical root manifest bytes; every line including the last is LF-terminated.
pub fn manifest_bytes(length: u64, tree_hex: &str) -> Vec<u8> {
    format!(
        "terrapin: sha256\nblock_size: {}\nlength: {}\ntree: {}\n",
        BLOCK, length, tree_hex
    )
    .into_bytes()
}

/// Terrapin v0.3 identifier "terrapin-sha256:<64 hex>" = G(canonical manifest).
pub fn identifier(data: &[u8]) -> String {
    let tree = tree_root(data);
    let id = g(&manifest_bytes(data.len() as u64, &to_hex(&tree)));
    format!("terrapin-sha256:{}", to_hex(&id))
}

/// Validate and parse a canonical root manifest. Non-canonical manifests are
/// rejected (not normalized).
pub fn parse_manifest(b: &[u8]) -> Result<(u64, String), String> {
    let s = std::str::from_utf8(b).map_err(|_| "manifest: non-utf8".to_string())?;
    if !s.ends_with('\n') {
        return Err("manifest: missing final LF".into());
    }
    let lines: Vec<&str> = s.split('\n').collect();
    if lines.len() != 5 || !lines[4].is_empty() {
        return Err("manifest: must be exactly 4 LF-terminated lines".into());
    }
    let keys = ["terrapin", "block_size", "length", "tree"];
    let mut vals: Vec<&str> = Vec::with_capacity(4);
    for (i, key) in keys.iter().enumerate() {
        let prefix = format!("{}: ", key);
        let line = lines[i];
        if !line.starts_with(&prefix) {
            return Err(format!("manifest: line {} bad prefix", i));
        }
        let v = &line[prefix.len()..];
        if v != v.trim() {
            return Err(format!("manifest: extra whitespace in line {}", i));
        }
        vals.push(v);
    }
    if vals[0] != "sha256" {
        return Err("manifest: algorithm must be sha256".into());
    }
    if vals[1] != BLOCK.to_string() {
        return Err("manifest: block_size must be 2097152".into());
    }
    if !is_canonical_decimal(vals[2]) {
        return Err("manifest: length not canonical decimal".into());
    }
    if !is_lower_hex64(vals[3]) {
        return Err("manifest: tree must be 64 lowercase hex".into());
    }
    let n: u64 = vals[2].parse().map_err(|_| "manifest: length parse".to_string())?;
    Ok((n, vals[3].to_string()))
}

fn is_canonical_decimal(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s == "0" {
        return true;
    }
    if s.starts_with('0') {
        return false;
    }
    s.bytes().all(|c| c.is_ascii_digit())
}

fn is_lower_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Golden vectors from the LLIFS conformance oracle (vectors-terrapin.json).
    const V: &[(&str, u64, &str, &str)] = &[
        ("one-zero-byte", 1,
         "449e9b795420cd16fe60ad5298cf680f15a7cd2ac9b44adaf7ed3edc0d08dd78",
         "dce39f984d9c140e4ad8f4b448a2ae6ae5398ed1adbb4d07ed8bedbc5b3b4598"),
        ("block-minus-1", (BLOCK - 1) as u64,
         "1024ef65054fcdb76a56b6fe00712dbc0007be8c65ee3902fa6c6b8c2fd7f09f",
         "dc7f0a33cf02e7a84fc380a41d396b451c96325a633a87528ebf797621befad7"),
        ("exactly-one-block", BLOCK as u64,
         "67cbed9b97ddabde2863f4daefa4f57176567a7c3ccfa1560c1065f9c8af74d6",
         "6fbd6447c2d8d70a83ae159461847a1a410679900702433dd2b04d063a3b2f9b"),
        ("block-plus-1", (BLOCK + 1) as u64,
         "18010af5fe70aa45e486608a97516f30410dc75c934c2486f985494990b54602",
         "5ba8049ae8f68a47acd4fad265c8a963aa82735e90f209dd79ff8d6d2188fdc5"),
    ];

    #[test]
    fn g_empty_is_git_empty_blob() {
        assert_eq!(
            to_hex(&g(b"")),
            "473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813"
        );
    }

    #[test]
    fn explicit_vectors() {
        assert_eq!(
            identifier(b""),
            "terrapin-sha256:f4b8abc1cfd6ffec75b4070be5440706286b3a7af937ef5d020ca2c0c1210458"
        );
        assert_eq!(
            identifier(b"hello world"),
            "terrapin-sha256:7bc0163f32e5f6082308ae0dff3dc7c9b0488e5aa652d9de01418df5ec800c8c"
        );
    }

    #[test]
    fn zero_data_vectors() {
        for (name, len, tree_hex, id_hex) in V {
            let data = vec![0u8; *len as usize];
            assert_eq!(to_hex(&tree_root(&data)), *tree_hex, "{} tree", name);
            assert_eq!(identifier(&data), format!("terrapin-sha256:{}", id_hex), "{} id", name);
        }
    }

    // T(n zero bytes) without materializing n bytes (128 GiB boundary vectors).
    fn tree_root_zero(n: u64) -> [u8; 32] {
        if n <= BLOCK as u64 {
            return tree_root(&vec![0u8; n as usize]);
        }
        let full = n / BLOCK as u64;
        let rem = (n % BLOCK as u64) as usize;
        let leaf = g(&vec![0u8; BLOCK]);
        let mut hf = Vec::with_capacity(((full as usize) + 1) * 32);
        for _ in 0..full {
            hf.extend_from_slice(&leaf);
        }
        if rem > 0 {
            hf.extend_from_slice(&g(&vec![0u8; rem]));
        }
        tree_root(&hf)
    }

    #[test]
    fn recursion_boundary_vectors() {
        let cases: &[(&str, u64, &str, &str)] = &[
            ("65536-full-blocks", 65536 * BLOCK as u64,
             "9e7e7e12b71c2b008302a4e4f5abe5b012025a8bd59d9ea5aa187f187a165599",
             "8d03319328c6d6b3cd00566d894443b2a82d31437b580ee533c2021d82bdb5a4"),
            ("65536-full-blocks-plus-1-byte", 65536 * BLOCK as u64 + 1,
             "73a1fd09b7b403e607c6ae58d0a4b0ac774d2b8519289ce3a1c85dbad6683316",
             "6f552f944f4995878c7facc92c29c3643aaafc2a5bff90e255bbf430210d551b"),
        ];
        for (name, len, tree_hex, id_hex) in cases {
            let tree = tree_root_zero(*len);
            assert_eq!(to_hex(&tree), *tree_hex, "{} tree", name);
            let id = g(&manifest_bytes(*len, &to_hex(&tree)));
            assert_eq!(to_hex(&id), *id_hex, "{} id", name);
        }
    }

    #[test]
    fn manifest_accept_reject() {
        let tree = "fee53a18d32820613c0527aa79be5cb30173c823a9b448fa4817767cc84c6f03";
        let good = manifest_bytes(11, tree);
        assert!(parse_manifest(&good).is_ok());

        let rejects: Vec<Vec<u8>> = vec![
            manifest_bytes(11, "FEE53A18d32820613c0527aa79be5cb30173c823a9b448fa4817767cc84c6f03"),
            format!("terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: {}", tree).into_bytes(), // no final LF
            format!("block_size: 2097152\nterrapin: sha256\nlength: 11\ntree: {}\n", tree).into_bytes(), // order
            format!("terrapin: sha256\nblock_size: 2097152\nlength: 011\ntree: {}\n", tree).into_bytes(), // leading zero
            format!("terrapin: sha256\nblock_size: 2097152\nlength:  11\ntree: {}\n", tree).into_bytes(), // double space
            format!("terrapin: sha256\nblock_size: 2000000\nlength: 11\ntree: {}\n", tree).into_bytes(), // block size
            manifest_bytes(11, "abcd"), // short tree
            format!("terrapin: sha256\nblock_size: 2097152\nlength: 11\ntree: {}\nextra: x\n", tree).into_bytes(), // extra key
        ];
        for (i, b) in rejects.iter().enumerate() {
            assert!(parse_manifest(b).is_err(), "reject case {} wrongly accepted", i);
        }
    }
}
