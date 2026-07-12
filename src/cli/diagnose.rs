//! Call-argument diagnostics for check --diagnose.
use crate::indexer::Indexer;
use std::path::Path;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct CallArgDiagnostic {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
    pub severity: String,
}

pub(crate) fn run_diagnose(files: &[PathBuf], idx: &Arc<Indexer>, json: bool) {
    for file in files {
        let diagnostics = diagnose_call_args(file, idx);
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&diagnostics).unwrap_or_default()
            );
        } else {
            for d in &diagnostics {
                println!("{}:{}:{} {}", d.file, d.line, d.col, d.message);
            }
        }
    }
}

pub(crate) fn diagnose_call_args(file: &Path, idx: &Arc<Indexer>) -> Vec<CallArgDiagnostic> {
    let Ok(source) = std::fs::read_to_string(file) else {
        return vec![];
    };
    let Ok(uri) = Url::from_file_path(file) else {
        return vec![];
    };

    let lang = crate::Language::from_path(file.to_str().unwrap_or(""));
    let mut parser = tree_sitter::Parser::new();
    let ts_lang = match lang {
        crate::Language::Kotlin => tree_sitter_kotlin::LANGUAGE.into(),
        crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
        _ => return vec![],
    };
    if parser.set_language(&ts_lang).is_err() {
        return vec![];
    }
    let Some(tree) = parser.parse(&source, None) else {
        return vec![];
    };

    let mut diagnostics = Vec::new();
    let root = tree.root_node();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            let (callee, arg_count) = extract_call_info(&node, &source);
            if !callee.is_empty() && !is_keyword(&callee) {
                if let Some((expected, detail)) = resolve_arg_count(idx, &callee, &uri) {
                    if arg_count != expected && expected > 0 {
                        let s = node.start_position();
                        diagnostics.push(CallArgDiagnostic {
                            file: file.display().to_string(),
                            line: s.row as u32 + 1,
                            col: s.column as u32 + 1,
                            message: format!(
                                "{callee}() expects {expected} args, got {arg_count} ({detail})"
                            ),
                            severity: "warning".to_string(),
                        });
                    }
                }
            }
        }
        for child in children(&node) {
            stack.push(child);
        }
    }
    diagnostics
}

fn extract_call_info(node: &tree_sitter::Node, source: &str) -> (String, usize) {
    let mut callee = String::new();
    let mut count = 0usize;
    for child in children(node) {
        match child.kind() {
            "simple_identifier" | "identifier" => {
                callee = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            }
            "navigation_expression" => {
                if let Some(last) = children(&child).last() {
                    callee = last.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                }
            }
            "value_arguments" => {
                let t = child.utf8_text(source.as_bytes()).unwrap_or("");
                if !t
                    .trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace())
                    .is_empty()
                {
                    count = t.chars().filter(|&c| c == ',').count() + 1;
                }
            }
            _ => {}
        }
    }
    (callee, count)
}

fn resolve_arg_count(idx: &Arc<Indexer>, callee: &str, _uri: &Url) -> Option<(usize, String)> {
    let locs = idx.definition_locations(callee);
    if let Some(loc) = locs.first() {
        let fp = loc.uri.to_file_path().ok()?;
        let src = std::fs::read_to_string(&fp).ok()?;
        let lines: Vec<&str> = src.lines().collect();
        let line = loc.range.start.line as usize;
        let lt = lines.get(line)?;
        let sig = {
            let open = lt.find('(')?;
            let mut depth = 0;
            let mut cp = open;
            for (i, c) in lt[open..].chars().enumerate() {
                if c == '(' {
                    depth += 1;
                }
                if c == ')' {
                    depth -= 1;
                }
                if depth == 0 && i > 0 {
                    cp = open + i;
                    break;
                }
            }
            let params = lt[open + 1..cp].trim();
            if params.is_empty() {
                0
            } else {
                params.chars().filter(|&c| c == ',').count() + 1
            }
        };
        return Some((sig, lt.trim().to_string()));
    }
    None
}

fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "else"
            | "when"
            | "for"
            | "while"
            | "do"
            | "return"
            | "try"
            | "catch"
            | "throw"
            | "class"
            | "fun"
            | "val"
            | "var"
            | "this"
            | "super"
            | "true"
            | "false"
            | "null"
            | "is"
            | "as"
            | "in"
            | "out"
            | "object"
            | "interface"
            | "enum"
    )
}

fn children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}
