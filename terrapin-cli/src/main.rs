use std::cmp::min;
use std::fs::File;
use std::io::{self, Cursor, Read, Seek, Write};
use std::path::PathBuf;
use structopt::StructOpt;
use terrapin::{Terrapin, BUFFER_CAPACITY, generate_for_reader};

#[derive(StructOpt)]
#[structopt(name = "terrapin", about = "A tool for creating and verifying data attestations.")]
enum Command {
    Attest {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        #[structopt(parse(from_os_str))]
        output: Option<PathBuf>,
    },
    Validate {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        #[structopt(parse(from_os_str))]
        attestations: PathBuf,
        #[structopt(long)]
        start: Option<u64>,
        #[structopt(long)]
        end: Option<u64>,
    },
    Cat {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        #[structopt(parse(from_os_str))]
        attestations: PathBuf,
        #[structopt(long)]
        start: Option<u64>,
        #[structopt(long)]
        end: Option<u64>,
    },
}

#[tokio::main]
async fn main() {
    let command = Command::from_args();

    match command {
        Command::Attest { input, output } => {
            let file = File::open(input).expect("cannot open file");
            let file_size = file.metadata().unwrap().len();
            let mut reader : Box<dyn Read> = Box::new(file);

            let mut attestations: Vec<Vec<u8>> = vec![];

            loop {
                let attestation = generate_for_reader(reader, 2*1024*1024, file_size).await.expect("reading data failed");
                attestations.push(attestation.clone());
                if attestation.len() == 32 {
                    break
                }
                reader = Box::new(Cursor::new(attestation.clone()));
            }

            attestations.reverse();

            let mut counter = 0;
            if let Some(ref output) = output {
                for attestation in attestations.clone() {
                    let p = format!("{}.{}", output.display(), counter);
                    std::fs::write(p, &attestation).expect("Failed to write attestations");
                    counter = counter + 1;
                }
            } else {
                io::stdout().write_all(&attestations[attestations.len()-1]).expect("Failed to write to stdout");
            }

        }
        Command::Validate { input, attestations, start, end } => {
            validate(input, attestations, start, end, None);
        }
        Command::Cat { input, attestations, start, end } => {
            validate(input, attestations, start, end, Some(&mut io::stdout()));
        }
    }
}

fn validate(input: PathBuf, attestations: PathBuf, start: Option<u64>, end: Option<u64>, mut writer: Option<&mut dyn Write>) {
    let mut input_file = File::open(input).expect("Failed to open input file");
    let attestations = std::fs::read(attestations).expect("Failed to read attestations file");

    let mut terrapin = Terrapin::new();
    let mut buffer = vec![0; BUFFER_CAPACITY];

    let aligned_start = if let Some(start) = start {
        start - start % BUFFER_CAPACITY as u64
    } else {
        0
    };

    let file_size = input_file.metadata().expect("Failed to read file metadata").len();
    let aligned_end = if let Some(end) = end {
        let proposed_end = (end + BUFFER_CAPACITY as u64) - end % BUFFER_CAPACITY as u64;
        min(proposed_end, file_size)
    } else {
        file_size
    };

    input_file.seek(std::io::SeekFrom::Start(aligned_start)).expect("Failed to seek to start position");

    let mut total: usize = 0;
    let mut total_hashed: usize = 0;
    let mut block: u64 = 1;
    loop {
        let n = input_file.read(&mut buffer).expect("Failed to read file");
        if n == 0 {
            break;
        } else if total > aligned_end as usize {
            panic!("total read greater than aligned end")
        }


        total_hashed += &buffer[0..n].len();
        terrapin.add(&buffer[0..n]).expect("TODO: panic message");

        if let Some(ref mut writer) = writer {
            let start_byte: usize = if let Some(start) = start {
                start as usize % BUFFER_CAPACITY
            } else {
                0
            };

            let end_byte = n;
            writer.write_all(&buffer[start_byte..end_byte]).expect("Failed to write to writer");
        }

        total += n;
        block = block + 1;

        if total == (aligned_end - aligned_start) as usize {
            break
        };
    }

    let computed_attestations = terrapin.finalize();

    let first_block: usize = ((aligned_start / BUFFER_CAPACITY as u64) * 32) as usize;
    let mut last_block: usize = ((aligned_end / BUFFER_CAPACITY as u64) * 32) as usize;

    if last_block == 0 {
        last_block = 32;
    }

    let att_slice = &attestations[first_block..last_block];

    if computed_attestations == *att_slice {
        println!("Validation successful: The data matches the attestations.");
    } else {
        eprintln!("Validation failed: The data does not match the attestations.");
    }
}
