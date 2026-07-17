//! Type hierarchy helpers — find_implementors preserved for tests.
//!
//! The `implementations` and `subclasses` CLI commands were merged into
//! `type hierarchy` (see Phase 42 CLI reorganization).

use std::collections::HashSet;
use std::sync::Arc;

use serde::Serialize;

use crate::indexer::Indexer;

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)] // used by tests only
pub(crate) struct InheritNode {
    name: String,
    kind: String,
    file: String,
    line: u32,
    children: Vec<InheritNode>,
}

// (run_implementations, run_subclasses — removed; merged into run_type_hierarchy
//  as part of Phase 42 CLI reorganization.)

#[allow(dead_code)] // used by tests only
pub(crate) fn find_implementors(
    super_name: &str,
    index: &Arc<Indexer>,
    depth: u32,
    visited: &mut HashSet<String>,
) -> Vec<InheritNode> {
    if depth == 0 || !visited.insert(super_name.to_string()) {
        return vec![];
    }

    let next_depth = depth.saturating_sub(1);
    let mut children = Vec::new();

    if let Some(locations) = index.subtypes.get(super_name) {
        let locs: Vec<_> = locations.value().clone();
        for loc in locs {
            children.push(InheritNode {
                name: super_name.to_string(),
                kind: "class".to_string(),
                file: loc.uri.to_string(),
                line: loc.range.start.line,
                children: find_implementors(super_name, index, next_depth, visited),
            });
        }
    }

    children
}

// (output_inherit_tree, print_node — removed; only used by dead run_implementations/run_subclasses.)

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    fn make_index(subtypes: Vec<(&str, &str)>) -> Arc<Indexer> {
        let index = Indexer::new();
        for (_sub_file, super_name) in subtypes {
            let uri = temp_uri("test");
            let loc = Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
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
        assert!(children.iter().any(|c| c.name == "Repository"));
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
