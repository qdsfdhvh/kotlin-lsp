//! Call graph CLI commands: `callers` and `callees`.
//!
//! Unlike the existing `call-hierarchy` command (which targets LSP protocol shapes),
//! these commands return **tree**-structured output optimised for AI agents:
//! depth-limited call chains instead of flat lists of locations.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use tower_lsp::lsp_types::Url;

use crate::indexer::Indexer;

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
struct CallNode {
    name: String,
    kind: String,
    file: String,
    line: u32,
    col: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<CallNode>,
}

// ── Entry points ────────────────────────────────────────────────────────────

pub(crate) async fn run_callers(file: &Path, line: u32, col: u32, depth: u32, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, file);
    let index = crate::cli::run::build_index(&root, false).await;
    let uri = Url::from_file_path(file).expect("valid file path");

    let word = extract_word_at_position(&index, &uri, line, col);
    if word.is_empty() {
        eprintln!("No symbol at cursor");
        std::process::exit(1);
    }

    let line = normalize_line_1(line);
    let root_node = CallNode {
        name: word.clone(),
        kind: "function".to_string(),
        file: file.display().to_string(),
        line,
        col,
        children: find_callers_tree(&word, &index, &root, depth, &mut HashSet::new()),
    };

    output_call_tree(&root_node, json);
}

pub(crate) async fn run_callees(file: &Path, line: u32, col: u32, depth: u32, json: bool) {
    use crate::indexer::SymbolGraph;
    let root = crate::cli::run::resolve_root_for_file(None, file);
    let index = crate::cli::run::build_index(&root, false).await;
    let uri = Url::from_file_path(file).expect("valid file path");

    let word = extract_word_at_position(&index, &uri, line, col);
    if word.is_empty() {
        eprintln!("No symbol at cursor");
        std::process::exit(1);
    }

    let graph = SymbolGraph::new(&index);
    let line = normalize_line_1(line);
    let root_node = CallNode {
        name: word.clone(),
        kind: "function".to_string(),
        file: file.display().to_string(),
        line,
        col,
        children: find_callees_from_graph(&word, &graph, &index, depth, &mut HashSet::new()),
    };
    output_call_tree(&root_node, json);
}

// ── Helper: get tree-sitter Language from Language enum ──────────────────────

fn ts_lang(lang: crate::Language) -> tree_sitter::Language {
    match lang {
        crate::Language::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
        crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
        crate::Language::Swift => tree_sitter_swift::LANGUAGE.into(),
    }
}

// ── Caller tree building (edge-index based) ─────────────────────────────────

/// Find callers using the pre-built call edge index.
/// The edge index maps callee_name → [(caller_file, caller_name)],
/// built during workspace indexing. No ripgrep or tree-sitter re-parse needed.
fn find_callers_tree(
    name: &str,
    index: &Arc<Indexer>,
    _project_root: &Path,
    depth: u32,
    visited: &mut HashSet<String>,
) -> Vec<CallNode> {
    if depth == 0 || visited.contains(name) {
        return vec![];
    }
    visited.insert(name.to_string());

    let mut children = Vec::new();
    if let Some(entries) = index.call_edges.get(name) {
        let next_depth = if depth == 1 { 0 } else { depth - 1 };
        for (caller_file, caller_name) in entries.iter() {
            let child = CallNode {
                name: caller_name.clone(),
                kind: "function".to_string(),
                file: caller_file.clone(),
                line: 0,
                col: 0,
                children: find_callers_tree(caller_name, index, _project_root, next_depth, visited),
            };
            children.push(child);
        }
    }
    children
}

/// Extract the callee name from a `call_expression` node.
/// Returns the simple identifier or the last segment of a navigation expression.
fn extract_callee_name(call_expr: &tree_sitter::Node, source: &str) -> String {
    let mut cursor = call_expr.walk();
    for child in call_expr.children(&mut cursor) {
        match child.kind() {
            "simple_identifier" | "identifier" => {
                return child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            }
            "navigation_expression" => {
                let mut sub = child.walk();
                let children: Vec<_> = child.children(&mut sub).collect();
                if let Some(last) = children.last() {
                    let raw = last.utf8_text(source.as_bytes()).unwrap_or("");
                    return raw.trim_start_matches('.').to_string();
                }
            }
            _ => {}
        }
    }
    String::new()
}

// ── Callee tree building ────────────────────────────────────────────────────

