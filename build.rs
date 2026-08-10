//! Emits the tree-sitter grammar crate versions as compile-time env vars so the
//! binary can report them (`capabilities --json` / `--version`). Mirrors
//! calldiff's `readCachedPackageVersion` idea — grammar version visibility —
//! with no runtime download: grammars stay statically linked per the release
//! philosophy (pre-built binary from GitHub Releases, rule 8).
//!
//! Versions come from Cargo.lock (the *resolved* versions actually linked).
//! Cargo.lock always exists when building a binary crate; if it is ever
//! missing we emit `unknown` rather than a misleading declared requirement.

use std::collections::HashMap;
use std::fs;

/// (Cargo package name → env var name emitted via `cargo:rustc-env`).
const GRAMMAR_CRATES: [(&str, &str); 3] = [
    ("tree-sitter-kotlin-sg", "KOTLIN_LSP_GRAMMAR_KOTLIN"),
    ("tree-sitter-java", "KOTLIN_LSP_GRAMMAR_JAVA"),
    ("tree-sitter-swift", "KOTLIN_LSP_GRAMMAR_SWIFT"),
];

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");

    let versions = parse_lock_versions("Cargo.lock");
    for (pkg, var) in GRAMMAR_CRATES {
        let version = versions
            .get(pkg)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        println!("cargo:rustc-env={var}={version}");
    }
}

/// name → version from Cargo.lock `[[package]]` blocks (resolved versions).
fn parse_lock_versions(path: &str) -> HashMap<String, String> {
    let mut versions = HashMap::new();
    let lock = match fs::read_to_string(path) {
        Ok(lock) => lock,
        Err(_) => return versions,
    };
    for block in lock.split("[[package]]").skip(1) {
        let mut name: Option<&str> = None;
        let mut version: Option<&str> = None;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("name = ") {
                name = Some(rest.trim().trim_matches('"'));
            } else if let Some(rest) = line.strip_prefix("version = ") {
                version = Some(rest.trim().trim_matches('"'));
            }
            if name.is_some() && version.is_some() {
                break;
            }
        }
        if let (Some(name), Some(version)) = (name, version) {
            versions.insert(name.to_string(), version.to_string());
        }
    }
    versions
}
