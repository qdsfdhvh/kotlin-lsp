//! Workspace graph — module → package → symbol hierarchy query.
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct WorkspaceInfo {
    modules: Vec<ModuleSummary>,
    total_files: usize,
    total_symbols: usize,
}
#[derive(Debug, Serialize)]
struct ModuleSummary {
    name: String,
    packages: Vec<PackageSummary>,
    file_count: usize,
}
#[derive(Debug, Serialize)]
struct PackageSummary {
    name: String,
    symbols: Vec<String>,
}

pub(crate) fn run_workspace(json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let info = collect_workspace_info(&root);
    if json {
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!(
            "Workspace: {} modules, {} files, {} symbols",
            info.modules.len(),
            info.total_files,
            info.total_symbols
        );
        for m in &info.modules {
            println!(
                "  {} ({} files, {} packages)",
                m.name,
                m.file_count,
                m.packages.len()
            );
            for p in &m.packages {
                println!("    {}: {} symbols", p.name, p.symbols.len());
            }
        }
    }
}

fn collect_workspace_info(root: &std::path::Path) -> WorkspaceInfo {
    use std::collections::HashMap;
    let mut modules: Vec<ModuleSummary> = Vec::new();
    let mut total_files = 0usize;
    let mut total_symbols = 0usize;

    // Walk src/ directories to discover packages and symbols.
    let src_dir = root.join("src");
    if src_dir.exists() {
        total_files = count_files(&src_dir, &mut 0);
        let mut pkg_map: HashMap<String, Vec<String>> = HashMap::new();
        collect_package_symbols(&src_dir, &mut pkg_map, &mut total_symbols);
        let packages: Vec<PackageSummary> = pkg_map
            .into_iter()
            .map(|(name, symbols)| PackageSummary { name, symbols })
            .collect();
        modules.push(ModuleSummary {
            name: ":".to_string(),
            packages,
            file_count: total_files,
        });
    }

    WorkspaceInfo {
        modules,
        total_files,
        total_symbols,
    }
}

fn count_files(dir: &std::path::Path, count: &mut usize) -> usize {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                count_files(&p, count);
            } else if let Some(ext) = p.extension() {
                if ext == "kt" || ext == "kts" || ext == "java" {
                    *count += 1;
                }
            }
        }
    }
    *count
}

fn collect_package_symbols(
    dir: &std::path::Path,
    map: &mut std::collections::HashMap<String, Vec<String>>,
    total: &mut usize,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_package_symbols(&p, map, total);
            } else if let Some(ext) = p.extension() {
                if ext == "kt" || ext == "kts" || ext == "java" {
                    if let Ok(src) = std::fs::read_to_string(&p) {
                        let (pkg, syms) = extract_symbols(&src);
                        let entry = map.entry(pkg).or_default();
                        for s in syms {
                            if !entry.contains(&s) {
                                entry.push(s);
                                *total += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::manual_strip)]
fn extract_symbols(src: &str) -> (String, Vec<String>) {
    let mut pkg = ".".to_string();
    let mut symbols = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("package ") {
            pkg = t["package ".len()..].trim().to_string();
        } else if t.starts_with("class ") {
            symbols.push(
                t[6..]
                    .split(['(', ':', '{'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        } else if t.starts_with("interface ") {
            symbols.push(
                t[10..]
                    .split(['(', ':', '{'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        } else if t.starts_with("object ") {
            symbols.push(
                t[7..]
                    .split(['(', ':', '{'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        } else if t.starts_with("fun ") {
            symbols.push(
                t[4..]
                    .split(['(', ':', '{'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        } else if t.starts_with("suspend fun ") {
            symbols.push(
                t[12..]
                    .split(['(', ':', '{'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        } else if t.starts_with("val ") || t.starts_with("var ") {
            symbols.push(
                t[4..]
                    .split([':', '='])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        }
    }
    (pkg, symbols)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extract_symbols_basic() {
        let (pkg, syms) = extract_symbols("package com.example\nclass Foo\nfun bar()");
        assert_eq!(pkg, "com.example");
        assert!(syms.contains(&"Foo".to_string()));
        assert!(syms.contains(&"bar".to_string()));
    }
}