fn find_callees_tree(
    name: &str,
    file: &Path,
    line: u32,
    index: &Arc<Indexer>,
    depth: u32,
    visited: &mut HashSet<String>,
) -> Vec<CallNode> {
    if depth == 0 || !visited.insert(name.to_string()) {
        return vec![];
    }

    let mut callees: Vec<CallNode> = Vec::new();

    let Ok(content) = std::fs::read_to_string(file) else {
        return callees;
    };

    let lang = crate::Language::from_path(file.to_str().unwrap_or(""));
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang(lang)).ok();
    let Some(tree) = parser.parse(&content, None) else {
        return callees;
    };

    let root_node = tree.root_node();

    // Find the function declaration for `name` at approximate position.
    let Some(decl_node) = find_function_decl_near(root_node, line.saturating_sub(1), &content)
    else {
        return callees;
    };

    // Collect all call expressions within this function body.
    let call_names = collect_callee_names(&decl_node, &content);

    for callee_name in call_names {
        if callee_name.is_empty() || is_keyword(&callee_name) {
            continue;
        }

        // Look up the declaration in the index.
        let file_uri =
            Url::from_file_path(file).unwrap_or_else(|_| Url::parse("file:///").unwrap());
        let decl_locs = index.find_definition_qualified(&callee_name, None, &file_uri);

        let (callee_file, callee_line, callee_col) = if let Some(loc) = decl_locs.first() {
            (
                loc.uri
                    .to_file_path()
                    .unwrap_or_else(|_| file.to_path_buf())
                    .display()
                    .to_string(),
                loc.range.start.line + 1,
                loc.range.start.character + 1,
            )
        } else {
            (file.display().to_string(), 0u32, 0u32)
        };

        let child_depth = depth.saturating_sub(1);

        let mut callee_visited = visited.clone();
        let callee_path = if let Ok(p) = Url::parse(&format!("file://{}", callee_file)) {
            p.to_file_path().unwrap_or_else(|_| file.to_path_buf())
        } else {
            file.to_path_buf()
        };

        let children = if child_depth > 0 {
            find_callees_tree(
                &callee_name,
                &callee_path,
                callee_line,
                index,
                child_depth,
                &mut callee_visited,
            )
        } else {
            vec![]
        };

        callees.push(CallNode {
            name: callee_name,
            kind: "function".to_string(),
            file: callee_file,
            line: callee_line,
            col: callee_col,
            children,
        });
    }

    callees
}

/// Build a callee tree using the pre-built call edge index (graph-based, no re-parse).
fn find_callees_from_graph(
    name: &str,
    graph: &crate::indexer::SymbolGraph,
    index: &Arc<crate::indexer::Indexer>,
    depth: u32,
    visited: &mut HashSet<String>,
) -> Vec<CallNode> {
    if depth == 0 || !visited.insert(name.to_string()) {
        return vec![];
    }

    let callees = graph.callees_of(name);
    let next_depth = depth.saturating_sub(1);

    callees
        .into_iter()
        .map(|(callee_file, callee_name)| {
            let (file, line, col) = if let Some(locs) = index.definitions.get(&callee_name) {
                if let Some(loc) = locs.first() {
                    (
                        loc.uri.to_string(),
                        loc.range.start.line + 1,
                        loc.range.start.character + 1,
                    )
                } else {
                    (callee_file.clone(), 0, 0)
                }
            } else {
                (callee_file.clone(), 0, 0)
            };

            CallNode {
                name: callee_name.clone(),
                kind: "function".to_string(),
                file,
                line,
                col,
                children: find_callees_from_graph(&callee_name, graph, index, next_depth, visited),
            }
        })
        .collect()
}

/// Find a function declaration node near the given line.
fn find_function_decl_near<'a>(
    root: tree_sitter::Node<'a>,
    line: u32,
    _source: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    let mut best: Option<tree_sitter::Node> = None;
    let mut best_dist: u32 = u32::MAX;

    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "function_declaration" | "method_declaration" | "constructor_declaration"
        ) {
            let start_line = node.start_position().row as u32;
            let dist = if start_line <= line {
                line.abs_diff(start_line)
            } else {
                start_line.abs_diff(line)
            };
            if dist < best_dist {
                best_dist = dist;
                best = Some(node);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    best
}

/// Collect all callee names (function calls) within a function declaration node.
fn collect_callee_names(decl: &tree_sitter::Node, source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut stack: Vec<tree_sitter::Node> = vec![*decl];
    let mut seen = HashSet::new();

    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            let name = extract_callee_name(&node, source);
            if !name.is_empty() && seen.insert(name.clone()) {
                names.push(name);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    names
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn extract_word_at_position(index: &Arc<Indexer>, uri: &Url, line: u32, col: u32) -> String {
    let lines = index.mem_lines_for(uri.as_str());
    lines
        .as_ref()
        .and_then(|l| {
            let li = line.saturating_sub(1) as usize;
            l.get(li).map(|ln| {
                crate::StrExt::word_at_utf16_col(ln.as_str(), col.saturating_sub(1) as usize)
            })
        })
        .unwrap_or_default()
}

/// Normalise a 1-based line to ensure it's non-zero (default to 1).
fn normalize_line_1(line: u32) -> u32 {
    if line == 0 {
        1
    } else {
        line
    }
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
            | "typealias"
            | "continue"
            | "break"
    )
}

// ── Output ──────────────────────────────────────────────────────────────────

fn output_call_tree(root: &CallNode, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(root).expect("serialize JSON")
        );
    } else {
        print_call_tree(root, 0);
    }
}

fn print_call_tree(node: &CallNode, indent: usize) {
    let prefix = "  ".repeat(indent);
    println!(
        "{prefix}- {name} ({kind}) @ {file}:{line}:{col}",
        name = node.name,
        kind = node.kind,
        file = node.file,
        line = node.line,
        col = node.col,
    );
    for child in &node.children {
        print_call_tree(child, indent + 1);
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "call_graph_tests.rs"]
mod tests;

#[allow(dead_code)]
fn extract_function_name_and_pos(
    decl: &tree_sitter::Node,
    source: &str,
) -> Option<(String, u32, u32)> {
    let start = decl.start_position();
    let line = start.row as u32 + 1;
    let col = start.column as u32 + 1;

    let mut cursor = decl.walk();
    for child in decl.children(&mut cursor) {
        if child.kind() == "simple_identifier" {
            let name = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            return Some((name, line, col));
        }
    }
    None
}
