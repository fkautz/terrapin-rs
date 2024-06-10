use std::error::Error;
use std::io;
use std::io::{BufReader, Read};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use gitoid::{Blob, GitOid};
use gitoid::boringssl::Sha256;

#[derive(Debug)]
pub struct BufferOverflowError;

impl std::fmt::Display for BufferOverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Buffer size exceeded 2MB unexpectedly")
    }
}

impl Error for BufferOverflowError {}

pub struct Terrapin {
    attestations : Vec<u8>,
    buffer: Vec<u8>,
    finalized: bool,
}

pub const BUFFER_CAPACITY: usize = 1024 * 1024 * 2; // 2MB buffer capacity

impl Terrapin {
    pub fn new() -> Terrapin {
        Terrapin{
            attestations: vec![],
            buffer: Vec::with_capacity(1024*1024*2),
            finalized: false,
        }
    }


    fn update_hash_buffer(&mut self) {
        if self.buffer.len() == 0 {
            return
        }
        let gid = GitOid::<Sha256, Blob>::id_bytes(self.buffer.as_slice());
        let hash = gid.as_bytes();

        self.attestations.extend(hash.to_vec());

        // Reset buffer and hasher for the next round
        self.buffer.clear();
    }

    pub fn add(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {

        if self.finalized {
            return Err(Box::new(FinalizedError{}));
        }

        let mut copied: usize = 0;
        while copied < data.len() {
            let to_copy = std::cmp::min(data.len() - copied, BUFFER_CAPACITY - self.buffer.len());
            let chunk = &data[copied..copied + to_copy];
            self.buffer.extend_from_slice(chunk);
            copied += to_copy;

            if self.buffer.len() >= BUFFER_CAPACITY {
                self.update_hash_buffer();
            } else if self.buffer.len() > BUFFER_CAPACITY {
                // This condition should never be true because of how to_copy is calculated
                return Err(Box::new(BufferOverflowError));
            }
        }

        Ok(())
    }


    pub fn finalize(&mut self) -> Vec<u8> {
        if !self.finalized {
            self.update_hash_buffer();
            self.finalized = true;
        }
        self.attestations.clone()
    }
}

#[derive(Debug)]
pub struct FinalizedError {
}

impl Error for FinalizedError{}

impl std::fmt::Display for FinalizedError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "terrapin attestor already finalized")
    }
}

struct ChunkReader<R> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: Read> ChunkReader<R> {
    fn new(reader: R, capacity: usize) -> ChunkReader<R> {
        ChunkReader {
            reader,
            buffer: vec![0; capacity],
        }
    }
}

impl<R: Read> Iterator for ChunkReader<R> {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read(&mut self.buffer) {
            Ok(0) => None,
            Ok(n) => Some(Ok(self.buffer[..n].to_vec())),
            Err(e) => Some(Err(e)),
        }
    }
}

#[derive(Debug)]
pub struct InvalidChunkSizeError;

impl Error for InvalidChunkSizeError {}

impl std::fmt::Display for InvalidChunkSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "chunk size cannot be zero")
    }
}

pub async fn new_writer(reader: BufReader<Box<dyn Read>>, writer: Sender<Vec<u8>>, chunk_size: usize) -> Result<(), Box<dyn Error>> {
    let chunk_reader = ChunkReader::new(reader, chunk_size);

    let handles = chunk_reader.map(|chunk| {
        tokio::spawn(async move {
            let chunk = chunk.expect("");
            let data_gitoid = GitOid::<Sha256, Blob>::id_bytes(chunk.as_slice());
            data_gitoid.as_bytes().to_vec()
        })
    });

    let results = futures::future::join_all (handles).await;
    for res in results {
            match res {
                Ok(bytes) => {
                    // let res = writer.write(bytes).expect("write everything!");
                    writer.send(bytes).expect("TODO: panic message")
                },
                Err(_) => {
                    panic!("gitoid generation failed")
                }
            }
    };


    return Ok(());
}

pub async fn generate_for_reader(reader: Box<dyn Read>, chunk_size: usize, _expected_reader_length: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if chunk_size == 0 {
        return Err(Box::new(InvalidChunkSizeError));
    }

    let reader = BufReader::new(reader);

    let (tx, rx) = mpsc::channel();
    let root = new_writer(reader, tx.clone(), chunk_size);
    drop(tx);

    root.await.expect("should work");

    let mut result : Vec<u8> = vec![];

    for x in rx {
        // println!("collect: {}", x.len());
        result.extend(x);
        // println!("result len: {}", result.len())
    }

    Ok(result)
}

    #[cfg(test)]
    mod tests {
        use std::fs::File;
        use std::os::unix::fs::MetadataExt;
        use std::path::PathBuf;
        use super::*;

        #[test]
        fn new_terrapin() {
            let terrapin = Terrapin::new();
            assert_eq!(terrapin.attestations.len(), 0);
            assert_eq!(terrapin.buffer.len(), 0);
            assert_eq!(terrapin.finalized, false);
        }

        #[test]
        fn add_data() {
            let mut terrapin = Terrapin::new();
            let data = vec![1, 2, 3, 4, 5];
            assert!(terrapin.add(&data).is_ok());
            assert_eq!(terrapin.buffer.len(), data.len());
        }

        #[test]
        fn add_data_when_finalized() {
            let mut terrapin = Terrapin::new();
            terrapin.finalize();
            let data = vec![1, 2, 3, 4, 5];
            let x = terrapin.add(&data).is_err();
            assert!(x);
        }

        #[test]
        fn finalize() {
            let mut terrapin = Terrapin::new();
            let data = vec![1, 2, 3, 4, 5];
            let data_gitoid = GitOid::<Sha256, Blob>::id_bytes(data.as_slice());
            let data_hash = data_gitoid.as_bytes();
            let result = terrapin.add(&data);
            assert!(result.is_ok(), "Adding data to terrapin failed");
            let attestation = terrapin.finalize();
            assert_eq!(attestation.len(), data_hash.len(), "Hash length mismatch");
            assert_eq!(
                &attestation[..data_hash.len()],
                data_hash,
                "Hashed data does not match original data"
            );
        }

        #[test]
        fn finalize_when_already_finalized() {
            let mut terrapin = Terrapin::new();
            let attestation1 = terrapin.finalize();
            let attestation2 = terrapin.finalize();
            assert_eq!(attestation1, attestation2);
        }

        #[tokio::test]
        async fn generate_for_file_with_zero_chunk_size() {
            let path = PathBuf::from("test_data/hello.txt");
            // println!("{:?}", path);
            let reader = File::open(path).expect("file should open");
            let size = reader.metadata().unwrap().size();
            let result = generate_for_reader(Box::new(reader), 0, size).await;
            assert!(result.is_err());
            if let Err(e) = result {
                assert_eq!(e.to_string(), "chunk size cannot be zero");
            }
        }

        #[tokio::test]
        async fn test_small_pin_generated_properly() {
            let path_data = PathBuf::from("test_data/hello.txt");
            let reader = File::open(path_data).expect("file should open");
            let size = reader.metadata().unwrap().size();
            let result = generate_for_reader(Box::new(reader), 2*1024*1024, size).await;
            let observed_pin = result.unwrap();
            let mut pin_file = File::open("test_data/hello.txt.pin").expect("test pin file should open");
            let mut expected_pin = Vec::new();
            pin_file.read_to_end(&mut expected_pin).expect("failed to read expected pin file");
            assert_eq!(expected_pin, observed_pin)
        }
    }
