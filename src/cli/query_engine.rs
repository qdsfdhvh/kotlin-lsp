//! Unified query interface — single API surface for CLI, LSP, and future MCP.
//!
//! All symbol queries flow through a `QueryEngine` implementation.
//! Currently `IndexQueryEngine` wraps the indexer; a future LSP refactoring
//! would route LSP handlers through the same engine.

use std::path::Path;
use std::sync::Arc;

use crate::cli::args::ResultFilters;
use crate::cli::output::CliResult;
use crate::indexer::Indexer;
use crate::types::SymbolEntry;
use tower_lsp::lsp_types::Location;

/// Unified query engine trait. All consumers (CLI, LSP, MCP) call these methods.
pub(crate) trait QueryEngine: Send + Sync {
    fn definitions(&self, name: &str) -> Vec<Location>;
    fn references(&self, name: &str) -> Vec<Location>;
    fn find_symbols(&self, name: &str, filters: &ResultFilters) -> Vec<CliResult>;
    fn hover(&self, file: &Path, line: u32, col: u32) -> Option<SymbolEntry>;
    fn summarize(&self, name: &str) -> Option<Vec<SymbolEntry>>;
    fn callers_of(&self, name: &str) -> Vec<(String, String)>;
    fn implementations_of(&self, name: &str) -> Vec<Location>;
    fn all_symbol_names(&self) -> Vec<String>;
    fn importing_files(&self, name: &str) -> Vec<String>;
}

/// Production implementation wrapping the workspace indexer.
pub(crate) struct IndexQueryEngine {
    index: Arc<Indexer>,
}

impl IndexQueryEngine {
    pub(crate) fn new(index: Arc<Indexer>) -> Self {
        Self { index }
    }

    pub(crate) fn get_index(&self) -> &Arc<Indexer> {
        &self.index
    }
}

impl QueryEngine for IndexQueryEngine {
    fn definitions(&self, name: &str) -> Vec<Location> {
        self.index.definition_locations(name)
    }

    fn references(&self, name: &str) -> Vec<Location> {
        self.index.definition_locations(name)
    }

    fn find_symbols(&self, name: &str, filters: &ResultFilters) -> Vec<CliResult> {
        let locs = self.definitions(name);
        if locs.is_empty() {
            return vec![];
        }
        let mut results = Vec::new();
        for loc in &locs {
            let uri_str = loc.uri.to_string();
            for sym_entry in self.index.files.get(&uri_str).iter() {
                for sym in &sym_entry.symbols {
                    if sym.name == name && sym.selection_range.start.line == loc.range.start.line {
                        let file_path = loc.uri.to_file_path().unwrap_or_default();
                        let r = CliResult {
                            name: name.to_string(),
                            kind: format!("{:?}", sym.kind).to_lowercase(),
                            file: file_path.display().to_string(),
                            line: loc.range.start.line + 1,
                            col: loc.range.start.character + 1,
                            module: None,
                            source_set: None,
                            owner: None,
                            relative_path: None,
                            signature: if sym.detail.is_empty() {
                                None
                            } else {
                                Some(sym.detail.clone())
                            },
                            visibility: Some(format!("{:?}", sym.visibility).to_lowercase()),
                            modifiers: if sym.deprecated {
                                Some(vec!["deprecated".into()])
                            } else {
                                None
                            },
                        };
                        // Apply filters
                        // visibility/modifier filters applied by caller
                        results.push(r);
                    }
                }
            }
        }
        if let Some(limit) = filters.limit {
            results.truncate(limit);
        }
        results
    }

    fn hover(&self, file: &Path, line: u32, col: u32) -> Option<SymbolEntry> {
        let uri = tower_lsp::lsp_types::Url::from_file_path(file).ok()?;
        let uri_str = uri.to_string();
        let content = std::fs::read_to_string(file).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = (line as usize).saturating_sub(1);
        let line_text = lines.get(line_idx)?;
        let col_idx = (col as usize).saturating_sub(1).min(line_text.len());
        let before = &line_text[..col_idx];
        let word = before
            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        if word.is_empty() {
            return None;
        }
        self.index
            .files
            .get(&uri_str)
            .and_then(|f| f.symbols.iter().find(|s| s.name == word).cloned())
    }

    fn summarize(&self, name: &str) -> Option<Vec<SymbolEntry>> {
        let locs = self.definitions(name);
        let loc = locs.first()?;
        let uri_str = loc.uri.to_string();
        let file_ref = self.index.files.get(&uri_str)?;
        let symbols: Vec<SymbolEntry> = file_ref
            .symbols
            .iter()
            .filter(|s| s.name == name || s.name.contains(name))
            .cloned()
            .collect();
        if symbols.is_empty() {
            None
        } else {
            Some(symbols)
        }
    }

    fn callers_of(&self, name: &str) -> Vec<(String, String)> {
        self.index
            .call_edges
            .get(name)
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }

    fn implementations_of(&self, name: &str) -> Vec<Location> {
        self.index
            .subtypes
            .get(name)
            .map(|locs| locs.clone())
            .unwrap_or_default()
    }

    fn all_symbol_names(&self) -> Vec<String> {
        self.index
            .definitions
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    fn importing_files(&self, name: &str) -> Vec<String> {
        let mut files = Vec::new();
        for entry in self.index.files.iter() {
            let file_data = entry.value();
            for import in &file_data.imports {
                if import.full_path.contains(name) {
                    files.push(entry.key().clone());
                    break;
                }
            }
        }
        files
    }
}
