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

    /// Build an AI-friendly summary for a symbol, combining identity,
    /// signature, documentation, and all relationship edges.
    pub(crate) fn build_summary(&self, name: &str) -> Option<crate::ai_summary::AISummary> {
        let locs = self.definition_locations(name);
        let loc = locs.first()?;

        let fd = self.file_data(&loc.uri)?;
        let sym = fd.symbols.iter().find(|s| s.name == name)?;

        let graph = self.graph();

        // Collect members for classes/interfaces
        let class_kinds = ["class", "interface", "object", "enum", "struct"];
        let members: Vec<crate::ai_summary::MemberInfo> =
            if class_kinds.contains(&format!("{:?}", sym.kind).to_lowercase().as_str()) {
                fd.symbols
                    .iter()
                    .filter(|s| {
                        s.name != name
                            && s.selection_range.start.line >= sym.selection_range.start.line
                            && (s.selection_range.end < sym.selection_range.end
                                || sym.selection_range.end.line == 0)
                    })
                    .map(|s| crate::ai_summary::MemberInfo {
                        name: s.name.clone(),
                        kind: format!("{:?}", s.kind).to_lowercase(),
                        signature: Some(s.detail.clone()).filter(|d| !d.is_empty()),
                    })
                    .collect()
            } else {
                Vec::new()
            };

        let callers: Vec<String> = graph
            .callers_of(name)
            .into_iter()
            .map(|(_, caller)| caller)
            .collect();
        let callees: Vec<String> = graph
            .callees_of(name)
            .into_iter()
            .map(|(_, callee)| callee)
            .collect();
        let importers: Vec<String> = graph
            .importers_of(name)
            .into_iter()
            .map(|(file, _)| file)
            .collect();
        let supertypes: Vec<String> = self
            .supertypes_of(name)
            .into_iter()
            .map(|(sup, _)| sup)
            .collect();
        let subtypes: Vec<String> = self
            .subtypes_of(name)
            .into_iter()
            .map(|loc| {
                self.file_data(&loc.uri)
                    .and_then(|fd| {
                        fd.symbols
                            .iter()
                            .find(|s| s.selection_range == loc.range)
                            .map(|s| s.name.clone())
                    })
                    .unwrap_or_else(|| loc.uri.to_string())
            })
            .collect();

        Some(crate::ai_summary::AISummary {
            name: name.to_string(),
            kind: format!("{:?}", sym.kind).to_lowercase(),
            package: fd.package.clone(),
            file: loc
                .uri
                .to_file_path()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            line: loc.range.start.line + 1,
            visibility: Some(format!("{:?}", sym.visibility).to_lowercase()),
            signature: Some(sym.detail.clone()).filter(|d| !d.is_empty()),
            doc: sym.documentation.clone(),
            deprecated: sym.deprecated,
            members,
            supertypes,
            subtypes,
            callers,
            callees,
            importers,
        })
    }
}
