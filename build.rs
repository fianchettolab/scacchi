//! Reads the shakmaty version linked into the library from Cargo.lock, so
//! `shakmaty_version()` reports what was built, not a hand-written copy.

use std::{env, fs, path::Path};

fn main() {
    let lock = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let lock = fs::read_to_string(&lock).expect("Cargo.lock is missing");

    let mut versions = lock
        .split("[[package]]")
        .filter(|package| package.lines().any(|line| line.trim() == r#"name = "shakmaty""#))
        .filter_map(|package| {
            package.lines().find_map(|line| line.trim().strip_prefix("version = \"")?.strip_suffix('"'))
        });
    let version = versions.next().expect("Cargo.lock has no shakmaty");
    assert!(versions.next().is_none(), "Cargo.lock has more than one shakmaty");
    println!("cargo:rustc-env=SCACCHI_SHAKMATY_VERSION={version}");
}
