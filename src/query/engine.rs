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

    pub(crate) fn find_definitions(&self, name: &str) -> Vec<Location> {
        self.index
            .definitions
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
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

    pub(crate) fn callers_of(&self, name: &str) -> Vec<(String, String)> {
        self.graph().callers_of(name)
    }

    pub(crate) fn callees_of(&self, name: &str) -> Vec<(String, String)> {
        self.graph().callees_of(name)
    }

    pub(crate) fn supertypes_of(&self, name: &str) -> Vec<(String, String)> {
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
}
