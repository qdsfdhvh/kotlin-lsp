//! CLI `doctor` subcommand — project diagnostics for agents.
//!
//! Checks:
//! - Source roots are auto-discovered
//! - Cache is fresh / available
//! - Library sources are extracted (`~/.kotlin-lsp/sources/`)
//! - No suspicious gitignored .kt/.java files
//! - Workspace root is set

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize)]
struct CheckResult {
    name: &'static str,
    status: &'static str, // "ok" | "warn" | "error"
    message: String,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(not(unix))]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
}

fn cache_dir() -> Option<PathBuf> {
    // XDG_CACHE_HOME or ~/.cache
    std::env::var("XDG_CACHE_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".cache")))
}

/// Count files recursively with the given extension.
fn count_files(dir: &Path, ext: &str) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|e| {
                    let e = e.to_string_lossy();
                    ext == &*e || ext.trim_start_matches('.') == &*e
                })
                .unwrap_or(false)
        })
        .count()
}

/// Compute total size of a directory in bytes.
fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Check if a tool is available on PATH.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(name);
            if full.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::metadata(&full).ok().and_then(|m| {
                        if m.permissions().mode() & 0o111 != 0 {
                            Some(full)
                        } else {
                            None
                        }
                    })
                }
                #[cfg(not(unix))]
                Some(full)
            } else {
                None
            }
        })
    })
}

/// Quick check if a path has a .git ancestor.
fn has_git_ancestor(path: &Path) -> bool {
    path.ancestors().any(|a| a.join(".git").exists())
}

// ─── Doctor runner ──────────────────────────────────────────────────────────

