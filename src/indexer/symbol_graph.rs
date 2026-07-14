//! Symbol Graph — typed query API for workspace-wide relationships.
//!
//! Thin read-only view over Indexer DashMaps for call, import, override,
//! and supertype relationships. Populated during workspace indexing.

use tower_lsp::lsp_types::Location;

use crate::indexer::Indexer;

pub(crate) type CallerInfo = (String, String);
pub(crate) type CalleeInfo = (String, String);

#[derive(Debug, Clone)]
pub(crate) struct SupertypeInfo {
    pub name: String,
    pub file: String,
}

pub(crate) struct SymbolGraph<'a> {
    index: &'a Indexer,
}

impl<'a> SymbolGraph<'a> {
    pub fn new(index: &'a Indexer) -> Self {
        Self { index }
    }

    /// Returns who calls `fn_name` (callee → callers).
    pub fn callers_of(&self, fn_name: &str) -> Vec<CallerInfo> {
        self.index
            .call_edges
            .get(fn_name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Returns what `fn_name` calls (reverse lookup through call_edges).
    pub fn callees_of(&self, fn_name: &str) -> Vec<CalleeInfo> {
        let mut result: Vec<CalleeInfo> = Vec::new();
        for entry in self.index.call_edges.iter() {
            let callee = entry.key();
            let callers = entry.value();
            for (caller_file, caller_name) in callers.iter() {
                if caller_name == fn_name {
                    result.push((caller_file.clone(), callee.clone()));
                }
            }
        }
        result
    }

    /// Direct supertypes of `name` (classes/interfaces it extends/implements).
    pub fn supertypes_of(&self, name: &str) -> Vec<SupertypeInfo> {
        self.index
            .supertypes_index
            .get(name)
            .map(|v| {
                v.iter()
                    .map(|(sup_name, file)| SupertypeInfo {
                        name: sup_name.clone(),
                        file: file.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Direct subtypes of `name` (classes that extend/implement it).
    pub fn subtypes_of(&self, name: &str) -> Vec<Location> {
        self.index
            .subtypes
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Files that import `fqn`.
    pub fn importers_of(&self, fqn: &str) -> Vec<(String, String)> {
        self.index
            .import_edges
            .get(fqn)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Files that override `method_name`.
    pub fn overrides_of(&self, method_name: &str) -> Vec<(String, String)> {
        self.index
            .override_edges
            .get(method_name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    pub fn stats(&self) -> SymbolGraphStats {
        SymbolGraphStats {
            call_edges: self.index.call_edges.len(),
            import_edges: self.index.import_edges.len(),
            override_edges: self.index.override_edges.len(),
            supertype_edges: self.index.supertypes_index.len(),
            subtype_edges: self.index.subtypes.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SymbolGraphStats {
    pub call_edges: usize,
    pub import_edges: usize,
    pub override_edges: usize,
    pub supertype_edges: usize,
    pub subtype_edges: usize,
}
