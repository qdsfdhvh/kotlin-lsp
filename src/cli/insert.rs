//! CLI `insert` subcommand — insert code at a specific line.
//!
//! Inserts text before or after a given line number in a file.
//! Supports --in-place to write back instead of printing to stdout.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Serialize)]
struct InsertResult {
    file: String,
    inserted_at: u32,
    lines_before: usize,
    lines_after: usize,
}

#[allow(clippy::unused_enumerate_index)]
pub(crate) fn run_insert(
    file: &PathBuf,
    line: u32,
    before: bool,
    after: bool,
    content: &str,
    in_place: bool,
) {
    if before == after {
        eprintln!("insert requires exactly one of --before or --after");
        std::process::exit(1);
    }

    let original = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: read error: {e}", file.display());
            std::process::exit(1);
        }
    };

    let lines: Vec<&str> = original.lines().collect();
    let insert_at = if after {
        line as usize
    } else {
        (line as usize).saturating_sub(1)
    }
    .min(lines.len());

    let indent = lines
        .get(insert_at.saturating_sub(1))
        .map(|l| {
            l.chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>()
        })
        .unwrap_or_default();

    let content_lines: Vec<&str> = content.split('\n').collect();
    let insert_lines: Vec<String> = content_lines
        .iter()
        .map(|c| {
            if c.is_empty() {
                String::new()
            } else {
                format!("{indent}{c}")
            }
        })
        .collect();

    let mut result: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    for (offset, inserted) in insert_lines.iter().enumerate() {
        result.insert(insert_at + offset, inserted.clone());
    }

    let mut new_content = result.join("\n");
    if original.ends_with('\n') {
        new_content.push('\n');
    }

    if in_place {
        if let Err(e) = std::fs::write(file, &new_content) {
            eprintln!("{}: write error: {e}", file.display());
            std::process::exit(1);
        }
        let info = InsertResult {
            file: file.to_string_lossy().into_owned(),
            inserted_at: line,
            lines_before: lines.len(),
            lines_after: result.len(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&info).unwrap_or_default()
        );
    } else {
        println!("{new_content}");
    }
}

use std::path::Path;
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

use crate::cli::edit::{apply_file_edits, FileEdit, FileEditResult};
use crate::indexer::Indexer;
use crate::{Language, LinesExt};

/// Semantic insert dispatcher.
///
/// Determines the insert position based on `kind`:
/// - `"import"` — uses `import_insertion_line()` from `LinesExt`
/// - `"member"` — finds the class body range for `owner` and inserts as member
/// - `"function"` — same as member but explicitly named function
/// - `"override"` — TBD
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_semantic_insert(
    file: &Path,
    kind: &str,
    owner: Option<&str>,
    content: &str,
    idx: &Arc<Indexer>,
    dry_run: bool,
    apply: bool,
    json: bool,
) {
    let original = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: read error: {e}", file.display());
            std::process::exit(1);
        }
    };
    let lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    let uri = Url::from_file_path(file).expect("valid file path");

    let (insert_line, indent) = match kind {
        "import" => {
            let line = lines.import_insertion_line();
            let indent = String::new();
            (line, indent)
        }
        "member" | "function" => {
            let owner = owner.unwrap_or_else(|| {
                eprintln!("error: --kind member/function requires --owner <ClassName>");
                std::process::exit(1);
            });
            let data = match idx.files.get(uri.as_str()) {
                Some(d) => d,
                None => {
                    eprintln!("error: file not found in index");
                    std::process::exit(1);
                }
            };
            let owner_sym = data
                .symbols
                .iter()
                .find(|s| s.name == owner)
                .unwrap_or_else(|| {
                    eprintln!("error: class '{owner}' not found in file");
                    std::process::exit(1);
                });
            // Insert at the line before the closing `}` of the class.
            let insert_at = owner_sym.range.end.line.saturating_sub(1);
            let prev_line = lines
                .get(insert_at.saturating_sub(1) as usize)
                .map(|l| l.as_str())
                .unwrap_or("");
            let indent: String = prev_line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            let class_indent = if indent.len() >= 4 {
                indent[..indent.len() - 4].to_string()
            } else {
                String::new()
            };
            (insert_at, class_indent + "    ")
        }
        "override" => {
            eprintln!("error: --kind override not yet implemented");
            std::process::exit(1);
        }
        _ => {
            eprintln!("error: unknown insert kind '{kind}'");
            std::process::exit(1);
        }
    };

    let insert_lines: Vec<String> = content
        .split('\n')
        .map(|c| {
            if c.is_empty() {
                String::new()
            } else {
                format!("{indent}{c}")
            }
        })
        .collect();

    let mut result: Vec<String> = lines.clone();
    for (offset, ins_line) in insert_lines.iter().enumerate() {
        result.insert((insert_line as usize) + offset, ins_line.clone());
    }

    let new_content = result.join("\n");
    let new_content = if original.ends_with('\n') && !new_content.ends_with('\n') {
        format!("{new_content}\n")
    } else {
        new_content
    };

    let te = TextEdit {
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(lines.len() as u32, 0),
        },
        new_text: new_content.clone(),
    };
    let file_edit = FileEdit {
        path: file.to_path_buf(),
        edits: vec![te],
    };

    if json || dry_run {
        let summary = apply_file_edits(&[file_edit], None, true);
        println!("{}", serde_json::to_string(&summary).expect("json"));
    } else if apply {
        let summary = apply_file_edits(&[file_edit], None, false);
        println!("{}", serde_json::to_string(&summary).expect("json"));
    } else {
        println!("{new_content}");
    }
}
