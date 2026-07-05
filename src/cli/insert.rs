//! CLI `insert` subcommand — insert code at a specific line.
//!
//! Supports both basic line-based insertion and semantic insert modes
//! (import, member, function, override).

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

// ─── semantic insert ──────────────────────────────────────────────────────────

use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range, TextEdit};

use crate::cli::edit::{apply_file_edits, FileEdit};
use crate::indexer::Indexer;
use crate::LinesExt;

/// Computes the n-space indent string for tree-sitter node-based indentation.
fn compute_indent(lines: &[String], line_idx: u32) -> String {
    lines
        .get(line_idx as usize)
        .map(|l| {
            l.chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>()
        })
        .unwrap_or_default()
}

/// Find the class body closure line using tree-sitter.
/// Returns (insert_line, indent_string).
pub(crate) fn find_class_body_insert_point(
    lines: &[String],
    owner_name: &str,
) -> Result<(u32, String), String> {
    let source = lines.join("\n");

    parser
        .set_language(&tree_sitter::Language::from(tree_sitter_kotlin::LANGUAGE))
        .expect("kotlin parser init");

    let tree = parser.parse(&source, None).ok_or("parse failed")?;
    let root = tree.root_node();

    // Walk children to find class/interface/object declarations matching owner_name.
    let class_node = find_class_named(root, owner_name, source.as_bytes())
        .ok_or_else(|| format!("class '{owner_name}' not found"))?;

    // The last line of the class node is the closing `}`.
    let end_line = class_node.end_position().row as u32;

    // Determine indent from existing members, or fall back to class indent + 4.
    let indent = {
        let mut cursor = class_node.walk();
        let first_member = class_node
            .children(&mut cursor)
            .find(|c| c.is_named() && c.kind() != "class_body");
        if let Some(member) = first_member {
            let member_indent = compute_indent(lines, member.start_position().row as u32);
            if member_indent.len() >= 4 {
                member_indent
            } else {
                compute_indent(lines, class_node.start_position().row as u32) + "    "
            }
        } else {
            // Empty body — use class indent + 4 spaces.
            compute_indent(lines, class_node.start_position().row as u32) + "    "
        }
    };

    // Insert at the line before the closing `}`.
    let insert_line = end_line.saturating_sub(1);
    Ok((insert_line, indent))
}

/// Recursively search for a class/interface/object declaration with the given name.
fn find_class_named<'a>(
    root: tree_sitter::Node<'a>,
    name: &str,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();
        if kind == crate::queries::KIND_CLASS_DECL
            || kind == crate::queries::KIND_INTERFACE_DECL
            || kind == crate::queries::KIND_OBJECT_DECL
            || kind == crate::queries::KIND_ENUM_DECL
        {
            // Check if this class's name matches.
            let mut sub = child.walk();
            for c in child.children(&mut sub) {
                let ck = c.kind();
                if ck == crate::queries::KIND_SIMPLE_IDENT
                    || ck == crate::queries::KIND_IDENTIFIER
                    || ck == crate::queries::KIND_TYPE_IDENT
                {
                    if c.utf8_text(source).ok() == Some(name) {
                        return Some(child);
                    }
                    break;
                }
            }
        }
        // Recurse into children for nested classes.
        if let Some(found) = find_class_named(child, name, source) {
            return Some(found);
        }
    }
    None
}

/// Semantic insert dispatcher.
///
/// Determines the insert position based on `kind`:
/// - `"import"` — uses `import_insertion_line()` from `LinesExt`, auto-generates from FQN
/// - `"member"` — finds the class body via tree-sitter and inserts as member
/// - `"function"` — same as member
/// - `"override"` — finds class body, inserts override boilerplate
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
    name_arg: Option<&str>,
) {
    let original = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: read error: {e}", file.display());
            std::process::exit(1);
        }
    };
    let lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();

    let (insert_line, indent, content_final) = match kind {
        "import" => {
            // Determine the import insertion line.
            let line = lines.import_insertion_line();
            // Use make_import_edit to handle blank-line gaps after package declarations.
            let needs_blank = line > 0
                && lines
                    .get((line - 1) as usize)
                    .map(|l| l.trim_start().starts_with("package "))
                    .unwrap_or(false)
                && lines
                    .get(line as usize)
                    .map(|l| !l.trim().is_empty())
                    .unwrap_or(false);

            let import_lines = content.trim();
            let formatted = if needs_blank {
                format!("\n{import_lines}")
            } else {
                import_lines.to_string()
            };

            // Check for duplicate imports.
            let imports = lines.parse_imports();
            let imported_fqn = content.trim().strip_prefix("import ").unwrap_or(content);
            // Strip trailing semicolon and newlines.
            let imported_fqn = imported_fqn.trim().trim_end_matches(';');
            if crate::resolver::already_imported(imported_fqn, &imports) {
                eprintln!(
                    "info: {imported_fqn} is already imported in {}",
                    file.display()
                );
                if json {
                    let summary = serde_json::json!({
                        "status": "already_imported",
                        "file": file.to_string_lossy().into_owned(),
                        "fqn": imported_fqn,
                    });
                    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
                }
                return;
            }

            (line as u32, String::new(), formatted)
        }
        "member" | "function" => {
            let owner = owner.unwrap_or_else(|| {
                eprintln!("error: --kind member/function requires --owner <ClassName>");
                std::process::exit(1);
            });

            let (insert_at, indent_val) = match find_class_body_insert_point(&lines, owner) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            (insert_at, indent_val, content.to_string())
        }
        "override" => {
            let owner = owner.unwrap_or_else(|| {
                eprintln!("error: --kind override requires --owner <ClassName>");
                std::process::exit(1);
            });

            let (insert_at, indent_val) = match find_class_body_insert_point(&lines, owner) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            let override_content = if let Some(method_name) = name_arg {
                // Look up method signature from index to generate override boilerplate.
                generate_override(method_name, idx, &indent_val)
            } else {
                // Use --content as-is for custom overrides.
                if content.is_empty() {
                    eprintln!("error: --kind override requires either --name <method> or --content <text>");
                    std::process::exit(1);
                }
                content.to_string()
            };

            (insert_at, indent_val, override_content)
        }
        _ => {
            eprintln!("error: unknown insert kind '{kind}'");
            std::process::exit(1);
        }
    };

    // Build the insert lines with proper indentation.
    let insert_lines: Vec<String> = content_final
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
    let insert_line_idx = insert_line as usize;

    for (offset, ins_line) in insert_lines.iter().enumerate() {
        result.insert(
            (insert_line_idx).min(result.len()) + offset,
            ins_line.clone(),
        );
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

/// Generate an override keyword + method stub for a given method name.
fn generate_override(method_name: &str, _idx: &Arc<Indexer>, indent: &str) -> String {
    // For now, generate a reasonable boilerplate.
    // Future: look up the actual signature from supertypes/interfaces.
    format!("override fun {method_name}() {{\n{indent}    TODO(\"not implemented\")\n{indent}}}")
}

/// Test-only wrapper for generate_override.
#[doc(hidden)]
pub(crate) fn generate_override_test(method_name: &str, indent: &str) -> String {
    generate_override(method_name, &Arc::new(Indexer::new()), indent)
}

#[cfg(test)]
#[path = "insert_tests.rs"]
mod tests;
