//! CLI `batch` subcommand — cross-file atomic modifications.
//!
//! Reads a JSON rule file and applies find-replace and insert operations
//! across multiple files. Designed for KMP refactoring where VM + binding
//! + caller + Koin module must change together.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

use crate::cli::edit::{apply_file_edits, FileEdit};
use crate::indexer::Indexer;
use crate::resolver::{already_imported, fqns_for_name};
use crate::{Language, LinesExt};

#[derive(Debug, Deserialize)]
struct BatchRule {
    files: std::collections::HashMap<String, Vec<FileAction>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
enum FileAction {
    #[serde(rename = "replace")]
    Replace { old: String, new: String },
    #[serde(rename = "insert")]
    Insert {
        after_line: Option<u32>,
        before_line: Option<u32>,
        content: String,
    },
}

#[allow(clippy::unused_enumerate_index)]
pub(crate) fn run_batch(rule_file: &PathBuf, dry_run: bool) {
    let json = match std::fs::read_to_string(rule_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: read error: {e}", rule_file.display());
            std::process::exit(1);
        }
    };

    let rule: BatchRule = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Invalid rule JSON: {e}");
            std::process::exit(1);
        }
    };

    let mut total_replaces = 0u32;
    let mut total_inserts = 0u32;
    let mut files_modified = 0u32;

    for (file_path, actions) in &rule.files {
        let original = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: read error: {e}", file_path);
                continue;
            }
        };

        let mut modified = original.clone();
        let mut file_changed = false;

        for action in actions {
            match action {
                FileAction::Replace { old, new } => {
                    if modified.contains(old.as_str()) {
                        modified = modified.replace(old.as_str(), new.as_str());
                        total_replaces += 1;
                        file_changed = true;
                    }
                }
                FileAction::Insert {
                    after_line,
                    before_line,
                    content,
                } => {
                    let lines: Vec<&str> = modified.lines().collect();
                    let insert_at = if let Some(al) = after_line {
                        (*al as usize).min(lines.len())
                    } else if let Some(bl) = before_line {
                        (bl.saturating_sub(1) as usize).min(lines.len())
                    } else {
                        0
                    };

                    let indent = lines
                        .get(insert_at.saturating_sub(1))
                        .map(|l| {
                            l.chars()
                                .take_while(|c| c.is_whitespace())
                                .collect::<String>()
                        })
                        .unwrap_or_default();

                    let inserted: Vec<String> = content
                        .split('\n')
                        .map(|c| {
                            if c.is_empty() {
                                String::new()
                            } else {
                                format!("{indent}{c}")
                            }
                        })
                        .collect();

                    let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                    for _ in 0..inserted.len() {
                        new_lines.insert(insert_at, String::new());
                    }
                    for (j, ins) in inserted.iter().enumerate() {
                        new_lines[insert_at + j] = ins.clone();
                    }

                    modified = new_lines.join("\n");
                    total_inserts += 1;
                    file_changed = true;
                }
            }
        }

        if file_changed {
            if dry_run {
                println!("--- {} (dry-run) ---", file_path);
                for (i, (o, m)) in original.lines().zip(modified.lines()).enumerate() {
                    if o != m {
                        println!("  L{}: -{}", i + 1, o);
                        println!("  L{}: +{}", i + 1, m);
                    }
                }
            } else {
                if let Err(e) = std::fs::write(file_path, &modified) {
                    eprintln!("{}: write error: {e}", file_path);
                } else {
                    files_modified += 1;
                }
            }
        }
    }

    if dry_run {
        println!(
            "dry-run: {} files, {} replaces, {} inserts",
            files_modified, total_replaces, total_inserts
        );
    } else {
        println!(
            "done: {} files, {} replaces, {} inserts",
            files_modified, total_replaces, total_inserts
        );
    }
}

// ─── batch-imports: resolve and add missing imports ──────────────────────

#[derive(Debug, Clone, serde::Serialize)]
struct UnresolvedCandidate {
    line: usize,
    name: String,
    status: String,
    fqns: Vec<String>,
}

