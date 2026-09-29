//! CLI `check` subcommand — syntax error diagnostics without an LSP session.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize)]
struct CheckError {
    file: String,
    line: u32,
    col: u32,
    message: String,
}

/// A user-supplied input path that could not be processed at all (missing
/// path, failed directory traversal, neither file nor directory).
#[derive(Debug)]
pub(crate) struct InputError {
    pub(crate) file: String,
    pub(crate) message: String,
}

/// Result of expanding user-supplied paths into concrete source files.
/// `input_errors` must be surfaced as failures; `empty_dirs` are reported
/// separately and are not errors.
#[derive(Debug, Default)]
pub(crate) struct ExpandedFiles {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) input_errors: Vec<InputError>,
    pub(crate) empty_dirs: Vec<PathBuf>,
}

pub(crate) fn run_check(expanded: &ExpandedFiles, json: bool, when_exhaustive: bool) {
    use crate::parser::parse_by_extension;

    let mut errors: Vec<CheckError> = Vec::new();
    let mut files_ok = 0u32;
    let mut files_err = 0u32;

    // Unprocessable inputs (missing paths, failed traversals) are structured
    // failures — they must make the run exit nonzero, never a silent success.
    for input_error in &expanded.input_errors {
        errors.push(CheckError {
            file: input_error.file.clone(),
            line: 0,
            col: 0,
            message: input_error.message.clone(),
        });
        files_err += 1;
    }

    for file in &expanded.files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                if !json {
                    eprintln!("{}: read error: {e}", file.display());
                }
                errors.push(CheckError {
                    file: file.to_string_lossy().into_owned(),
                    line: 0,
                    col: 0,
                    message: format!("read error: {e}"),
                });
                files_err += 1;
                continue;
            }
        };

        let data = parse_by_extension(&file.to_string_lossy(), &content);

        if data.syntax_errors.is_empty() {
            files_ok += 1;
            continue;
        }

        files_err += 1;
        for se in &data.syntax_errors {
            errors.push(CheckError {
                file: file.to_string_lossy().into_owned(),
                line: se.range.start.line + 1,
                col: se.range.start.character + 1,
                message: se.message.clone(),
            });
        }
    }

    if json {
        let output = serde_json::json!({
            "files_ok": files_ok,
            "files_with_errors": files_err,
            "errors": errors,
            "empty_dirs": expanded
                .empty_dirs
                .iter()
                .map(|d| d.to_string_lossy())
                .collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).expect("serialize JSON")
        );
    } else {
        for e in &errors {
            println!("{}:{}:{}: {}", e.file, e.line, e.col, e.message);
        }
        for d in &expanded.empty_dirs {
            println!(
                "note: {}: no checkable sources found in directory",
                d.display()
            );
        }
        if errors.is_empty() {
            println!("All {} files OK.", files_ok);
        } else {
            eprintln!("{} error(s) in {} file(s).", errors.len(), files_err);
        }
    }

    if when_exhaustive {
        check_when_exhaustive(&expanded.files);
    }

    if !errors.is_empty() {
        std::process::exit(1);
    }
}

pub(crate) fn expand_file_list(paths: &[PathBuf]) -> ExpandedFiles {
    let mut result = ExpandedFiles::default();
    for path in paths {
        if path.is_dir() {
            expand_directory(path, &mut result);
        } else if path.is_file() {
            result.files.push(path.clone());
        } else {
            let message = if path.exists() {
                "not a regular file or directory".to_string()
            } else {
                "no such file or directory".to_string()
            };
            result.input_errors.push(InputError {
                file: path.to_string_lossy().into_owned(),
                message,
            });
        }
    }
    result
}

fn expand_directory(dir: &Path, result: &mut ExpandedFiles) {
    let mut found = 0usize;
    let mut traversal_failed = false;
    for entry in walkdir::WalkDir::new(dir) {
        match entry {
            Ok(entry) => {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension() {
                        if matches!(ext.to_str(), Some("kt" | "kts" | "java" | "swift")) {
                            result.files.push(p.to_path_buf());
                            found += 1;
                        }
                    }
                }
            }
            Err(err) => {
                traversal_failed = true;
                let path = err.path().unwrap_or(dir).to_string_lossy().into_owned();
                let message = match err.io_error() {
                    Some(io) => format!("traversal error: {io}"),
                    None => "traversal error".to_string(),
                };
                result.input_errors.push(InputError {
                    file: path,
                    message,
                });
            }
        }
    }
    if found == 0 && !traversal_failed {
        result.empty_dirs.push(dir.to_path_buf());
    }
}

// ── when exhaustiveness check ──────────────────────────────────────────────

/// Lightweight CST-level check for non-exhaustive when expressions.
fn check_when_exhaustive(files: &[PathBuf]) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_sg::LANGUAGE.into())
        .ok();

    for f in files {
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let Some(tree) = parser.parse(&src, None) else {
            continue;
        };
        walk_when_nodes(f, tree.root_node(), &src);
    }
}

fn walk_when_nodes(file: &Path, node: tree_sitter::Node, src: &str) {
    if node.kind() == "when_expression" {
        check_one_when(file, node, src);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_when_nodes(file, child, src);
    }
}

fn check_one_when(file: &Path, node: tree_sitter::Node, src: &str) {
    let text = &src[node.start_byte()..node.end_byte()];

    // Only `when (expr) { ... }` — skip subject-less when
    let before_brace = text.find('{').unwrap_or(text.len());
    let header = &text[..before_brace];
    if !header.contains('(') {
        return;
    }
    // Has else branch → exhaustive
    if text.contains("else ->") {
        return;
    }
    // Flag when with single branch and no else
    let branch_count = text.matches(" -> ").count();
    if branch_count <= 1 {
        let line = node.start_position().row as u32 + 1;
        eprintln!(
            "{}:{}: when expression may be non-exhaustive (no 'else' branch)",
            file.display(),
            line,
        );
    }
}
