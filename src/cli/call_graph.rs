//! Call graph helpers — tree-building utilities preserved for potential reuse.
//!
//! The `callers` and `callees` CLI commands were merged into `call hierarchy`
//! (see Phase 42 CLI reorganization). These internal helpers were moved here
//! and are kept for tests; the dead top-level functions were removed.

use std::collections::HashSet;
use std::sync::Arc;

/// Internal type for callee tree building.
#[derive(Debug, Clone, serde::Serialize)]
#[allow(dead_code)]
struct CallNode {
    name: String,
    kind: String,
    file: String,
    line: u32,
    col: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<CallNode>,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
/// Extract the callee name from a `call_expression` node.
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "call_graph_tests.rs"]
mod tests;
