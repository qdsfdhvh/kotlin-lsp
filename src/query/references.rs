//! Name-based CLI references: existing rg scoping discovers files; CST identifiers
//! supply exact occurrences and UTF-16 ranges (never textual comments/strings).
use std::collections::BTreeSet;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::indexer::Indexer;
use crate::queries::{
    KIND_IDENTIFIER, KIND_IMPORT_DECL, KIND_IMPORT_HEADER, KIND_INTERP_IDENT, KIND_PACKAGE_DECL,
    KIND_PACKAGE_HEADER, KIND_SIMPLE_IDENT, KIND_TYPE_IDENT,
};

pub(crate) fn reference_locations(
    index: &Indexer,
    name: &str,
    include_imports: bool,
) -> Vec<Location> {
    if name.is_empty() {
        return Vec::new();
    }
    let root = index
        .workspace_root
        .read()
        .expect("workspace_root lock")
        .clone();
    let candidates: BTreeSet<String> = if let Some(root) = root {
        let Ok(root) = root.canonicalize() else {
            return Vec::new();
        };
        let Ok(uri) = Url::from_file_path(&root) else {
            return Vec::new();
        };
        let sources = index
            .workspace_source_roots
            .read()
            .expect("workspace_source_roots lock")
            .clone();
        let decl_files: Vec<String> = index
            .definition_locations(name)
            .iter()
            .filter_map(|loc| loc.uri.to_file_path().ok())
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let locations = if include_imports {
            // Normal refs deliberately skips imports. An explicit import filter
            // must discover import-only files too, using the same rg scope.
            crate::rg::rg_word_search(name, &root, &sources)
        } else {
            let request = crate::rg::RgSearchRequest::new(
                name,
                None,
                None,
                Some(&root),
                true,
                &uri,
                &decl_files,
            )
            .with_source_paths(&sources);
            crate::rg::rg_find_references(&request, None)
        };
        locations
            .into_iter()
            .map(|loc| loc.uri.to_string())
            .collect()
    } else {
        // Content-only engines (no workspace scan) already have their candidate set.
        index
            .files
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    };
    let mut results = Vec::new();
    for candidate in candidates {
        let Ok(uri) = Url::parse(&candidate) else {
            continue;
        };
        // The native path is best-effort: content-only engines (and tests)
        // carry URIs without a native path — `file:///test/…` has no drive
        // letter on Windows — so in-memory lines must stay reachable.
        let native_path = uri.to_file_path().ok();
        let source = native_path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .or_else(|| {
                index
                    .mem_lines_for(&candidate)
                    .map(|lines| lines.join("\n"))
            });
        let Some(source) = source else {
            continue;
        };
        let path_label = native_path
            .as_deref()
            .and_then(|path| path.to_str())
            .unwrap_or(candidate.as_str());
        let language = match crate::Language::from_path(path_label) {
            crate::Language::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
            crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
            crate::Language::Swift => tree_sitter_swift::LANGUAGE.into(),
        };
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            continue;
        }
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if matches!(node.kind(), KIND_PACKAGE_HEADER | KIND_PACKAGE_DECL)
                || (!include_imports
                    && matches!(node.kind(), KIND_IMPORT_HEADER | KIND_IMPORT_DECL))
            {
                continue;
            }
            if matches!(
                node.kind(),
                KIND_SIMPLE_IDENT | KIND_TYPE_IDENT | KIND_IDENTIFIER | KIND_INTERP_IDENT
            ) && node.utf8_text(source.as_bytes()).ok() == Some(name)
            {
                let point = node.start_position();
                if let Some(prefix) = lines
                    .get(point.row)
                    .and_then(|line| line.get(..point.column))
                {
                    let start =
                        Position::new(point.row as u32, prefix.encode_utf16().count() as u32);
                    let end = Position::new(
                        start.line,
                        start.character + name.encode_utf16().count() as u32,
                    );
                    results.push(Location {
                        uri: uri.clone(),
                        range: Range::new(start, end),
                    });
                }
            }
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
    }
    sort_locations(&mut results);
    results
}

pub(crate) fn sort_locations(locations: &mut Vec<Location>) {
    locations.sort_by(|a, b| {
        (a.uri.as_str(), a.range.start.line, a.range.start.character).cmp(&(
            b.uri.as_str(),
            b.range.start.line,
            b.range.start.character,
        ))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range.start == b.range.start);
}
