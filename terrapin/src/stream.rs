//! Streaming, parallel construction from a reader.
//!
//! Data blocks are read sequentially at exact `BLOCK` boundaries and hashed on
//! the blocking thread pool with bounded, order-preserving concurrency, then fed
//! to a [`TreeBuilder`]. The dataset itself is never held in memory; only up to
//! `parallelism` blocks are in flight plus the leaf hash file.

use std::io::{self, ErrorKind, Read};
use std::thread::available_parallelism;

use futures::stream::{self, StreamExt};

use crate::builder::{BuiltTree, TreeBuilder};
use crate::manifest::{g, BLOCK};

/// Reads a `Read` source into exact `BLOCK`-sized blocks (the final block may be
/// shorter). An empty source yields exactly one empty block, so the dataset is
/// treated as a single empty leaf (spec section 4.3, empty case).
struct BlockReader<R> {
    reader: R,
    finished: bool,
    emitted: bool,
}

impl<R: Read> BlockReader<R> {
    fn new(reader: R) -> Self {
        BlockReader {
            reader,
            finished: false,
            emitted: false,
        }
    }
}

impl<R: Read> Iterator for BlockReader<R> {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let mut buf = vec![0u8; BLOCK];
        let mut filled = 0;
        while filled < BLOCK {
            match self.reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.finished = true;
                    return Some(Err(e));
                }
            }
        }
        if filled == 0 {
            self.finished = true;
            if !self.emitted {
                self.emitted = true;
                return Some(Ok(Vec::new())); // empty dataset -> one empty leaf
            }
            return None;
        }
        self.emitted = true;
        buf.truncate(filled);
        Some(Ok(buf))
    }
}

fn parallelism() -> usize {
    available_parallelism().map(|n| n.get()).unwrap_or(4)
}

/// Build the full tree from a reader, hashing blocks in parallel.
pub async fn build_from_reader<R: Read + Send + 'static>(reader: R) -> io::Result<BuiltTree> {
    let n = parallelism();
    let mut hashes = stream::iter(BlockReader::new(reader))
        .map(|res| async move {
            let block = res?;
            let len = block.len();
            let h = tokio::task::spawn_blocking(move || g(&block))
                .await
                .map_err(io::Error::other)?;
            Ok::<(usize, [u8; 32]), io::Error>((len, h))
        })
        .buffered(n);

    let mut builder = TreeBuilder::new();
    let mut length: u64 = 0;
    while let Some(item) = hashes.next().await {
        let (len, h) = item?;
        length += len as u64;
        builder.push_leaf(&h);
    }
    Ok(builder.build(length))
}

/// Convenience: the `terrapin-sha256:<hex>` identifier of a reader.
pub async fn identifier_from_reader<R: Read + Send + 'static>(reader: R) -> io::Result<String> {
    Ok(build_from_reader(reader).await?.identifier())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::identifier;
    use std::io::Cursor;

    /// A reader that returns at most `chunk` bytes per `read`, to exercise short
    /// reads and block reassembly.
    struct Choppy {
        data: Vec<u8>,
        pos: usize,
        chunk: usize,
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

    // Verifies: REQ-SB-001
    #[tokio::test(flavor = "multi_thread")]
    async fn matches_in_memory_identifier() {
        for len in [0usize, 1, BLOCK - 1, BLOCK, BLOCK + 1, 2 * BLOCK, 3 * BLOCK + 7] {
            let data: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            let want = identifier(&data);
            let got = identifier_from_reader(Cursor::new(data.clone())).await.unwrap();
            assert_eq!(got, want, "whole-cursor len {}", len);
        }
    }

    // Verifies: REQ-SB-002
    #[tokio::test(flavor = "multi_thread")]
    async fn matches_under_short_reads() {
        let len = 2 * BLOCK + 12345;
        let data: Vec<u8> = (0..len).map(|i| (i * 17 + 3) as u8).collect();
        let want = identifier(&data);
        for chunk in [1usize, 7, 65536, BLOCK + 1] {
            let reader = Choppy {
                data: data.clone(),
                pos: 0,
                chunk,
            };
            let got = identifier_from_reader(reader).await.unwrap();
            assert_eq!(got, want, "chunk {}", chunk);
        }
    }
}
