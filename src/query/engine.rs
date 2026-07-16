#![allow(dead_code)]
//! Query Engine — unified query API shared by CLI and LSP backend.
//!
//! Wraps `Indexer` and `SymbolGraph` in a single entry point for workspace
//! queries. CLI commands and LSP handlers share the same methods, eliminating
//! duplication between the two code paths.
//!
//! Currently used by: callers, callees commands.
//! Future: migrate remaining CLI commands and LSP handlers.

use std::sync::Arc;

use tower_lsp::lsp_types::{Location, Url};

use crate::indexer::{Indexer, SymbolGraph};
use crate::types::FileData;

#[derive(Debug, Clone)]
pub(crate) struct QueryContext {
    pub(crate) source_uri: Option<Url>,
    pub(crate) cursor_line: Option<u32>,
}

/// The primary query engine — wraps the workspace index and graph.
pub(crate) struct WorkspaceQueryEngine {
    pub(crate) index: Arc<Indexer>,
}

impl WorkspaceQueryEngine {
    pub(crate) fn new(index: Arc<Indexer>) -> Self {
        Self { index }
    }

    pub(crate) fn graph(&self) -> SymbolGraph<'_> {
        SymbolGraph::new(&self.index)
    }

    /// All definition locations for `name`, including JAR-derived definitions.
    pub(crate) fn definition_locations(&self, name: &str) -> Vec<Location> {
        self.index.definition_locations(name)
    }

    /// Definition locations from workspace sources only (no JAR definitions).
    pub(crate) fn find_definitions(&self, name: &str) -> Vec<Location> {
        self.index
            .definitions
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Qualified symbol lookup (e.g. `Foo.bar` with owner="Foo").
    pub(crate) fn find_definition_qualified(
        &self,
        name: &str,
        qualifier: Option<&str>,
        from_uri: &Url,
    ) -> Vec<Location> {
        self.index
            .find_definition_qualified(name, qualifier, from_uri)
    }

    /// In-memory lines for a URI (live or indexed snapshot).
    pub(crate) fn mem_lines_for(&self, uri: &str) -> Option<std::sync::Arc<Vec<String>>> {
        self.index.mem_lines_for(uri)
    }

    pub(crate) fn word_at(&self, uri: &Url, line: u32, col: u32) -> String {
        let lines = self.index.mem_lines_for(uri.as_str());
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

    pub(crate) fn file_data(&self, uri: &Url) -> Option<Arc<FileData>> {
        self.index.files.get(uri.as_str()).map(|v| v.clone())
    }

    /// Fast access to files DashMap (for bulk iteration patterns that
    /// don't have a clean query-engine equivalent yet).
    pub(crate) fn file_by_uri_str(&self, uri_str: &str) -> Option<Arc<FileData>> {
        self.index.files.get(uri_str).map(|v| v.clone())
    }

    pub(crate) fn callers_of(&self, name: &str) -> Vec<(String, String)> {
        self.graph().callers_of(name)
    }

    pub(crate) fn callees_of(&self, name: &str) -> Vec<(String, String)> {
        self.graph().callees_of(name)
    }

    pub(crate) fn supertypes_of(
        &self,
        name: &str,
    ) -> Vec<(String, String, crate::types::SuperKind)> {
        self.index
            .supertypes_index
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    pub(crate) fn subtypes_of(&self, name: &str) -> Vec<Location> {
        self.graph().subtypes_of(name)
    }

    pub(crate) fn all_symbol_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .index
            .definitions
            .iter()
            .map(|e| e.key().clone())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    pub(crate) fn completions(
        &self,
        uri: &Url,
        position: tower_lsp::lsp_types::Position,
        snippets: bool,
    ) -> (Vec<tower_lsp::lsp_types::CompletionItem>, bool) {
        self.index.completions(uri, position, snippets)
    }

    pub(crate) fn word_and_qualifier_at(
        &self,
        uri: &Url,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<(String, Option<String>)> {
        self.index.word_and_qualifier_at(uri, position)
    }

    pub(crate) fn is_library_uri(&self, uri: &Url) -> bool {
        self.index.is_library_uri(uri)
    }

    pub(crate) fn file_symbols(&self, uri: &Url) -> Vec<crate::types::SymbolEntry> {
        self.index.file_symbols(uri)
    }

    pub(crate) fn live_doc(&self, uri: &Url) -> Option<Arc<crate::indexer::LiveDoc>> {
        self.index.live_doc(uri)
    }

    pub(crate) fn enclosing_class_at(&self, uri: &Url, line: u32) -> Option<String> {
        self.index.enclosing_class_at(uri, line)
    }

    pub(crate) fn get_file(&self, uri_str: &str) -> Option<Arc<FileData>> {
        self.index.get_file(uri_str)
    }
}
