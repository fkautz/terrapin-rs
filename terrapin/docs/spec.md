# Terrapin Data Structure and Algorithm Specification

## Overview

Terrapin is a system designed to create and verify data attestations. It processes data in chunks, computes cryptographic hashes for each chunk, and stores these hashes as attestations. The system ensures data integrity by allowing verification of the data against the stored attestations.

## Data Structures

### Terrapin

The `Terrapin` data structure is the core component of the system. It maintains the following fields:

- `attestations`: A vector of bytes that stores the cryptographic hashes of the data chunks.
- `buffer`: A vector of bytes that temporarily holds data until it reaches a specified capacity.
- `finalized`: A boolean flag indicating whether the attestation process is complete.

### Constants

- `BUFFER_CAPACITY`: The maximum size of the buffer, set to 2MB (2 * 1024 * 1024 bytes).

### Errors

- `BufferOverflowError`: An error indicating that the buffer size has exceeded the specified capacity.
- `FinalizedError`: An error indicating that the attestation process has already been finalized.
- `InvalidChunkSizeError`: An error indicating that the specified chunk size is zero.

## Algorithm

### Initialization

1. Create a new `Terrapin` instance with an empty `attestations` vector, a buffer with a capacity of 2MB, and the `finalized` flag set to `false`.

### Adding Data

1. Check if the `finalized` flag is `true`. If it is, return a `FinalizedError`.
2. Initialize a variable `copied` to track the number of bytes processed.
3. While `copied` is less than the length of the input data:
   - Calculate the number of bytes to copy (`to_copy`) as the minimum of the remaining data length and the remaining buffer capacity.
   - Copy the data chunk into the buffer.
   - Update the `copied` variable.
   - If the buffer reaches its capacity, compute the hash of the buffer contents and store it in the `attestations` vector. Then, clear the buffer.

### Finalizing

1. If the `finalized` flag is `false`, compute the hash of any remaining data in the buffer and store it in the `attestations` vector.
2. Set the `finalized` flag to `true`.
3. Return a clone of the `attestations` vector.

### ChunkReader

The `ChunkReader` is an iterator that reads data in chunks from a given reader. It maintains the following fields:

- `reader`: The data source to read from.
- `buffer`: A buffer to hold the read data.

### Generating Attestations for a Reader

1. Create a `ChunkReader` with the specified chunk size.
2. For each chunk read by the `ChunkReader`:
   - Compute the cryptographic hash of the chunk.
   - Send the hash to a channel.
3. Collect the hashes from the channel and return them as a vector of bytes.

### Validating Data

1. Open the input file and read the attestations file.
2. Create a new `Terrapin` instance.
3. Calculate the aligned start and end positions based on the specified range and buffer capacity.
4. Seek to the aligned start position in the input file.
5. Read data in chunks and add it to the `Terrapin` instance.
6. If a writer is provided, write the data chunks to the writer.
7. Compute the final attestations and compare them with the stored attestations.
8. Print a success message if the data matches the attestations, otherwise print an error message.

## Conclusion

The Terrapin system provides a robust mechanism for creating and verifying data attestations. By processing data in chunks and computing cryptographic hashes, it ensures data integrity and allows for efficient verification.
