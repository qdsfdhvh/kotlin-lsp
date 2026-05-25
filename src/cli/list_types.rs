//! CLI `list-types` subcommand — project-level type listing.
//!
//! Lists all known types in the workspace index with optional filters.

use std::sync::Arc;

use serde::Serialize;

#[derive(Debug, Serialize)]
struct TypeEntry {
    name: String,
    kind: String,
    module: String,
}

pub(crate) async fn run_list_types(
    root: &std::path::Path,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
    limit: usize,
    json: bool,
) {
    use crate::indexer::{Indexer, NoopReporter};

    let idx = {
        let idx = Arc::new(Indexer::new());
        Arc::clone(&idx)
            .index_workspace_full(root, Arc::new(NoopReporter))
            .await;
        idx
    };

    let mut entries: Vec<TypeEntry> = Vec::new();

    for file_entry in idx.files.iter() {
        let uri_str = file_entry.key();
        let data = file_entry.value();

        let module = module_name(uri_str, module_filter);
        if module.is_none() && module_filter.is_some() {
            continue;
        }

        for sym in &data.symbols {
            if let Some(kf) = kind_filter {
                let k = kind_str(sym.kind);
                if k != kf {
                    continue;
                }
            }
            entries.push(TypeEntry {
                name: sym.name.clone(),
                kind: kind_str(sym.kind).to_owned(),
                module: module.clone().unwrap_or_default(),
            });
        }

        if entries.len() >= limit.saturating_mul(2) {
            break;
        }
    }

    entries.dedup_by_key(|e| e.name.clone());
    entries.truncate(limit);

    if json {
        let output = serde_json::json!({
            "count": entries.len(),
            "types": entries,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        for e in &entries {
            if e.module.is_empty() {
                println!("{} [{}]", e.name, e.kind);
            } else {
                println!("{} [{}] ({})", e.name, e.kind, e.module);
            }
        }
    }
}

fn kind_str(k: tower_lsp::lsp_types::SymbolKind) -> &'static str {
    match k {
        tower_lsp::lsp_types::SymbolKind::CLASS => "class",
        tower_lsp::lsp_types::SymbolKind::INTERFACE => "interface",
        tower_lsp::lsp_types::SymbolKind::ENUM => "enum",
        tower_lsp::lsp_types::SymbolKind::FUNCTION => "fun",
        tower_lsp::lsp_types::SymbolKind::METHOD => "method",
        tower_lsp::lsp_types::SymbolKind::PROPERTY => "val",
        tower_lsp::lsp_types::SymbolKind::VARIABLE => "var",
        tower_lsp::lsp_types::SymbolKind::CONSTANT => "const",
        tower_lsp::lsp_types::SymbolKind::OBJECT => "object",
        tower_lsp::lsp_types::SymbolKind::STRUCT => "data class",
        _ => "other",
    }
}

fn module_name(uri_str: &str, _filter: Option<&str>) -> Option<String> {
    // Extract module-like path segment
    let path = uri_str.strip_prefix("file://")?;
    let segments: Vec<&str> = path.split('/').collect();
    // Look for "src" segment and take the next directory as module
    if let Some(src_pos) = segments.iter().position(|&s| s == "src") {
        if src_pos + 1 < segments.len() {
            return Some(segments[src_pos + 1].to_owned());
        }
    }
    None
}
