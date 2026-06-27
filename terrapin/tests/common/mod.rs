//! Shared helpers for Terrapin integration tests. Offline / dependency-free.
#![allow(dead_code)]

use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use terrapin::{g, BuiltTree, TreeBuilder, BLOCK};

// ---------------------------------------------------------------------------
// Deterministic pseudo-random data (xorshift64*, no external crates).
// ---------------------------------------------------------------------------

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn next_u8(&mut self) -> u8 {
        (self.next_u64() >> 33) as u8
    }
    /// Uniform in `[0, n)` (n > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// Deterministic pseudo-random bytes of the given length.
pub fn fill(len: usize, seed: u64) -> Vec<u8> {
    let mut r = Rng::new(seed);
    (0..len).map(|_| r.next_u8()).collect()
}

// ---------------------------------------------------------------------------
// Readers for streaming-path tests.
// ---------------------------------------------------------------------------

/// Streams `remaining` zero bytes without allocating the dataset.
pub struct ZeroReader {
    pub remaining: u64,
}
impl Read for ZeroReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let n = (buf.len() as u64).min(self.remaining) as usize;
        for b in &mut buf[..n] {
            *b = 0;
        }
        self.remaining -= n as u64;
        Ok(n)
    }
}

/// Returns at most `chunk` bytes per `read` (exercises short reads / reassembly).
pub struct Choppy {
    pub data: Vec<u8>,
    pub pos: usize,
    pub chunk: usize,
}
impl Choppy {
    pub fn new(data: Vec<u8>, chunk: usize) -> Self {
        Choppy { data, pos: 0, chunk }
    }
}
impl Read for Choppy {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.data.len() - self.pos;
        let n = remaining.min(self.chunk).min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Yields one `ErrorKind::Interrupted` at first `read`, then behaves normally.
pub struct InterruptOnce {
    pub data: Vec<u8>,
    pub pos: usize,
    pub fired: bool,
}
impl InterruptOnce {
    pub fn new(data: Vec<u8>) -> Self {
        InterruptOnce {
            data,
            pos: 0,
            fired: false,
        }
    }
}
impl Read for InterruptOnce {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.fired {
            self.fired = true;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let remaining = self.data.len() - self.pos;
        let n = remaining.min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Delivers `ok_bytes` of data, then returns a hard error.
pub struct ErrAfter {
    pub data: Vec<u8>,
    pub pos: usize,
}
impl ErrAfter {
    pub fn new(ok_bytes: usize) -> Self {
        ErrAfter {
            data: vec![0xa5; ok_bytes],
            pos: 0,
        }
    }
}
impl Read for ErrAfter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.data.len() {
            return Err(io::Error::new(io::ErrorKind::Other, "boom"));
        }
        let remaining = self.data.len() - self.pos;
        let n = remaining.min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// Tree construction helper (mirrors the streaming feed, in-memory).
// ---------------------------------------------------------------------------

pub fn build_tree(data: &[u8]) -> BuiltTree {
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

// ---------------------------------------------------------------------------
// Temp files with cleanup.
// ---------------------------------------------------------------------------

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temp path that removes itself (and `.head`/`.blocks` siblings) on drop.
pub struct TmpPath(pub PathBuf);

impl TmpPath {
    pub fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("terrapin-it-{}-{}-{}", std::process::id(), tag, n));
        TmpPath(p)
    }
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
    pub fn with_ext(&self, ext: &str) -> PathBuf {
        let mut s = self.0.as_os_str().to_os_string();
        s.push(".");
        s.push(ext);
        PathBuf::from(s)
    }
}

impl Drop for TmpPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.with_ext("head"));
        let _ = std::fs::remove_file(self.with_ext("blocks"));
    }
}
