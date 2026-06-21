//! Shared edit preview/apply engine.
//!
//! Every write-capable command should route through this module so edits are
//! previewed, validated, and applied consistently.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tower_lsp::lsp_types::{AnnotatedTextEdit, OneOf, TextEdit, Url, WorkspaceEdit};

/// A resolved file-level edit — the result of flattening a `WorkspaceEdit`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileEdit {
    pub(crate) path: PathBuf,
    pub(crate) edits: Vec<TextEdit>,
}

/// Full edit summary.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct EditSummary {
    pub(crate) files_modified: usize,
    pub(crate) files: Vec<FileEditResult>,
}

/// Per-file edit result.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub(crate) enum FileEditResult {
    #[serde(rename = "ok")]
    Ok {
        path: PathBuf,
        edits_applied: usize,
        dry_run: bool,
    },
    #[serde(rename = "error")]
    Error { path: PathBuf, message: String },
    #[serde(rename = "noop")]
    Noop { path: PathBuf },
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn uri_to_path(uri: &Url) -> Result<PathBuf, String> {
    uri.to_file_path()
        .map_err(|_| format!("URI is not a valid file path: {uri}"))
}

fn oneof_to_textedit(oneof: &OneOf<TextEdit, AnnotatedTextEdit>) -> TextEdit {
    match oneof {
        OneOf::Left(te) => te.clone(),
        OneOf::Right(ae) => TextEdit {
            range: ae.text_edit.range,
            new_text: ae.text_edit.new_text.clone(),
        },
    }
}

/// Validate that a path is under the given workspace root.
pub(crate) fn path_is_under_root(path: &Path, root: &Path) -> bool {
    path.canonicalize()
        .ok()
        .and_then(|p| root.canonicalize().ok().map(|r| p.starts_with(&r)))
        .unwrap_or(false)
}

// ── Flatten ──────────────────────────────────────────────────────────────

/// Flatten a `WorkspaceEdit` into per-file `FileEdit` entries.
pub(crate) fn flatten_workspace_edit(edit: &WorkspaceEdit) -> Result<Vec<FileEdit>, String> {
    let mut file_edits: Vec<FileEdit> = Vec::new();

    if let Some(changes) = &edit.changes {
        for (uri, text_edits) in changes {
            let path = uri_to_path(uri)?;
            file_edits.push(FileEdit {
                path,
                edits: text_edits.clone(),
            });
        }
    }

    if let Some(doc_changes) = &edit.document_changes {
        match doc_changes {
            tower_lsp::lsp_types::DocumentChanges::Edits(versioned) => {
                for ve in versioned {
                    let path = uri_to_path(&ve.text_document.uri)?;
                    file_edits.push(FileEdit {
                        path,
                        edits: ve.edits.iter().map(oneof_to_textedit).collect(),
                    });
                }
            }
            tower_lsp::lsp_types::DocumentChanges::Operations(ops) => {
                for op in ops {
                    if let tower_lsp::lsp_types::DocumentChangeOperation::Edit(ve) = op {
                        let path = uri_to_path(&ve.text_document.uri)?;
                        file_edits.push(FileEdit {
                            path,
                            edits: ve.edits.iter().map(oneof_to_textedit).collect(),
                        });
                    }
                }
            }
        }
    }

    Ok(file_edits)
}

// ── Apply text edits to lines ────────────────────────────────────────────

pub(crate) fn apply_text_edits_to_lines(lines: &[String], edits: &[TextEdit]) -> Vec<String> {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|a| (a.range.start.line, a.range.start.character));
    sorted.reverse();

    let mut result: Vec<String> = lines.to_vec();

    for edit in &sorted {
        let start_line = edit.range.start.line as usize;
        let end_line = edit.range.end.line as usize;
        let start_col = edit.range.start.character as usize;
        let end_col = edit.range.end.character as usize;

        if start_line >= result.len() {
            continue;
        }

        let end_line_clamped = end_line.min(result.len() - 1);

        if start_line == end_line_clamped {
            let line = &result[start_line];
            let end_col_clamped = end_col.min(line.len());
            let start_col_clamped = start_col.min(line.len());
            result[start_line] = format!(
                "{}{}{}",
                &line[..start_col_clamped],
                edit.new_text,
                &line[end_col_clamped..]
            );
        } else {
            let start_col_clamped = start_col.min(result[start_line].len());
            let line_prefix = result[start_line][..start_col_clamped].to_string();
            let end_col_clamped = end_col.min(result[end_line_clamped].len());
            let line_suffix = result[end_line_clamped][end_col_clamped..].to_string();

            let replacement_lines: Vec<String> = if edit.new_text.is_empty() {
                vec![]
            } else {
                edit.new_text.split('\n').map(|s| s.to_string()).collect()
            };

            let mut new_result: Vec<String> = Vec::with_capacity(result.len());
            new_result.extend_from_slice(&result[..start_line]);

            if let Some(first_replacement) = replacement_lines.first() {
                new_result.push(format!("{line_prefix}{first_replacement}"));
                if replacement_lines.len() > 1 {
                    for line in &replacement_lines[1..] {
                        new_result.push(line.clone());
                    }
                    if let Some(last) = new_result.last_mut() {
                        *last = format!("{last}{line_suffix}");
                    }
                } else if let Some(last) = new_result.last_mut() {
                    *last = format!("{last}{line_suffix}");
                }
            } else {
                new_result.push(format!("{line_prefix}{line_suffix}"));
            }

            new_result.extend_from_slice(&result[(end_line_clamped + 1)..]);
            result = new_result;
        }
    }

    result
}

