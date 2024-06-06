use gitoid::{GitOid, Sha256, Blob};


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

    pub fn add(&mut self, data: &[u8]) -> Result<(), FinalizedError> {
        if self.finalized {
            return Err(FinalizedError {})
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
                panic!("Buffer size exceeded 2MB unexpectedly");
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

impl std::fmt::Display for FinalizedError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "terrapin attestor already finalized")
    }
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

    #[cfg(test)]
    mod tests {
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
            assert!(matches!(terrapin.add(&data), Err(FinalizedError{})));
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
    }