pub(crate) fn run_doctor(root: Option<&Path>, verbose: bool, json: bool) {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    if json {
        let mut results: Vec<CheckResult> = Vec::new();
        results.push(CheckResult {
            name: "workspace-root",
            status: if root.exists() { "ok" } else { "error" },
            message: format!("{}", root.display()),
        });
        if root.exists() {
            let kt = count_files(&root, ".kt");
            let java = count_files(&root, ".java");
            let swift = count_files(&root, ".swift");
            let total = kt + java + swift;
            results.push(CheckResult {
                name: "source-files",
                status: if total > 0 { "ok" } else { "warn" },
                message: format!(
                    "{} total ({} .kt, {} .java, {} .swift)",
                    total, kt, java, swift
                ),
            });
        }
        let has_cache = home()
            .zip(cache_dir())
            .map(|(h, c)| format!("home={}, cache={}", h.display(), c.display()))
            .unwrap_or_else(|| "not found".to_string());
        results.push(CheckResult {
            name: "cache",
            status: "ok",
            message: has_cache,
        });
        let ws_cache = crate::indexer::try_load_cache(&root);
        results.push(CheckResult {
            name: "workspace-index",
            status: if ws_cache.is_some() { "ok" } else { "error" },
            message: ws_cache
                .as_ref()
                .map(|c| format!("{} files, version {}", c.entries.len(), c.version()))
                .unwrap_or_else(|| {
                    format!(
                        "missing/corrupt/stale: {}",
                        crate::indexer::workspace_cache_path(&root).display()
                    )
                }),
        });
        results.push(CheckResult {
            name: "rg",
            status: if which("rg").is_some() { "ok" } else { "warn" },
            message: String::new(),
        });
        results.push(CheckResult {
            name: "fd",
            status: if which("fd").is_some() { "ok" } else { "warn" },
            message: String::new(),
        });
        results.push(CheckResult {
            name: "ktlint",
            status: if which("ktlint").is_some() {
                "ok"
            } else {
                "error"
            },
            message: if which("ktlint").is_some() {
                String::new()
            } else {
                "Install: brew install ktlint".to_string()
            },
        });
        let output = serde_json::json!({
            "workspace_root": root.to_string_lossy(),
            "checks": results,
        });
        println!("{}", serde_json::to_string_pretty(&output).expect("json"));
        // Exit code must mirror the text mode (issue #322): workspace root /
        // index failures and a missing rg fail the run; fd/ktlint/source-files
        // warnings are advisory only.
        let failed = results.iter().any(|c| match c.name {
            "workspace-root" | "workspace-index" => c.status == "error",
            "rg" => c.status != "ok",
            _ => false,
        });
        if failed {
            std::process::exit(1);
        }
        return;
    }

    let mut all_ok = true;

    // ── 1. Workspace root exists ─────────────────────────────────────────
    let root_exists = root.exists();
    if root_exists {
        println!("[✓] workspace root exists");
    } else {
        println!("[!] workspace root does not exist: {}", root.display());
        all_ok = false;
    }

    // ── 2. Kotlin/Java/Swift files found ────────────────────────────────
    if root_exists {
        let kt_files = count_files(&root, ".kt");
        let java_files = count_files(&root, ".java");
        let swift_files = count_files(&root, ".swift");
        let total = kt_files + java_files + swift_files;
        if total > 0 {
            println!(
                "[✓] {} source files found ({} .kt, {} .java, {} .swift)",
                total, kt_files, java_files, swift_files
            );
        } else {
            println!(
                "[!] no .kt, .java, or .swift files found under {}",
                root.display()
            );
        }
    }

    // ── 3. Library sources extracted ────────────────────────────────────
    let sources_dir = home().map(|h| h.join(".kotlin-lsp").join("sources"));
    let sources_extracted = match &sources_dir {
        Some(d) if d.exists() => {
            let jars = count_files(d, ".jar");
            // extract-sources unpacks jars into directories, so a seeded cache
            // holds source files, not jars (issue #234). Accept either form.
            let source_files = ["kt", "java", "swift"]
                .iter()
                .map(|ext| count_files(d, ext))
                .sum::<usize>();
            if jars > 0 {
                println!("[✓] {} library source jars extracted", jars);
                true
            } else if source_files > 0 {
                println!(
                    "[✓] library sources extracted ({} source files)",
                    source_files
                );
                true
            } else {
                println!("[!] library sources directory is empty: {}", d.display());
                println!("     run `kotlin-lsp extract-sources` to populate it");
                false
            }
        }
        Some(d) => {
            println!("[!] library sources not extracted (run `kotlin-lsp extract-sources`)");
            if verbose {
                println!("     expected: {}", d.display());
            }
            false
        }
        None => {
            println!("[!] cannot determine home directory for library sources");
            false
        }
    };
    if !sources_extracted {
        all_ok = false;
    }

    // ── 4. Index cache status ───────────────────────────────────────────
    let cache_dir = cache_dir().map(|c| c.join("kotlin-lsp"));
    if let Some(cd) = &cache_dir {
        if cd.exists() {
            let size = dir_size(cd);
            if size > 0 {
                println!("[✓] index cache: {} ({} KB)", cd.display(), size / 1024);
                if verbose {
                    if let Ok(entries) = std::fs::read_dir(cd) {
                        for e in entries.flatten() {
                            let path = e.path();
                            if path.is_dir() {
                                let sz = dir_size(&path);
                                println!(
                                    "     └─ {} ({} KB)",
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    sz / 1024
                                );
                            }
                        }
                    }
                }
            } else {
                println!("[!] index cache is empty (0 KB): {}", cd.display());
                println!("     run `kotlin-lsp index` to build one");
                all_ok = false;
            }
        } else {
            println!("[!] no index cache found (run `kotlin-lsp index` to build one)");
            all_ok = false;
            if verbose {
                println!("     expected: {}", cd.display());
            }
        }
    }

    // ── 4b. Workspace index loads (version + deserialize) ─────────────
    match crate::indexer::try_load_cache(&root) {
        Some(c) => {
            println!(
                "[✓] workspace index loads ({} files, version {})",
                c.entries.len(),
                c.version()
            );
        }
        None => {
            let path = crate::indexer::workspace_cache_path(&root);
            if path.exists() {
                println!(
                    "[!] workspace index corrupt or stale version — run `kotlin-lsp index` to rebuild: {}",
                    path.display()
                );
            } else {
                println!("[!] no workspace index (run `kotlin-lsp index` to build one)");
            }
            all_ok = false;
        }
    }

    // ── 5. Ignored .kt files under common source dirs ───────────────────
    if verbose && root_exists {
        let common_src_dirs = ["src", "app/src", "shared/src", "androidApp/src"];
        for dir_name in &common_src_dirs {
            let candidate = root.join(dir_name);
            if candidate.exists() && candidate.is_dir() && !has_git_ancestor(&candidate) {
                println!(
                    "[!] source directory not git-tracked: {}",
                    candidate.display()
                );
            }
        }
    }

    // ── 6. Runtime tools ────────────────────────────────────────────────
    let rg_available = which("rg").is_some();
    if rg_available {
        println!("[✓] rg (ripgrep) found on PATH");
    } else {
        println!("[!] rg (ripgrep) not found — cross-file searches will fail");
        all_ok = false;
    }

    let fd_available = which("fd").is_some();
    if fd_available {
        println!("[✓] fd found on PATH");
    } else if verbose {
        println!("[!] fd not found — file discovery may be slower");
    }

    // ── Summary ─────────────────────────────────────────────────────────
    println!();
    if all_ok {
        println!("All checks passed.");
    } else {
        println!("Some checks failed — see [!] items above.");
        // Failures must surface in the exit code, not just the summary: a
        // calling agent treats rc=0 as healthy (issue #237).
        std::process::exit(1);
    }
}