// ── Preview ──────────────────────────────────────────────────────────────

/// Return (old_lines, new_lines) per file without writing.
#[allow(dead_code)]
#[allow(clippy::type_complexity)]
pub(crate) fn preview_file_edits(
    edits: &[FileEdit],
) -> Result<HashMap<PathBuf, (Vec<String>, Vec<String>)>, String> {
    let mut result = HashMap::new();
    for fe in edits {
        let content =
            std::fs::read_to_string(&fe.path).map_err(|e| format!("{}: {e}", fe.path.display()))?;
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let new_lines = apply_text_edits_to_lines(&lines, &fe.edits);
        result.insert(fe.path.clone(), (lines, new_lines));
    }
    Ok(result)
}

// ── Apply ────────────────────────────────────────────────────────────────

/// Apply file edits to disk.
pub(crate) fn apply_file_edits(
    edits: &[FileEdit],
    root: Option<&Path>,
    dry_run: bool,
) -> EditSummary {
    let mut results: Vec<FileEditResult> = Vec::with_capacity(edits.len());

    if let Some(root) = root {
        for fe in edits {
            if !path_is_under_root(&fe.path, root) {
                results.push(FileEditResult::Error {
                    path: fe.path.clone(),
                    message: format!("path is not under workspace root '{}'", root.display()),
                });
            }
        }
    }

    if results
        .iter()
        .any(|r| matches!(r, FileEditResult::Error { .. }))
    {
        return EditSummary {
            files_modified: 0,
            files: results,
        };
    }

    for fe in edits {
        if !fe.path.exists() {
            results.push(FileEditResult::Error {
                path: fe.path.clone(),
                message: "file does not exist".to_string(),
            });
            continue;
        }

        let content = match std::fs::read_to_string(&fe.path) {
            Ok(c) => c,
            Err(e) => {
                results.push(FileEditResult::Error {
                    path: fe.path.clone(),
                    message: format!("read error: {e}"),
                });
                continue;
            }
        };

        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let new_lines = apply_text_edits_to_lines(&lines, &fe.edits);

        if new_lines == lines {
            results.push(FileEditResult::Noop {
                path: fe.path.clone(),
            });
            continue;
        }

        if dry_run {
            results.push(FileEditResult::Ok {
                path: fe.path.clone(),
                edits_applied: fe.edits.len(),
                dry_run: true,
            });
        } else {
            let new_content = new_lines.join("\n");
            let new_content = if content.ends_with('\n') && !new_content.ends_with('\n') {
                format!("{new_content}\n")
            } else {
                new_content
            };

            match std::fs::write(&fe.path, &new_content) {
                Ok(()) => results.push(FileEditResult::Ok {
                    path: fe.path.clone(),
                    edits_applied: fe.edits.len(),
                    dry_run: false,
                }),
                Err(e) => results.push(FileEditResult::Error {
                    path: fe.path.clone(),
                    message: format!("write error: {e}"),
                }),
            }
        }
    }

    EditSummary {
        files_modified: results
            .iter()
            .filter(|r| matches!(r, FileEditResult::Ok { .. }))
            .count(),
        files: results,
    }
}

// ── Format preview ───────────────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) fn format_preview(preview: &HashMap<PathBuf, (Vec<String>, Vec<String>)>) -> String {
    let mut out = String::new();
    for (path, (old_lines, new_lines)) in preview {
        out.push_str(&format!("--- {}\n", path.display()));
        out.push_str(&format!("+++ {}\n", path.display()));
        let max_lines = old_lines.len().max(new_lines.len());
        for i in 0..max_lines {
            let old = old_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            let new = new_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            if old != new {
                out.push_str(&format!("-{old}\n"));
                out.push_str(&format!("+{new}\n"));
            }
        }
        out.push('\n');
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    fn te(line: u32, col: u32, end_line: u32, end_col: u32, new_text: &str) -> TextEdit {
        TextEdit {
            range: Range {
                start: Position::new(line, col),
                end: Position::new(end_line, end_col),
            },
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn single_line_replacement() {
        let lines = vec!["hello world".to_string()];
        let edits = vec![te(0, 0, 0, 5, "goodbye")];
        assert_eq!(
            apply_text_edits_to_lines(&lines, &edits),
            vec!["goodbye world"]
        );
    }

    #[test]
    fn multi_line_insertion() {
        let lines = vec!["line1".to_string(), "line3".to_string()];
        let edits = vec![te(1, 0, 1, 5, "line2")];
        let result = apply_text_edits_to_lines(&lines, &edits);
        assert_eq!(result, vec!["line1", "line2"]);
    }

    #[test]
    fn reverse_order_edits() {
        let lines: Vec<String> = vec!["aaa", "bbb", "ccc"]
            .into_iter()
            .map(String::from)
            .collect();
        let edits = vec![te(0, 0, 0, 3, "AAA"), te(2, 0, 2, 3, "CCC")];
        let result = apply_text_edits_to_lines(&lines, &edits);
        assert_eq!(result, vec!["AAA", "bbb", "CCC"]);
    }

    #[test]
    fn path_under_root_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(path_is_under_root(&sub, dir.path()));
    }

    #[test]
    fn path_outside_root_invalid() {
        assert!(!path_is_under_root(
            Path::new("/other/main.kt"),
            Path::new("/workspace"),
        ));
    }
}
