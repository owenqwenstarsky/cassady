use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
const FNV_PRIME: u64 = 1_099_511_628_211;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let docs_dir = manifest_dir.join("docs");

    println!("cargo:rerun-if-changed={}", docs_dir.display());

    let mut hash = Fnv64::new();
    if docs_dir.exists() {
        if let Err(err) = hash_dir(&docs_dir, &docs_dir, &mut hash) {
            panic!(
                "failed to hash docs directory {}: {err}",
                docs_dir.display()
            );
        }
    }

    println!("cargo:rustc-env=CASS_DOCS_HASH={:016x}", hash.finish());
}

fn hash_dir(base: &Path, dir: &Path, hash: &mut Fnv64) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());

        let rel = path
            .strip_prefix(base)
            .expect("entry is below docs base")
            .to_string_lossy()
            .replace('\\', "/");
        hash.write(rel.as_bytes());
        hash.write(&[0]);

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            hash.write(&[1]);
            hash_dir(base, &path, hash)?;
        } else if file_type.is_file() {
            hash.write(&[2]);
            let mut file = fs::File::open(&path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            hash.write(&buf);
        }
    }

    Ok(())
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(FNV_OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}
