//! Inheritance graph CLI commands: `implementations` and `subclasses`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::indexer::Indexer;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InheritNode {
    name: String,
    kind: String,
    file: String,
    line: u32,
    col: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<InheritNode>,
}

pub(crate) async fn run_implementations(name: &str, depth: u32, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let root_node = InheritNode {
        name: name.to_string(),
        kind: "interface".to_string(),
        file: String::new(),
        line: 0,
        col: 0,
        children: find_implementors(name, &index, depth, &mut HashSet::new()),
    };

    output_inherit_tree(&root_node, json);
}

pub(crate) async fn run_subclasses(name: &str, depth: u32, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let root_node = InheritNode {
        name: name.to_string(),
        kind: "class".to_string(),
        file: String::new(),
        line: 0,
        col: 0,
        children: find_implementors(name, &index, depth, &mut HashSet::new()),
    };

    output_inherit_tree(&root_node, json);
}

pub(crate) fn find_implementors(
    super_name: &str,
    index: &Arc<Indexer>,
    depth: u32,
    visited: &mut HashSet<String>,
) -> Vec<InheritNode> {
    if depth == 0 || visited.contains(super_name) {
        return vec![];
    }
    visited.insert(super_name.to_string());

    let mut children = Vec::new();
    if let Some(locs) = index.subtypes.get(super_name) {
        let next_depth = if depth == 1 { 0 } else { depth - 1 };
        for loc in locs.iter() {
            let file_str = loc.uri.to_file_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| loc.uri.path().to_string());
            let class_name = std::path::Path::new(&file_str)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            children.push(InheritNode {
                name: class_name.clone(),
                kind: "class".to_string(),
                file: file_str,
                line: loc.range.start.line + 1,
                col: loc.range.start.character + 1,
                children: find_implementors(&class_name, index, next_depth, visited),
            });
        }
    }
    children
}

fn output_inherit_tree(root: &InheritNode, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(root).expect("serialize JSON")
        );
    } else {
        print_node(root, 0);
    }
}

fn print_node(node: &InheritNode, indent: usize) {
    let prefix = "  ".repeat(indent);
    if node.line > 0 {
        println!(
            "{}├─ {} ({}) @ {}:{}",
            prefix, node.name, node.kind, node.file, node.line
        );
    } else {
        println!("{}{} ({})", prefix, node.name, node.kind);
    }
    for child in &node.children {
        print_node(child, indent + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    fn make_index(subtypes: Vec<(&str, &str)>) -> Arc<Indexer> {
        let index = Indexer::new();
        for (sub_file, super_name) in subtypes {
            let uri = temp_uri(sub_file);
            let loc = Location {
                uri: uri.clone(),
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 0 },
                },
            };
            index
                .subtypes
                .entry(super_name.to_string())
                .or_default()
                .push(loc);
        }
        Arc::new(index)
    }

    /// Platform-independent URI for tests.
    fn temp_uri(name: &str) -> Url {
        Url::parse(&format!("file:///{name}.kt")).unwrap()
    }

    #[test]
    fn find_implementations() {
        let index = make_index(vec![
            ("AuthRepository", "Repository"),
            ("UserRepository", "Repository"),
            ("MockRepository", "AuthRepository"),
        ]);
        let children = find_implementors("Repository", &index, 2, &mut HashSet::new());
        assert_eq!(children.len(), 2);
        assert!(children.iter().any(|c| c.name == "AuthRepository"));
        assert!(children.iter().any(|c| c.name == "UserRepository"));
    }

    #[test]
    fn depth_limit() {
        let index = make_index(vec![
            ("Dog", "Animal"),
            ("Cat", "Animal"),
            ("Poodle", "Dog"),
        ]);
        let children = find_implementors("Animal", &index, 1, &mut HashSet::new());
        assert_eq!(children.len(), 2);
        for c in &children {
            assert!(c.children.is_empty());
        }
    }
}
