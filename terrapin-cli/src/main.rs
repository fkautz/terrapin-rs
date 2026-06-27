use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::exit;

use structopt::StructOpt;
use terrapin::{build_from_reader, identifier_from_reader, PersistedTree};

#[derive(StructOpt)]
#[structopt(
    name = "terrapin",
    about = "Parallel content addressing and slice validation for very large datasets."
)]
enum Command {
    /// Print the terrapin-sha256 identifier of a file.
    Id {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
    },
    /// Build and write the publishable tree (<out>.head + <out>.blocks) and
    /// print the identifier.
    Attest {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        /// Output base name (default: <input>.terra).
        #[structopt(long, parse(from_os_str))]
        out: Option<PathBuf>,
    },
    /// Validate a file (or a byte range) against a published tree.
    Validate {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        /// Tree base name (the <name> of <name>.head / <name>.blocks).
        #[structopt(long, parse(from_os_str))]
        tree: PathBuf,
        /// Trusted identifier (terrapin-sha256:...); the tree must match it.
        #[structopt(long)]
        identifier: Option<String>,
        #[structopt(long)]
        start: Option<u64>,
        #[structopt(long)]
        end: Option<u64>,
    },
    /// Validate then stream the verified bytes (or a byte range) to stdout.
    Cat {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        #[structopt(long, parse(from_os_str))]
        tree: PathBuf,
        #[structopt(long)]
        start: Option<u64>,
        #[structopt(long)]
        end: Option<u64>,
    },
}

#[tokio::main]
async fn main() {
    match Command::from_args() {
        Command::Id { input } => {
            let reader = open(&input);
            let id = identifier_from_reader(reader)
                .await
                .unwrap_or_else(|e| fail(&format!("hashing failed: {}", e)));
            println!("{}", id);
        }
        Command::Attest { input, out } => {
            let reader = open(&input);
            let tree = build_from_reader(reader)
                .await
                .unwrap_or_else(|e| fail(&format!("hashing failed: {}", e)));
            let base = out.unwrap_or_else(|| with_terra(&input));
            PersistedTree::write(&base, &tree)
                .unwrap_or_else(|e| fail(&format!("writing tree failed: {}", e)));
            println!("{}", tree.identifier());
        }
        Command::Validate {
            input,
            tree,
            identifier,
            start,
            end,
        } => {
            let pt = PersistedTree::read(&tree).unwrap_or_else(|e| fail(&e));
            if let Some(trusted) = identifier.as_deref() {
                if let Err(e) = pt.check_against(trusted) {
                    eprintln!("Validation failed: {}", e);
                    exit(1);
                }
            }
            match pt.validate(&input, start, end, None) {
                Ok(()) => println!("Validation successful: the data matches the tree."),
                Err(e) => {
                    eprintln!("Validation failed: {}", e);
                    exit(1);
                }
            }
        }
        Command::Cat {
            input,
            tree,
            start,
            end,
        } => {
            let pt = PersistedTree::read(&tree).unwrap_or_else(|e| fail(&e));
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            if let Err(e) = pt.validate(&input, start, end, Some(&mut handle)) {
                eprintln!("Validation failed: {}", e);
                exit(1);
            }
            let _ = handle.flush();
        }
    }
}

fn open(path: &Path) -> File {
    File::open(path).unwrap_or_else(|e| fail(&format!("cannot open {}: {}", path.display(), e)))
}

fn with_terra(input: &Path) -> PathBuf {
    let mut s = input.as_os_str().to_os_string();
    s.push(".terra");
    PathBuf::from(s)
}

fn fail(msg: &str) -> ! {
    eprintln!("{}", msg);
    exit(1);
}