/// Batch-add missing imports to a file.
///
/// Scans for uppercase identifiers, resolves each against the workspace index,
/// creates import edits for unique (unambiguous) FQNs, and reports ambiguous
/// / unresolvable identifiers separately.
pub(crate) fn run_batch_imports(
    file: &Path,
    idx: &Arc<Indexer>,
    dry_run: bool,
    apply: bool,
    json: bool,
    output: Option<&str>,
) {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: read error: {e}", file.display());
            std::process::exit(1);
        }
    };

    let uri = Url::from_file_path(file).expect("valid file path");
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let imports = lines.parse_imports();

    let package_name = idx
        .files
        .get(uri.as_str())
        .and_then(|f| f.package.clone())
        .unwrap_or_default();

    let needs_semicolons = Language::from_path(&file.to_string_lossy()).needs_semicolons();

    // Scan for unique / ambiguous / unknown uppercase identifiers.
    let mut seen = std::collections::HashSet::<String>::new();
    let mut candidates: Vec<UnresolvedCandidate> = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("package ")
        {
            continue;
        }
        for word in line.split(|c: char| !c.is_alphanumeric() && c != '.') {
            let w = word.trim();
            if w.is_empty() || w.len() < 2 {
                continue;
            }
            if !w.chars().next().is_some_and(|c| c.is_uppercase()) {
                continue;
            }
            // Skip common built-in / special types.
            if matches!(
                w,
                "I" | "Unit" | "String" | "Int" | "Long" | "Float" | "Double" | "Boolean" | "Any"
            ) {
                continue;
            }
            if !seen.insert(w.to_string()) {
                continue;
            }

            let fqns = fqns_for_name(idx, w);
            if fqns.is_empty() {
                candidates.push(UnresolvedCandidate {
                    line: line_idx,
                    name: w.to_string(),
                    status: "unknown".to_string(),
                    fqns: vec![],
                });
            } else if fqns.len() == 1 {
                candidates.push(UnresolvedCandidate {
                    line: line_idx,
                    name: w.to_string(),
                    status: "unique".to_string(),
                    fqns,
                });
            } else {
                candidates.push(UnresolvedCandidate {
                    line: line_idx,
                    name: w.to_string(),
                    status: "ambiguous".to_string(),
                    fqns,
                });
            }
        }
    }

    // Classify: unique → import, ambiguous → report, unknown → report.
    let mut unique_imports: Vec<String> = Vec::new();
    let mut ambiguities: Vec<UnresolvedCandidate> = Vec::new();
    let mut unknowns: Vec<UnresolvedCandidate> = Vec::new();

    for cand in &candidates {
        match cand.status.as_str() {
            "unique" => {
                let fqn = &cand.fqns[0];
                if !already_imported(fqn, &imports) {
                    let pkg = fqn.rfind('.').map(|i| &fqn[..i]).unwrap_or("");
                    if pkg != package_name && !unique_imports.contains(fqn) {
                        unique_imports.push(fqn.clone());
                    }
                }
            }
            "ambiguous" => ambiguities.push(cand.clone()),
            "unknown" => unknowns.push(cand.clone()),
            _ => {}
        }
    }

    // Build TextEdit for each unique import (all inserted at the same line,
    // sorted in descending order so positions stay valid).
    let import_line = lines.import_insertion_line();
    let mut text_edits: Vec<TextEdit> = Vec::new();
    for fqn in &unique_imports {
        let stmt = if needs_semicolons {
            format!("import {fqn};")
        } else {
            format!("import {fqn}")
        };
        let needs_blank = import_line > 0
            && lines
                .get((import_line - 1) as usize)
                .map(|l| l.trim_start().starts_with("package "))
                .unwrap_or(false)
            && lines
                .get(import_line as usize)
                .map(|l| !l.trim().is_empty())
                .unwrap_or(false);
        let new_text = if needs_blank {
            format!("\n{stmt}\n")
        } else {
            format!("{stmt}\n")
        };
        text_edits.push(TextEdit {
            range: Range {
                start: Position {
                    line: import_line,
                    character: 0,
                },
                end: Position {
                    line: import_line,
                    character: 0,
                },
            },
            new_text,
        });
    }

    // Sort descending so earlier insertions don't shift later positions.
    text_edits.sort_by_key(|a| a.range.start.line);
    text_edits.reverse();
    let file_edit = FileEdit {
        path: file.to_path_buf(),
        edits: text_edits,
    };

    // Output.
    if json || output.is_some() {
        let (old_lines, new_lines) = if file_edit.edits.is_empty() {
            (lines.clone(), lines.clone())
        } else {
            let new = crate::cli::edit::apply_text_edits_to_lines(&lines, &file_edit.edits);
            (lines.clone(), new)
        };
        let result = serde_json::json!({
            "file": file.to_string_lossy(),
            "unique": unique_imports,
            "ambiguous": ambiguities.iter().map(|c| serde_json::json!({
                "line": c.line, "name": c.name, "fqns": c.fqns,
            })).collect::<Vec<_>>(),
            "unknown": unknowns.iter().map(|c| serde_json::json!({
                "line": c.line, "name": c.name,
            })).collect::<Vec<_>>(),
            "modified": unique_imports.len(),
            "preview": {
                "old_lines": old_lines,
                "new_lines": new_lines,
            },
        });
        let out_str = serde_json::to_string_pretty(&result).expect("json");
        if let Some(path) = output {
            let _ = std::fs::write(path, &out_str);
        }
        println!("{out_str}");
    } else if apply && !dry_run {
        let summary = apply_file_edits(&[file_edit], None, false);
        println!("{}", serde_json::to_string(&summary).expect("json"));
    } else {
        println!(
            "{}: {} unique imports",
            file.display(),
            unique_imports.len()
        );
        println!(
            "{} ambiguous, {} unknown",
            ambiguities.len(),
            unknowns.len()
        );
        if !unique_imports.is_empty() {
            println!(
                "To apply: kotlin-lsp batch-imports {} --apply",
                file.display()
            );
        }
    }
}
