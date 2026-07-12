//! Workspace graph — module → package → symbol hierarchy query.
//!
//! `workspace --json` returns a full snapshot including symbols and entry points.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::cli::modules::{self, ModuleInfo};

// ── Snapshot types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct WorkspaceSnapshot {
    project_root: String,
    modules: Vec<ModuleInfo>,
    total_files: usize,
    total_symbols: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    symbols: Vec<SnapshotSymbol>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    entry_points: Vec<EntryPoint>,
}

#[derive(Debug, Serialize)]
struct SnapshotSymbol {
    name: String,
    kind: String,
    visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    file: String,
    line: u32,
    col: u32,
}

#[derive(Debug, Serialize)]
struct EntryPoint {
    kind: String,
    name: String,
    file: String,
    line: u32,
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub(crate) fn run_workspace(json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let snapshot = collect_snapshot(&root);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).expect("serialize JSON")
        );
    } else {
        println!(
            "Workspace: {} modules, {} files, {} symbols",
            snapshot.modules.len(),
            snapshot.total_files,
            snapshot.total_symbols,
        );
        for m in &snapshot.modules {
            println!(
                "  {} ({} files, {} deps) @ {}",
                m.name,
                m.file_count,
                m.dependencies.len(),
                m.path,
            );
        }
        for ep in &snapshot.entry_points {
            println!(
                "  entry: {} '{}' @ {}:{}",
                ep.kind, ep.name, ep.file, ep.line,
            );
        }
    }
}

fn collect_snapshot(root: &Path) -> WorkspaceSnapshot {
    let modules = modules::discover_modules();

    let mut total_files = 0usize;
    let mut total_symbols = 0usize;
    let mut symbols: Vec<SnapshotSymbol> = Vec::new();
    let mut entry_points: Vec<EntryPoint> = Vec::new();

    for module in &modules {
        let module_path = PathBuf::from(&module.path);
        if let Ok(entries) = std::fs::read_dir(&module_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    collect_from_dir(
                        &p,
                        &mut total_files,
                        &mut total_symbols,
                        &mut symbols,
                        &mut entry_points,
                    );
                } else if is_source_file(&p) {
                    total_files += 1;
                    if let Ok(src) = std::fs::read_to_string(&p) {
                        let (sym_count, syms, eps) = extract_file_symbols(&p, &src);
                        total_symbols += sym_count;
                        symbols.extend(syms);
                        entry_points.extend(eps);
                    }
                }
            }
        }
    }

    if modules.is_empty() {
        let src_dir = root.join("src");
        if src_dir.exists() {
            collect_from_dir(
                &src_dir,
                &mut total_files,
                &mut total_symbols,
                &mut symbols,
                &mut entry_points,
            );
        }
    }

    WorkspaceSnapshot {
        project_root: root.display().to_string(),
        modules,
        total_files,
        total_symbols,
        symbols,
        entry_points,
    }
}

fn collect_from_dir(
    dir: &Path,
    total_files: &mut usize,
    total_symbols: &mut usize,
    symbols: &mut Vec<SnapshotSymbol>,
    entry_points: &mut Vec<EntryPoint>,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_from_dir(&p, total_files, total_symbols, symbols, entry_points);
            } else if is_source_file(&p) {
                *total_files += 1;
                if let Ok(src) = std::fs::read_to_string(&p) {
                    let (sym_count, syms, eps) = extract_file_symbols(&p, &src);
                    *total_symbols += sym_count;
                    symbols.extend(syms);
                    entry_points.extend(eps);
                }
            }
        }
    }
}

fn is_source_file(p: &Path) -> bool {
    p.extension()
        .map(|ext| ext == "kt" || ext == "kts" || ext == "java" || ext == "swift")
        .unwrap_or(false)
}

fn extract_file_symbols(
    file: &Path,
    source: &str,
) -> (usize, Vec<SnapshotSymbol>, Vec<EntryPoint>) {
    let mut count = 0usize;
    let mut symbols = Vec::new();
    let mut entry_points = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let (kind, name_start) = if trimmed.starts_with("class ") {
            ("class", 6)
        } else if trimmed.starts_with("interface ") {
            ("interface", 10)
        } else if trimmed.starts_with("object ") {
            ("object", 7)
        } else if trimmed.starts_with("enum class ") || trimmed.starts_with("enum ") {
            (
                "enum",
                if trimmed.starts_with("enum class ") {
                    11
                } else {
                    5
                },
            )
        } else if trimmed.starts_with("fun ") || trimmed.starts_with("suspend fun ") {
            (
                "function",
                if trimmed.starts_with("suspend fun ") {
                    12
                } else {
                    4
                },
            )
        } else if trimmed.starts_with("data class ") {
            ("class", 11)
        } else if trimmed.starts_with("sealed class ") || trimmed.starts_with("sealed interface ") {
            ("class", 13)
        } else {
            continue;
        };

        let remainder = &trimmed[name_start..];
        let name = remainder
            .split(['(', ':', '{', '<', ' ', '\t'])
            .next()
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            continue;
        }

        let visibility = if trimmed.starts_with("private ") {
            "private"
        } else if trimmed.starts_with("internal ") {
            "internal"
        } else if trimmed.starts_with("protected ") {
            "protected"
        } else {
            "public"
        };

        let signature = remainder
            .split('{')
            .next()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != name);

        symbols.push(SnapshotSymbol {
            name: name.to_string(),
            kind: kind.to_string(),
            visibility: visibility.to_string(),
            signature,
            file: file.display().to_string(),
            line: (line_num + 1) as u32,
            col: 1,
        });
        count += 1;

        if kind == "class"
            && (name.ends_with("Activity")
                || name.ends_with("Fragment")
                || name.ends_with("Application"))
        {
            entry_points.push(EntryPoint {
                kind: "android".to_string(),
                name: name.to_string(),
                file: file.display().to_string(),
                line: (line_num + 1) as u32,
            });
        }
        if kind == "function" && (name == "main" || name == "Main") {
            entry_points.push(EntryPoint {
                kind: "main".to_string(),
                name: name.to_string(),
                file: file.display().to_string(),
                line: (line_num + 1) as u32,
            });
        }
    }

    (count, symbols, entry_points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_file_symbols_kotlin() {
        let src = "package com.example\nclass Foo\nfun bar()\nprivate val x = 1";
        let (count, syms, _eps) = extract_file_symbols(&PathBuf::from("Foo.kt"), src);
        assert_eq!(count, 2);
        assert!(syms.iter().any(|s| s.name == "Foo" && s.kind == "class"));
        assert!(syms.iter().any(|s| s.name == "bar" && s.kind == "function"));
    }

    #[test]
    fn extract_entry_points() {
        let src = "package com.example\nclass MainActivity\nfun main()";
        let (_count, _syms, eps) = extract_file_symbols(&PathBuf::from("MainActivity.kt"), src);
        assert!(eps
            .iter()
            .any(|e| e.name == "MainActivity" && e.kind == "android"));
        assert!(eps.iter().any(|e| e.name == "main" && e.kind == "main"));
    }
}
